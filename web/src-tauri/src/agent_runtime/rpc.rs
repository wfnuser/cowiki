//! Newline-delimited JSON-RPC over a child process's stdio.
//!
//! Codex app-server omits the `jsonrpc` field; ACP agents require
//! `"jsonrpc": "2.0"`. Responses are matched by request id, never by arrival
//! order. The child runs in its own process group, which is the only group a
//! shutdown signals: other Codex/Grok sessions the user runs are untouched.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::fmt;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// Longest accepted JSON line; longer lines are dropped with a diagnostic.
pub const MAX_LINE_BYTES: usize = 8 * 1024 * 1024;
/// Stderr bytes kept for error messages (the newest ones).
pub const MAX_STDERR_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone)]
pub struct LaunchSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    /// The complete environment of the child (it does not inherit CoWiki's).
    pub env: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub enum Incoming {
    Request {
        id: Value,
        method: String,
        params: Value,
    },
    Notification {
        method: String,
        params: Value,
    },
    /// A line that was not valid JSON-RPC or was too long.
    Diagnostic(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum RpcError {
    Remote {
        code: i64,
        message: String,
        data: Option<Value>,
    },
    Timeout(String),
    Closed(String),
    Io(String),
}

impl fmt::Display for RpcError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Remote { message, data, .. } => match data.as_ref().and_then(Value::as_str) {
                Some(detail) if !detail.is_empty() && detail != message => {
                    write!(formatter, "{message} ({detail})")
                }
                _ => formatter.write_str(message),
            },
            Self::Timeout(method) => write!(formatter, "{method} timed out"),
            Self::Closed(reason) => formatter.write_str(reason),
            Self::Io(error) => formatter.write_str(error),
        }
    }
}

/// Outstanding requests keyed by id. Responses can arrive in any order and a
/// late or duplicate response finds nothing.
pub struct PendingRequests<T> {
    entries: HashMap<u64, T>,
}

impl<T> Default for PendingRequests<T> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }
}

impl<T> PendingRequests<T> {
    pub fn insert(&mut self, id: u64, value: T) {
        self.entries.insert(id, value);
    }

    pub fn take(&mut self, id: u64) -> Option<T> {
        self.entries.remove(&id)
    }

    pub fn drain(&mut self) -> Vec<T> {
        self.entries.drain().map(|(_, value)| value).collect()
    }
}

type Reply = mpsc::Sender<Result<Value, RpcError>>;
pub type IncomingHandler = Arc<dyn Fn(Incoming) + Send + Sync>;
pub type ExitHandler = Box<dyn FnOnce(String) + Send>;

struct Shared {
    stdin: Mutex<Option<ChildStdin>>,
    pending: Mutex<PendingRequests<Reply>>,
    /// `Some(reason)` once the child exited and its stdout closed.
    exited: Mutex<Option<String>>,
    exit_signal: Condvar,
    stderr: Mutex<String>,
}

pub struct RpcProcess {
    shared: Arc<Shared>,
    next_id: AtomicU64,
    pid: u32,
    jsonrpc: bool,
    label: String,
}

impl RpcProcess {
    /// Starts the child with a fixed argv (no shell) and begins reading.
    /// `on_incoming` runs on the reader thread: it must not block on
    /// [`RpcProcess::request`] (the reader is what delivers responses).
    pub fn spawn(
        label: &str,
        spec: &LaunchSpec,
        jsonrpc: bool,
        on_incoming: IncomingHandler,
        on_exit: ExitHandler,
    ) -> Result<Arc<Self>, String> {
        let mut command = Command::new(&spec.program);
        command
            .args(&spec.args)
            .current_dir(&spec.cwd)
            .env_clear()
            .envs(spec.env.iter().map(|(key, value)| (key, value)))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut child = command
            .spawn()
            .map_err(|error| format!("could not start {label}: {error}"))?;
        let pid = child.id();
        let stdin = child.stdin.take();
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| format!("{label} has no stdout"))?;
        let stderr = child.stderr.take();
        let shared = Arc::new(Shared {
            stdin: Mutex::new(stdin),
            pending: Mutex::new(PendingRequests::default()),
            exited: Mutex::new(None),
            exit_signal: Condvar::new(),
            stderr: Mutex::new(String::new()),
        });
        if let Some(stderr) = stderr {
            let shared = shared.clone();
            std::thread::spawn(move || read_stderr(stderr, shared));
        }
        {
            let shared = shared.clone();
            let label = label.to_string();
            std::thread::spawn(move || {
                read_stdout(label, stdout, child, shared, on_incoming, on_exit)
            });
        }
        Ok(Arc::new(Self {
            shared,
            next_id: AtomicU64::new(1),
            pid,
            jsonrpc,
            label: label.to_string(),
        }))
    }

    #[cfg(test)]
    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn is_alive(&self) -> bool {
        self.shared
            .exited
            .lock()
            .map(|exited| exited.is_none())
            .unwrap_or(false)
    }

    pub fn exit_reason(&self) -> Option<String> {
        self.shared
            .exited
            .lock()
            .ok()
            .and_then(|exited| exited.clone())
    }

    fn write(&self, mut message: Value) -> Result<(), RpcError> {
        if self.jsonrpc {
            if let Some(object) = message.as_object_mut() {
                object.insert("jsonrpc".into(), Value::String("2.0".into()));
            }
        }
        if let Some(reason) = self.exit_reason() {
            return Err(RpcError::Closed(reason));
        }
        let mut line =
            serde_json::to_vec(&message).map_err(|error| RpcError::Io(error.to_string()))?;
        line.push(b'\n');
        let mut stdin = self
            .shared
            .stdin
            .lock()
            .map_err(|_| RpcError::Io("stdin lock poisoned".into()))?;
        let pipe = stdin
            .as_mut()
            .ok_or_else(|| RpcError::Closed(format!("{} input is closed", self.label)))?;
        pipe.write_all(&line)
            .and_then(|_| pipe.flush())
            .map_err(|error| RpcError::Io(format!("could not write to {}: {error}", self.label)))
    }

    pub fn request(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, RpcError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = mpsc::channel();
        self.shared
            .pending
            .lock()
            .map_err(|_| RpcError::Io("pending lock poisoned".into()))?
            .insert(id, sender);
        if let Err(error) = self.write(json!({"id": id, "method": method, "params": params})) {
            self.forget(id);
            return Err(error);
        }
        match receiver.recv_timeout(timeout) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                self.forget(id);
                Err(RpcError::Timeout(method.to_string()))
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(RpcError::Closed(
                self.exit_reason()
                    .unwrap_or_else(|| format!("{} closed", self.label)),
            )),
        }
    }

    fn forget(&self, id: u64) {
        if let Ok(mut pending) = self.shared.pending.lock() {
            pending.take(id);
        }
    }

    pub fn notify(&self, method: &str, params: Value) -> Result<(), RpcError> {
        self.write(json!({"method": method, "params": params}))
    }

    pub fn respond(&self, id: &Value, result: Value) -> Result<(), RpcError> {
        self.write(json!({"id": id, "result": result}))
    }

    pub fn respond_error(&self, id: &Value, code: i64, message: &str) -> Result<(), RpcError> {
        self.write(json!({"id": id, "error": {"code": code, "message": message}}))
    }

    /// Ends the process group: close stdin, SIGTERM, then SIGKILL after
    /// `grace`. Returns Ok only once the leader exited and no process is left
    /// in its group.
    pub fn shutdown(&self, grace: Duration) -> Result<(), String> {
        if let Ok(mut stdin) = self.shared.stdin.lock() {
            stdin.take();
        }
        signal_group(self.pid, Signal::Term);
        if !self.wait_exit(grace) {
            signal_group(self.pid, Signal::Kill);
            if !self.wait_exit(Duration::from_secs(3)) {
                return Err(format!("{} did not exit after it was stopped", self.label));
            }
        }
        // The leader is gone; commands it started in its group may not be.
        let deadline = Instant::now() + Duration::from_secs(2);
        while group_has_members(self.pid) {
            signal_group(self.pid, Signal::Kill);
            if Instant::now() > deadline {
                return Err(format!(
                    "commands started by {} are still running",
                    self.label
                ));
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        Ok(())
    }

    pub fn wait_exit(&self, timeout: Duration) -> bool {
        let Ok(guard) = self.shared.exited.lock() else {
            return false;
        };
        let deadline = Instant::now() + timeout;
        let mut guard = guard;
        while guard.is_none() {
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            match self.shared.exit_signal.wait_timeout(guard, deadline - now) {
                Ok((next, _)) => guard = next,
                Err(_) => return false,
            }
        }
        true
    }
}

impl Drop for RpcProcess {
    fn drop(&mut self) {
        if self.is_alive() {
            signal_group(self.pid, Signal::Kill);
        }
    }
}

#[derive(Clone, Copy)]
enum Signal {
    Term,
    Kill,
}

#[cfg(unix)]
fn signal_group(pid: u32, signal: Signal) {
    let signal = match signal {
        Signal::Term => libc::SIGTERM,
        Signal::Kill => libc::SIGKILL,
    };
    // The child is its own group leader (process_group(0)), so -pid is the
    // group CoWiki created and nothing else.
    unsafe {
        libc::kill(-(pid as i32), signal);
    }
}

#[cfg(not(unix))]
fn signal_group(pid: u32, _signal: Signal) {
    let _ = Command::new("taskkill")
        .args(["/T", "/F", "/PID", &pid.to_string()])
        .status();
}

#[cfg(unix)]
fn group_has_members(pid: u32) -> bool {
    let result = unsafe { libc::kill(-(pid as i32), 0) };
    if result == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(not(unix))]
fn group_has_members(_pid: u32) -> bool {
    false
}

fn read_stderr(mut stderr: impl Read, shared: Arc<Shared>) {
    let mut buffer = [0_u8; 8192];
    loop {
        match stderr.read(&mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(read) => {
                if let Ok(mut text) = shared.stderr.lock() {
                    text.push_str(&String::from_utf8_lossy(&buffer[..read]));
                    super::contract::keep_tail(&mut text, MAX_STDERR_BYTES);
                }
            }
        }
    }
}

enum Line {
    Complete(Vec<u8>),
    Oversize,
    Eof,
}

fn read_bounded_line(reader: &mut impl BufRead, limit: usize) -> std::io::Result<Line> {
    let mut line = Vec::new();
    let mut oversize = false;
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Ok(if oversize {
                Line::Oversize
            } else if line.is_empty() {
                Line::Eof
            } else {
                Line::Complete(line)
            });
        }
        if let Some(position) = available.iter().position(|byte| *byte == b'\n') {
            if !oversize && line.len() + position <= limit {
                line.extend_from_slice(&available[..position]);
            } else {
                oversize = true;
            }
            reader.consume(position + 1);
            return Ok(if oversize {
                Line::Oversize
            } else {
                Line::Complete(line)
            });
        }
        let length = available.len();
        if !oversize && line.len() + length <= limit {
            line.extend_from_slice(available);
        } else {
            oversize = true;
            line.clear();
        }
        reader.consume(length);
    }
}

fn response_id(value: &Value) -> Option<u64> {
    match value {
        Value::Number(number) => number.as_u64(),
        Value::String(text) => text.parse().ok(),
        _ => None,
    }
}

fn dispatch(line: &[u8], shared: &Shared, on_incoming: &IncomingHandler) {
    let text = String::from_utf8_lossy(line);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    let Ok(message) = serde_json::from_str::<Value>(trimmed) else {
        let (preview, _) = super::contract::keep_head(trimmed, 200);
        on_incoming(Incoming::Diagnostic(format!(
            "ignored a line that is not JSON: {preview}"
        )));
        return;
    };
    let Some(object) = message.as_object() else {
        on_incoming(Incoming::Diagnostic(
            "ignored a JSON value that is not an object".into(),
        ));
        return;
    };
    let method = object.get("method").and_then(Value::as_str);
    match (method, object.get("id")) {
        (Some(method), Some(id)) if !id.is_null() => on_incoming(Incoming::Request {
            id: id.clone(),
            method: method.to_string(),
            params: object.get("params").cloned().unwrap_or(Value::Null),
        }),
        (Some(method), _) => on_incoming(Incoming::Notification {
            method: method.to_string(),
            params: object.get("params").cloned().unwrap_or(Value::Null),
        }),
        (None, Some(id)) => {
            let Some(id) = response_id(id) else {
                return;
            };
            let reply = shared
                .pending
                .lock()
                .ok()
                .and_then(|mut pending| pending.take(id));
            let Some(reply) = reply else {
                return;
            };
            let result = match object.get("error") {
                Some(error) if !error.is_null() => Err(RpcError::Remote {
                    code: error.get("code").and_then(Value::as_i64).unwrap_or(-32000),
                    message: error
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("request failed")
                        .to_string(),
                    data: error.get("data").cloned(),
                }),
                _ => Ok(object.get("result").cloned().unwrap_or(Value::Null)),
            };
            let _ = reply.send(result);
        }
        (None, None) => {}
    }
}

fn read_stdout(
    label: String,
    stdout: impl Read,
    mut child: Child,
    shared: Arc<Shared>,
    on_incoming: IncomingHandler,
    on_exit: ExitHandler,
) {
    let mut reader = BufReader::with_capacity(64 * 1024, stdout);
    loop {
        match read_bounded_line(&mut reader, MAX_LINE_BYTES) {
            Ok(Line::Complete(line)) => dispatch(&line, &shared, &on_incoming),
            Ok(Line::Oversize) => on_incoming(Incoming::Diagnostic(format!(
                "{label} sent a message larger than {} MiB; it was dropped",
                MAX_LINE_BYTES / 1024 / 1024
            ))),
            Ok(Line::Eof) | Err(_) => break,
        }
    }
    let status = child.wait();
    // Give the stderr reader a moment to collect the last words.
    std::thread::sleep(Duration::from_millis(30));
    let stderr = shared
        .stderr
        .lock()
        .map(|text| text.trim().to_string())
        .unwrap_or_default();
    let status = match status {
        Ok(status) => status.to_string(),
        Err(error) => error.to_string(),
    };
    let reason = if stderr.is_empty() {
        format!("{label} exited ({status})")
    } else {
        let (tail, _) = super::contract::keep_head(last_lines(&stderr, 12).as_str(), 2000);
        format!("{label} exited ({status}): {tail}")
    };
    if let Ok(mut exited) = shared.exited.lock() {
        *exited = Some(reason.clone());
    }
    shared.exit_signal.notify_all();
    if let Ok(mut stdin) = shared.stdin.lock() {
        stdin.take();
    }
    let pending = shared
        .pending
        .lock()
        .map(|mut pending| pending.drain())
        .unwrap_or_default();
    for reply in pending {
        let _ = reply.send(Err(RpcError::Closed(reason.clone())));
    }
    on_exit(reason);
}

fn last_lines(text: &str, count: usize) -> String {
    let lines = text.lines().collect::<Vec<_>>();
    lines[lines.len().saturating_sub(count)..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn responses_follow_request_ids_not_arrival_order() {
        let mut pending = PendingRequests::default();
        pending.insert(1, "initialize");
        pending.insert(2, "thread/start");
        assert_eq!(pending.take(2).unwrap(), "thread/start");
        assert_eq!(pending.take(1).unwrap(), "initialize");
        assert!(pending.take(2).is_none());
    }

    #[test]
    fn bounded_lines_drop_oversize_messages_and_keep_reading() {
        let mut data = Vec::new();
        data.extend_from_slice(b"{\"a\":1}\n");
        data.extend(std::iter::repeat_n(b'x', 50));
        data.extend_from_slice(b"\n{\"b\":2}\n");
        let mut reader = BufReader::with_capacity(8, data.as_slice());
        assert!(
            matches!(read_bounded_line(&mut reader, 20).unwrap(), Line::Complete(line) if line == b"{\"a\":1}")
        );
        assert!(matches!(
            read_bounded_line(&mut reader, 20).unwrap(),
            Line::Oversize
        ));
        assert!(
            matches!(read_bounded_line(&mut reader, 20).unwrap(), Line::Complete(line) if line == b"{\"b\":2}")
        );
        assert!(matches!(
            read_bounded_line(&mut reader, 20).unwrap(),
            Line::Eof
        ));
    }

    fn shell(script: &str) -> LaunchSpec {
        LaunchSpec {
            program: PathBuf::from("/bin/sh"),
            args: vec!["-c".into(), script.into()],
            cwd: std::env::temp_dir(),
            env: vec![("PATH".into(), "/usr/bin:/bin".into())],
        }
    }

    #[cfg(unix)]
    #[test]
    fn out_of_order_responses_reach_the_right_caller() {
        // Answers request 2 before request 1.
        let script = r#"read a; read b; printf '{"id":2,"result":"second"}\n{"id":1,"result":"first"}\n'; sleep 5"#;
        let process = RpcProcess::spawn(
            "fake",
            &shell(script),
            false,
            Arc::new(|_| {}),
            Box::new(|_| {}),
        )
        .unwrap();
        let first = {
            let process = process.clone();
            std::thread::spawn(move || process.request("one", json!({}), Duration::from_secs(5)))
        };
        std::thread::sleep(Duration::from_millis(50));
        let second = process.request("two", json!({}), Duration::from_secs(5));
        assert_eq!(second.unwrap(), json!("second"));
        assert_eq!(first.join().unwrap().unwrap(), json!("first"));
        process.shutdown(Duration::from_millis(200)).unwrap();
        assert!(!process.is_alive());
    }

    #[cfg(unix)]
    #[test]
    fn exit_rejects_pending_requests_with_stderr() {
        let process = RpcProcess::spawn(
            "fake",
            &shell("read a; echo boom >&2; exit 3"),
            true,
            Arc::new(|_| {}),
            Box::new(|_| {}),
        )
        .unwrap();
        let error = process
            .request("initialize", json!({}), Duration::from_secs(5))
            .unwrap_err();
        let text = error.to_string();
        assert!(matches!(error, RpcError::Closed(_)), "{text}");
        assert!(text.contains("boom"), "{text}");
        assert!(!process.is_alive());
    }

    #[cfg(unix)]
    #[test]
    fn shutdown_ends_commands_started_in_the_group() {
        let process = RpcProcess::spawn(
            "fake",
            &shell("sleep 30 & sleep 30 & wait"),
            false,
            Arc::new(|_| {}),
            Box::new(|_| {}),
        )
        .unwrap();
        let pid = process.pid();
        std::thread::sleep(Duration::from_millis(100));
        assert!(group_has_members(pid));
        process.shutdown(Duration::from_millis(300)).unwrap();
        assert!(!group_has_members(pid));
    }

    #[cfg(unix)]
    #[test]
    fn shutdown_leaves_unrelated_processes_alone() {
        // Stands in for a Codex session the user runs outside CoWiki.
        let mut outsider = std::process::Command::new("/bin/sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let process = RpcProcess::spawn(
            "fake",
            &shell("sleep 30"),
            false,
            Arc::new(|_| {}),
            Box::new(|_| {}),
        )
        .unwrap();
        process.shutdown(Duration::from_millis(200)).unwrap();
        assert!(
            outsider.try_wait().unwrap().is_none(),
            "an unrelated process was killed"
        );
        outsider.kill().unwrap();
        outsider.wait().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn requests_time_out_without_a_response() {
        let process = RpcProcess::spawn(
            "fake",
            &shell("sleep 30"),
            false,
            Arc::new(|_| {}),
            Box::new(|_| {}),
        )
        .unwrap();
        let error = process
            .request("turn/start", json!({}), Duration::from_millis(100))
            .unwrap_err();
        assert_eq!(error, RpcError::Timeout("turn/start".into()));
        process.shutdown(Duration::from_millis(200)).unwrap();
    }
}
