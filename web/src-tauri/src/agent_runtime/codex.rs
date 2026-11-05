//! Codex `app-server --listen stdio://` adapter (protocol checked against
//! codex-cli 0.166.0 with `codex app-server generate-ts`).

use super::contract::{AgentApproval, ApprovalKind, ToolKind, ToolStatus};
use super::permissions::{
    classify_approval, codex_decision, codex_sandbox_policy, verify_codex_thread_policy,
    CodexApproval, CODEX_APPROVAL_POLICY, CODEX_SANDBOX_MODE,
};
use super::provider::{
    EventSink, OpenError, Opened, Provider, ProviderConfig, ProviderEvent, TurnOutcome,
};
use super::rpc::{Incoming, RpcError, RpcProcess};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Duration;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);
const THREAD_TIMEOUT: Duration = Duration::from_secs(90);
#[cfg(not(test))]
const TURN_START_TIMEOUT: Duration = Duration::from_secs(60);
#[cfg(test)]
const TURN_START_TIMEOUT: Duration = Duration::from_secs(2);
const INTERRUPT_TIMEOUT: Duration = Duration::from_secs(15);
const SHUTDOWN_GRACE: Duration = Duration::from_millis(1500);

struct PendingApproval {
    request_id: Value,
    kind: CodexApproval,
}

#[derive(Default)]
struct CodexState {
    thread_id: String,
    current_turn: Option<String>,
    starting: bool,
    stop_requested: bool,
    last_completed: Option<String>,
    approvals: HashMap<String, PendingApproval>,
    file_paths: HashMap<String, Vec<String>>,
    next_approval: u64,
}

pub struct CodexProvider {
    rpc: OnceLock<Arc<RpcProcess>>,
    state: Mutex<CodexState>,
    sink: EventSink,
    cwd: PathBuf,
}

pub fn open(
    config: ProviderConfig,
    resume: Option<String>,
    sink: EventSink,
) -> Result<Opened, OpenError> {
    let provider = Arc::new(CodexProvider {
        rpc: OnceLock::new(),
        state: Mutex::new(CodexState::default()),
        sink: sink.clone(),
        cwd: config.launch.cwd.clone(),
    });
    let mut launch = config.launch.clone();
    launch
        .args
        .extend(["app-server".into(), "--listen".into(), "stdio://".into()]);
    if let Some(mcp) = config.mcp.as_ref() {
        let args = mcp
            .args
            .iter()
            .map(|argument| crate::terminal::toml_string(argument))
            .collect::<Vec<_>>()
            .join(",");
        launch.args.extend([
            "-c".into(),
            format!(
                "mcp_servers.cowiki.command={}",
                crate::terminal::toml_string(&mcp.command.to_string_lossy())
            ),
            "-c".into(),
            format!("mcp_servers.cowiki.args=[{args}]"),
        ]);
    }
    let weak: Weak<CodexProvider> = Arc::downgrade(&provider);
    let on_incoming = {
        let weak = weak.clone();
        Arc::new(move |incoming: Incoming| {
            if let Some(provider) = weak.upgrade() {
                provider.handle(incoming);
            }
        })
    };
    let on_exit = {
        let sink = sink.clone();
        Box::new(move |reason: String| sink(ProviderEvent::Exited { reason }))
    };
    let rpc = RpcProcess::spawn("Codex", &launch, false, on_incoming, on_exit)
        .map_err(OpenError::Failed)?;
    let _ = provider.rpc.set(rpc.clone());
    match handshake(&provider, &rpc, &config, resume) {
        Ok((thread_id, model)) => Ok(Opened {
            provider,
            thread_id,
            model,
        }),
        Err(error) => {
            let _ = rpc.shutdown(SHUTDOWN_GRACE);
            Err(error)
        }
    }
}

fn open_error(error: RpcError) -> OpenError {
    let text = error.to_string();
    if text.contains("unrecognized subcommand") || text.contains("unexpected argument") {
        return OpenError::Incompatible(format!(
            "This Codex CLI does not provide `codex app-server`. Update Codex and try again. ({text})"
        ));
    }
    OpenError::Failed(text)
}

fn handshake(
    provider: &Arc<CodexProvider>,
    rpc: &RpcProcess,
    config: &ProviderConfig,
    resume: Option<String>,
) -> Result<(String, Option<String>), OpenError> {
    rpc.request(
        "initialize",
        json!({
            "clientInfo": {"name": "cowiki", "title": "CoWiki", "version": env!("CARGO_PKG_VERSION")},
            "capabilities": {"experimentalApi": false, "requestAttestation": false},
        }),
        HANDSHAKE_TIMEOUT,
    )
    .map_err(open_error)?;
    rpc.notify("initialized", json!({})).map_err(open_error)?;
    let account = rpc
        .request("account/read", json!({}), HANDSHAKE_TIMEOUT)
        .map_err(open_error)?;
    if account.get("account").is_none_or(Value::is_null)
        && account.get("requiresOpenaiAuth").and_then(Value::as_bool) == Some(true)
    {
        return Err(OpenError::SignedOut("Codex is not signed in".into()));
    }
    let cwd = config.launch.cwd.to_string_lossy().into_owned();
    let mut params = json!({
        "cwd": cwd,
        "approvalPolicy": CODEX_APPROVAL_POLICY,
        "sandbox": CODEX_SANDBOX_MODE,
        "developerInstructions": config.instructions,
    });
    let (method, resuming) = match resume.as_deref() {
        Some(thread_id) => {
            params["threadId"] = json!(thread_id);
            ("thread/resume", true)
        }
        None => ("thread/start", false),
    };
    let response = rpc.request(method, params, THREAD_TIMEOUT).map_err(|error| {
        let text = error.to_string();
        let lower = text.to_lowercase();
        if resuming
            && matches!(error, RpcError::Remote { .. })
            && (lower.contains("not found") || lower.contains("no rollout") || lower.contains("does not exist"))
        {
            OpenError::ThreadMissing(format!(
                "Codex no longer has this conversation ({text}). CoWiki did not start a new one in its place."
            ))
        } else {
            open_error(error)
        }
    })?;
    verify_codex_thread_policy(&response, &config.launch.cwd).map_err(OpenError::Unsafe)?;
    let thread_id = response
        .pointer("/thread/id")
        .and_then(Value::as_str)
        .ok_or_else(|| OpenError::Failed("Codex did not return a thread id".into()))?
        .to_string();
    if let Some(expected) = resume.as_deref() {
        if expected != thread_id {
            return Err(OpenError::ThreadMissing(
                "Codex resumed a different conversation than the one CoWiki saved".into(),
            ));
        }
    }
    provider
        .state
        .lock()
        .map_err(|_| OpenError::Failed("Codex state is unavailable".into()))?
        .thread_id = thread_id.clone();
    let model = response
        .get("model")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    Ok((thread_id, model))
}

fn tool_status(value: Option<&Value>) -> Option<ToolStatus> {
    match value?.as_str()? {
        "inProgress" | "pending" => Some(ToolStatus::Running),
        "completed" => Some(ToolStatus::Completed),
        "failed" => Some(ToolStatus::Failed),
        "declined" => Some(ToolStatus::Declined),
        _ => None,
    }
}

fn str_of<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

impl CodexProvider {
    fn rpc(&self) -> Result<&Arc<RpcProcess>, String> {
        self.rpc
            .get()
            .ok_or_else(|| "Codex is not running".to_string())
    }

    fn emit(&self, event: ProviderEvent) {
        (self.sink)(event);
    }

    fn handle(&self, incoming: Incoming) {
        match incoming {
            Incoming::Notification { method, params } => self.notification(&method, &params),
            Incoming::Request { id, method, params } => self.request(id, &method, &params),
            Incoming::Diagnostic(message) => self.emit(ProviderEvent::Notice(message)),
        }
    }

    fn own_thread(&self, params: &Value) -> bool {
        let Some(thread) = str_of(params, "threadId") else {
            return true;
        };
        self.state
            .lock()
            .map(|state| state.thread_id.is_empty() || state.thread_id == thread)
            .unwrap_or(false)
    }

    fn notification(&self, method: &str, params: &Value) {
        if !self.own_thread(params) {
            return;
        }
        match method {
            "turn/started" => {
                let Some(turn_id) = params.pointer("/turn/id").and_then(Value::as_str) else {
                    return;
                };
                let interrupt = {
                    let Ok(mut state) = self.state.lock() else {
                        return;
                    };
                    if state.last_completed.as_deref() == Some(turn_id)
                        || state.current_turn.as_deref() == Some(turn_id)
                    {
                        return;
                    }
                    state.current_turn = Some(turn_id.to_string());
                    state.starting = false;
                    state.stop_requested
                };
                self.emit(ProviderEvent::TurnStarted {
                    turn_id: turn_id.to_string(),
                });
                if interrupt {
                    self.interrupt_in_background();
                }
            }
            "turn/completed" => {
                let turn = params.get("turn").cloned().unwrap_or(Value::Null);
                let turn_id = str_of(&turn, "id").map(ToOwned::to_owned);
                let cleared = {
                    let Ok(mut state) = self.state.lock() else {
                        return;
                    };
                    if turn_id.is_some() && state.last_completed == turn_id {
                        return;
                    }
                    state.last_completed = turn_id.clone();
                    state.current_turn = None;
                    state.starting = false;
                    state.stop_requested = false;
                    state
                        .approvals
                        .drain()
                        .map(|(id, _)| id)
                        .collect::<Vec<_>>()
                };
                for approval_id in cleared {
                    self.emit(ProviderEvent::ApprovalResolved { approval_id });
                }
                let outcome = match str_of(&turn, "status") {
                    Some("interrupted") => TurnOutcome::Interrupted,
                    Some("failed") => TurnOutcome::Failed,
                    _ => TurnOutcome::Completed,
                };
                let error = turn
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
                self.emit(ProviderEvent::TurnFinished {
                    turn_id,
                    outcome,
                    error,
                });
            }
            "item/started" | "item/completed" => {
                if let Some(item) = params.get("item") {
                    self.item(item, str_of(params, "turnId"), method == "item/completed");
                }
            }
            "item/agentMessage/delta" => {
                if let (Some(item_id), Some(delta)) =
                    (str_of(params, "itemId"), str_of(params, "delta"))
                {
                    self.emit(ProviderEvent::Message {
                        item_id: item_id.to_string(),
                        turn_id: str_of(params, "turnId").map(ToOwned::to_owned),
                        text: delta.to_string(),
                        append: true,
                    });
                }
            }
            "item/commandExecution/outputDelta" | "item/fileChange/outputDelta" => {
                if let (Some(item_id), Some(delta)) =
                    (str_of(params, "itemId"), str_of(params, "delta"))
                {
                    self.emit(ProviderEvent::Tool {
                        item_id: item_id.to_string(),
                        turn_id: str_of(params, "turnId").map(ToOwned::to_owned),
                        name: None,
                        kind: if method.contains("fileChange") {
                            ToolKind::File
                        } else {
                            ToolKind::Command
                        },
                        status: None,
                        output: None,
                        append: Some(delta.to_string()),
                    });
                }
            }
            "item/reasoning/summaryTextDelta" => {
                if let (Some(item_id), Some(delta)) =
                    (str_of(params, "itemId"), str_of(params, "delta"))
                {
                    self.emit(ProviderEvent::Tool {
                        item_id: item_id.to_string(),
                        turn_id: str_of(params, "turnId").map(ToOwned::to_owned),
                        name: Some("Thinking".into()),
                        kind: ToolKind::Think,
                        status: Some(ToolStatus::Running),
                        output: None,
                        append: Some(delta.to_string()),
                    });
                }
            }
            "serverRequest/resolved" => {
                let request = params.get("requestId").map(request_key);
                let resolved = {
                    let Ok(mut state) = self.state.lock() else {
                        return;
                    };
                    let found = state
                        .approvals
                        .iter()
                        .find(|(_, pending)| Some(request_key(&pending.request_id)) == request)
                        .map(|(id, _)| id.clone());
                    if let Some(id) = found.as_ref() {
                        state.approvals.remove(id);
                    }
                    found
                };
                if let Some(approval_id) = resolved {
                    self.emit(ProviderEvent::ApprovalResolved { approval_id });
                }
            }
            "error" => {
                let message = params
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .unwrap_or("Codex reported an error")
                    .to_string();
                if params.get("willRetry").and_then(Value::as_bool) == Some(true) {
                    self.emit(ProviderEvent::Notice(message));
                } else {
                    self.emit(ProviderEvent::Error(message));
                }
            }
            "model/rerouted" => {
                if let Some(model) = str_of(params, "toModel") {
                    self.emit(ProviderEvent::Model(model.to_string()));
                }
            }
            "warning" | "configWarning" | "deprecationNotice" => {
                if let Some(message) =
                    str_of(params, "message").or_else(|| str_of(params, "summary"))
                {
                    self.emit(ProviderEvent::Notice(message.to_string()));
                }
            }
            _ => {}
        }
    }

    fn item(&self, item: &Value, turn_id: Option<&str>, completed: bool) {
        let Some(item_id) = str_of(item, "id") else {
            return;
        };
        let turn_id = turn_id.map(ToOwned::to_owned);
        let item_type = str_of(item, "type").unwrap_or_default();
        let status = tool_status(item.get("status")).or(Some(if completed {
            ToolStatus::Completed
        } else {
            ToolStatus::Running
        }));
        let tool = |name: String, kind: ToolKind, output: Option<String>| ProviderEvent::Tool {
            item_id: item_id.to_string(),
            turn_id: turn_id.clone(),
            name: Some(name),
            kind,
            status,
            output,
            append: None,
        };
        let event = match item_type {
            "userMessage" | "hookPrompt" => return,
            "agentMessage" => ProviderEvent::Message {
                item_id: item_id.to_string(),
                turn_id: turn_id.clone(),
                text: str_of(item, "text").unwrap_or_default().to_string(),
                // `item/started` places the message; deltas and the completed
                // item fill it. A completed item is authoritative.
                append: !completed && str_of(item, "text").is_none_or(str::is_empty),
            },
            "commandExecution" => tool(
                display_command(str_of(item, "command").unwrap_or("command")),
                ToolKind::Command,
                completed.then(|| {
                    str_of(item, "aggregatedOutput")
                        .unwrap_or_default()
                        .to_string()
                }),
            ),
            "fileChange" => {
                let changes = item
                    .get("changes")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let paths = changes
                    .iter()
                    .filter_map(|change| str_of(change, "path").map(ToOwned::to_owned))
                    .collect::<Vec<_>>();
                if let Ok(mut state) = self.state.lock() {
                    state.file_paths.insert(item_id.to_string(), paths.clone());
                }
                let output = changes
                    .iter()
                    .map(|change| {
                        let path = str_of(change, "path").unwrap_or_default();
                        let kind = change
                            .pointer("/kind/type")
                            .and_then(Value::as_str)
                            .unwrap_or("update");
                        let diff = str_of(change, "diff").unwrap_or_default();
                        if diff.is_empty() {
                            format!("{kind} {path}")
                        } else {
                            format!("{kind} {path}\n{diff}")
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                let name = if paths.is_empty() {
                    "File changes".to_string()
                } else {
                    paths.join(", ")
                };
                tool(name, ToolKind::File, Some(output))
            }
            "mcpToolCall" => {
                let name = format!(
                    "{} · {}",
                    str_of(item, "server").unwrap_or("mcp"),
                    str_of(item, "tool").unwrap_or("tool")
                );
                let output = item
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
                    .or_else(|| item.pointer("/result/content").map(content_text));
                tool(name, ToolKind::Mcp, output)
            }
            "dynamicToolCall" => tool(
                str_of(item, "tool").unwrap_or("tool").to_string(),
                ToolKind::Other,
                None,
            ),
            "webSearch" => tool(
                str_of(item, "query")
                    .map(|query| format!("Search: {query}"))
                    .unwrap_or_else(|| "Web search".into()),
                ToolKind::Search,
                None,
            ),
            "reasoning" => {
                let summary = item
                    .get("summary")
                    .and_then(Value::as_array)
                    .map(|parts| {
                        parts
                            .iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join("\n\n")
                    })
                    .unwrap_or_default();
                if summary.is_empty() && !completed {
                    return;
                }
                tool("Thinking".into(), ToolKind::Think, Some(summary))
            }
            "plan" => tool(
                "Plan".into(),
                ToolKind::Other,
                str_of(item, "text").map(ToOwned::to_owned),
            ),
            "contextCompaction" => {
                if completed {
                    self.emit(ProviderEvent::Notice(
                        "Codex compacted the conversation context.".into(),
                    ));
                }
                return;
            }
            "imageView" => tool(
                format!("View image {}", str_of(item, "path").unwrap_or_default()),
                ToolKind::Other,
                None,
            ),
            other => tool(other.to_string(), ToolKind::Other, None),
        };
        self.emit(event);
    }

    fn request(&self, id: Value, method: &str, params: &Value) {
        let Ok(rpc) = self.rpc() else { return };
        if !self.own_thread(params) {
            let _ = rpc.respond_error(&id, -32602, "CoWiki does not own this thread");
            return;
        }
        let kind = match classify_approval(method) {
            Ok(kind) => kind,
            Err(message) => {
                let _ = rpc.respond_error(&id, -32601, &message);
                self.emit(ProviderEvent::Notice(format!(
                    "Codex asked for something CoWiki does not support ({method}); it was refused."
                )));
                return;
            }
        };
        let item_id = str_of(params, "itemId").unwrap_or_default().to_string();
        let turn_id = str_of(params, "turnId").map(ToOwned::to_owned);
        let reason = str_of(params, "reason").unwrap_or_default().to_string();
        let approval = match kind {
            CodexApproval::Command => AgentApproval {
                id: String::new(),
                turn_id,
                kind: ApprovalKind::Command,
                title: "Run a command".into(),
                description: reason,
                command: str_of(params, "command").map(ToOwned::to_owned),
                cwd: str_of(params, "cwd").map(ToOwned::to_owned),
                paths: Vec::new(),
            },
            CodexApproval::File => {
                if let Some(root) = str_of(params, "grantRoot") {
                    // Asking to write a whole folder for the rest of the
                    // session is an escalation, not a file approval.
                    let _ = rpc.respond(&id, codex_decision(CodexApproval::File, false));
                    self.emit(ProviderEvent::Notice(format!(
                        "Codex asked to write anywhere under {root}; CoWiki declined. Agents only write their own workspace."
                    )));
                    return;
                }
                let paths = self
                    .state
                    .lock()
                    .ok()
                    .and_then(|state| state.file_paths.get(&item_id).cloned())
                    .unwrap_or_default();
                AgentApproval {
                    id: String::new(),
                    turn_id,
                    kind: ApprovalKind::File,
                    title: "Change files".into(),
                    description: reason,
                    command: None,
                    cwd: Some(self.cwd.to_string_lossy().into_owned()),
                    paths,
                }
            }
            CodexApproval::McpTool => {
                if params
                    .pointer("/_meta/codex_approval_kind")
                    .and_then(Value::as_str)
                    != Some("mcp_tool_call")
                {
                    let _ = rpc.respond(&id, json!({"action": "decline"}));
                    self.emit(ProviderEvent::Notice(format!(
                        "{} asked for input CoWiki cannot collect; it was declined.",
                        str_of(params, "serverName").unwrap_or("An MCP server")
                    )));
                    return;
                }
                let server = str_of(params, "serverName").unwrap_or("MCP");
                let tool = params
                    .pointer("/_meta/tool_title")
                    .and_then(Value::as_str)
                    .unwrap_or("tool");
                AgentApproval {
                    id: String::new(),
                    turn_id,
                    kind: ApprovalKind::Tool,
                    title: format!("Use {server} · {tool}"),
                    description: str_of(params, "message").unwrap_or_default().to_string(),
                    command: None,
                    cwd: None,
                    paths: Vec::new(),
                }
            }
        };
        let approval_id = {
            let Ok(mut state) = self.state.lock() else {
                return;
            };
            state.next_approval += 1;
            let approval_id = format!(
                "approval-{}-{}",
                state.next_approval,
                uuid::Uuid::new_v4().simple()
            );
            state.approvals.insert(
                approval_id.clone(),
                PendingApproval {
                    request_id: id,
                    kind,
                },
            );
            approval_id
        };
        self.emit(ProviderEvent::Approval(AgentApproval {
            id: approval_id,
            ..approval
        }));
    }

    fn interrupt_in_background(&self) {
        let Some(rpc) = self.rpc.get().cloned() else {
            return;
        };
        let target = self.state.lock().ok().and_then(|state| {
            state
                .current_turn
                .clone()
                .map(|turn| (state.thread_id.clone(), turn))
        });
        if let Some((thread_id, turn_id)) = target {
            std::thread::spawn(move || {
                let _ = rpc.request(
                    "turn/interrupt",
                    json!({"threadId": thread_id, "turnId": turn_id}),
                    INTERRUPT_TIMEOUT,
                );
            });
        }
    }

    fn cancel_pending_approvals(&self) {
        let pending = self
            .state
            .lock()
            .map(|mut state| state.approvals.drain().collect::<Vec<_>>())
            .unwrap_or_default();
        let Ok(rpc) = self.rpc() else { return };
        for (approval_id, approval) in pending {
            let cancel = match approval.kind {
                CodexApproval::McpTool => json!({"action": "cancel"}),
                _ => json!({"decision": "cancel"}),
            };
            let _ = rpc.respond(&approval.request_id, cancel);
            self.emit(ProviderEvent::ApprovalResolved { approval_id });
        }
    }
}

/// Codex runs commands as `/bin/zsh -lc '<command>'`; show the command.
pub fn display_command(command: &str) -> String {
    let trimmed = command.trim();
    let inner = [
        "/bin/zsh -lc ",
        "/bin/bash -lc ",
        "/bin/sh -c ",
        "zsh -lc ",
        "bash -lc ",
    ]
    .iter()
    .find_map(|prefix| trimmed.strip_prefix(prefix));
    let Some(inner) = inner else {
        return trimmed.to_string();
    };
    let inner = inner.trim();
    if inner.len() >= 2
        && inner.starts_with('\'')
        && inner.ends_with('\'')
        && !inner[1..inner.len() - 1].contains('\'')
    {
        return inner[1..inner.len() - 1].to_string();
    }
    if inner.len() >= 2
        && inner.starts_with('"')
        && inner.ends_with('"')
        && !inner[1..inner.len() - 1].contains(['"', '\\', '$', '`'])
    {
        return inner[1..inner.len() - 1].to_string();
    }
    trimmed.to_string()
}

fn request_key(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn content_text(content: &Value) -> String {
    content
        .as_array()
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_else(|| content.to_string())
}

impl Provider for CodexProvider {
    fn start_turn(&self, _turn_id: &str, text: &str) -> Result<(), String> {
        let rpc = self.rpc()?.clone();
        let thread_id = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| "Codex state is unavailable".to_string())?;
            if state.current_turn.is_some() || state.starting {
                return Err("Codex is already working on a task".into());
            }
            if std::mem::take(&mut state.stop_requested) {
                // Stop arrived before the task was sent: never send it.
                drop(state);
                self.emit(ProviderEvent::TurnFinished {
                    turn_id: None,
                    outcome: TurnOutcome::Interrupted,
                    error: None,
                });
                return Ok(());
            }
            state.starting = true;
            state.thread_id.clone()
        };
        let params = json!({
            "threadId": thread_id,
            "input": [{"type": "text", "text": text, "text_elements": []}],
            "cwd": self.cwd.to_string_lossy(),
            "approvalPolicy": CODEX_APPROVAL_POLICY,
            "sandboxPolicy": codex_sandbox_policy(&self.cwd),
        });
        match rpc.request("turn/start", params, TURN_START_TIMEOUT) {
            Ok(response) => {
                // Accepted is not started: Codex refuses turn/interrupt until
                // turn/started names the turn, so a Stop pressed now waits
                // for that notification (see `notification`).
                let turn_id = response
                    .pointer("/turn/id")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
                let mut state = self
                    .state
                    .lock()
                    .map_err(|_| "Codex state is unavailable".to_string())?;
                if turn_id.is_some() && state.last_completed == turn_id {
                    state.starting = false;
                }
                Ok(())
            }
            Err(RpcError::Timeout(_)) => {
                // The turn may be running. Never send it again: stop Codex.
                let _ = rpc.shutdown(SHUTDOWN_GRACE);
                Err("Codex did not confirm the task within 60 seconds. CoWiki stopped Codex instead of sending the task again.".into())
            }
            Err(error) => {
                if let Ok(mut state) = self.state.lock() {
                    state.starting = false;
                    state.stop_requested = false;
                }
                Err(error.to_string())
            }
        }
    }

    fn interrupt(&self) -> Result<(), String> {
        let rpc = self.rpc()?.clone();
        let target = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| "Codex state is unavailable".to_string())?;
            match state.current_turn.clone() {
                Some(turn) => Some((state.thread_id.clone(), turn)),
                None => {
                    // turn/started (or the start that is about to happen)
                    // honours this as soon as the turn exists.
                    state.stop_requested = true;
                    None
                }
            }
        };
        self.cancel_pending_approvals();
        if let Some((thread_id, turn_id)) = target {
            rpc.request(
                "turn/interrupt",
                json!({"threadId": thread_id, "turnId": turn_id}),
                INTERRUPT_TIMEOUT,
            )
            .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    fn answer(&self, approval_id: &str, allow: bool) -> Result<(), String> {
        let pending = self
            .state
            .lock()
            .map_err(|_| "Codex state is unavailable".to_string())?
            .approvals
            .remove(approval_id)
            .ok_or_else(|| "This approval is no longer pending".to_string())?;
        self.rpc()?
            .respond(&pending.request_id, codex_decision(pending.kind, allow))
            .map_err(|error| error.to_string())?;
        self.emit(ProviderEvent::ApprovalResolved {
            approval_id: approval_id.to_string(),
        });
        Ok(())
    }

    fn shutdown(&self) -> Result<(), String> {
        match self.rpc.get() {
            Some(rpc) => rpc.shutdown(SHUTDOWN_GRACE),
            None => Ok(()),
        }
    }

    fn is_alive(&self) -> bool {
        self.rpc.get().is_some_and(|rpc| rpc.is_alive())
    }
}

#[cfg(test)]
mod tests {
    use super::display_command;

    #[test]
    fn shows_the_command_inside_codex_shell_wrappers() {
        assert_eq!(
            display_command("/bin/zsh -lc 'curl -sS -I https://example.com'"),
            "curl -sS -I https://example.com"
        );
        assert_eq!(display_command("/bin/zsh -lc \"ls docs\""), "ls docs");
        // Anything with nested quoting or expansion is shown as Codex ran it.
        let quoted = "/bin/zsh -lc \"printf '%s\\n' 'x' > a.md\"";
        assert_eq!(display_command(quoted), quoted);
        assert_eq!(display_command("ls"), "ls");
    }
}
