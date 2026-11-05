//! Claude Code through its own stdio protocol: `claude` with
//! `--input-format stream-json --output-format stream-json` and
//! `--permission-prompt-tool stdio`. This is the protocol the official
//! Claude Agent SDK speaks to the CLI (checked against Claude Code 2.1.318
//! and the protocol types of `@anthropic-ai/claude-agent-sdk` 0.3.314), so
//! only `claude` itself is needed: no Node.js, no ACP adapter.
//!
//! One process per session holds the conversation; each user message is one
//! turn that ends with a `result`. Permission prompts arrive as
//! `can_use_tool` control requests and become approval cards. A PreToolUse
//! hook registered at `initialize` keeps file writes inside the session's
//! directory even when the user's own settings allow more.

use super::contract::{keep_head, AgentApproval, ApprovalKind, ToolKind, ToolStatus};
use super::permissions::{claude_file_target, inside_workspace, CLAUDE_PERMISSION_MODE};
use super::provider::{
    EventSink, OpenError, Opened, Provider, ProviderConfig, ProviderEvent, TurnOutcome,
};
use super::rpc::{Framing, Incoming, RpcError, RpcProcess};
use crate::terminal::strip_terminal_control_sequences;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex, OnceLock, Weak};
use std::time::Duration;

const LABEL: &str = "Claude Code";
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);
const INTERRUPT_TIMEOUT: Duration = Duration::from_secs(10);
/// A turn may run for a long time; Stop and the registry's watchdog end it.
const TURN_TIMEOUT: Duration = Duration::from_secs(12 * 60 * 60);
/// Claude Code cleans up the commands it started when it gets SIGTERM.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(3);
const WORKSPACE_HOOK: &str = "cowiki-workspace";
const FILE_WRITE_MATCHER: &str = "Write|Edit|MultiEdit|NotebookEdit";
/// Tools that wait for a person at a terminal, or schedule work outside
/// CoWiki.
const DISALLOWED_TOOLS: &str =
    "AskUserQuestion,ExitPlanMode,CronCreate,CronDelete,CronList,ScheduleWakeup,RemoteTrigger";
const SIGNED_OUT: &str =
    "Claude Code is not signed in. Sign in from the Agent menu, then send the message again.";

/// A new conversation with an id CoWiki chose, or a saved one.
#[derive(Debug, Clone, Copy)]
pub enum SessionStart<'a> {
    New(&'a str),
    Resume(&'a str),
}

/// The readiness probe's argv: the same boundary, no conversation saved.
pub fn probe_args() -> Vec<String> {
    let mut args = base_args();
    args.push("--no-session-persistence".into());
    args
}

/// The permission boundary every Claude Code process starts with.
fn base_args() -> Vec<String> {
    [
        "--output-format",
        "stream-json",
        "--verbose",
        "--input-format",
        "stream-json",
        "--include-partial-messages",
        "--permission-prompt-tool",
        "stdio",
        "--permission-mode",
        CLAUDE_PERMISSION_MODE,
        // The user's own settings apply (sign-in, proxy, model); a Space's
        // `.claude/` settings and hooks are content and never run.
        "--setting-sources",
        "user",
        "--strict-mcp-config",
        "--disallowedTools",
        DISALLOWED_TOOLS,
    ]
    .iter()
    .map(|arg| arg.to_string())
    .collect()
}

/// argv after the program.
pub fn launch_args(config: &ProviderConfig, start: SessionStart<'_>) -> Vec<String> {
    let mut args = base_args();
    if let Some(mcp) = config.mcp.as_ref() {
        args.push("--mcp-config".into());
        args.push(
            json!({"mcpServers": {"cowiki": {
                "type": "stdio",
                "command": mcp.command.to_string_lossy(),
                "args": mcp.args,
            }}})
            .to_string(),
        );
        // CoWiki's own server only reads (every tool is readOnlyHint).
        args.push("--allowedTools".into());
        args.push("mcp__cowiki".into());
    }
    if !config.instructions.trim().is_empty() {
        args.push("--append-system-prompt".into());
        args.push(config.instructions.clone());
    }
    match start {
        SessionStart::New(id) => args.extend(["--session-id".to_string(), id.to_string()]),
        SessionStart::Resume(id) => args.extend(["--resume".to_string(), id.to_string()]),
    }
    args
}

/// The `initialize` control request: the PreToolUse hook that checks file
/// writes against the working directory.
pub fn initialize_params() -> Value {
    json!({"hooks": {"PreToolUse": [{
        "matcher": FILE_WRITE_MATCHER,
        "hookCallbackIds": [WORKSPACE_HOOK],
    }]}})
}

/// The permission mode Claude Code reports after `initialize` must be the
/// one CoWiki asked for (user or managed settings cannot widen it).
pub fn check_initialize(response: &Value) -> Result<(), OpenError> {
    match response
        .get("current_permission_mode")
        .and_then(Value::as_str)
    {
        None => Ok(()),
        Some(mode) if mode == CLAUDE_PERMISSION_MODE => Ok(()),
        Some(mode) => Err(OpenError::Unsafe(format!(
            "Claude Code started in the {mode} permission mode instead of {CLAUDE_PERMISSION_MODE}. CoWiki will not run it that way."
        ))),
    }
}

enum TurnEnd {
    Result(Value),
    Exited(String),
}

struct PendingApproval {
    request_id: Value,
    input: Value,
    tool_use_id: Option<String>,
}

#[derive(Default)]
struct ClaudeState {
    session_id: String,
    turn: Option<String>,
    finish: Option<mpsc::Sender<TurnEnd>>,
    /// Stop arrived for the running turn, or for the one about to start.
    stop_requested: bool,
    /// `message_start` id of the message being streamed.
    stream_message: Option<String>,
    /// Text items of the streamed message, in block order.
    text_items: Vec<String>,
    thinking_items: HashSet<String>,
    approvals: HashMap<String, PendingApproval>,
    /// Tool calls that were denied (shown as Declined, not Failed).
    declined: HashSet<String>,
    /// TodoWrite calls: their step shows the plan, not the tool's reply.
    plans: HashSet<String>,
    auth_failed: bool,
    /// The CLI's own words for a failed request (limits, billing).
    turn_error: Option<String>,
    /// `result` errors that arrived without a turn (a failed resume).
    startup_errors: Vec<String>,
}

pub struct ClaudeProvider {
    rpc: OnceLock<Arc<RpcProcess>>,
    state: Mutex<ClaudeState>,
    sink: EventSink,
    cwd: PathBuf,
}

pub fn open(
    config: ProviderConfig,
    resume: Option<String>,
    sink: EventSink,
) -> Result<Opened, OpenError> {
    let session_id = resume
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let start = match resume {
        Some(_) => SessionStart::Resume(&session_id),
        None => SessionStart::New(&session_id),
    };
    let mut launch = config.launch.clone();
    launch.args.extend(launch_args(&config, start));
    let provider = Arc::new(ClaudeProvider {
        rpc: OnceLock::new(),
        state: Mutex::new(ClaudeState {
            session_id: session_id.clone(),
            ..ClaudeState::default()
        }),
        sink: sink.clone(),
        cwd: launch.cwd.clone(),
    });
    let weak: Weak<ClaudeProvider> = Arc::downgrade(&provider);
    let on_incoming = Arc::new(move |incoming: Incoming| {
        if let Some(provider) = weak.upgrade() {
            provider.handle(incoming);
        }
    });
    let on_exit = {
        let weak: Weak<ClaudeProvider> = Arc::downgrade(&provider);
        let sink = sink.clone();
        Box::new(move |reason: String| {
            if let Some(provider) = weak.upgrade() {
                provider.exited(&reason);
            }
            sink(ProviderEvent::Exited { reason })
        })
    };
    let rpc = RpcProcess::spawn(LABEL, &launch, Framing::ClaudeControl, on_incoming, on_exit)
        .map_err(OpenError::Failed)?;
    let _ = provider.rpc.set(rpc.clone());
    let opened = rpc
        .request("initialize", initialize_params(), HANDSHAKE_TIMEOUT)
        .map_err(|error| provider.open_error(&error, resume.is_some()))
        .and_then(|response| check_initialize(&response));
    match opened {
        Ok(()) => Ok(Opened {
            provider,
            thread_id: session_id,
            model: None,
        }),
        Err(error) => {
            let _ = rpc.shutdown(SHUTDOWN_GRACE);
            Err(error)
        }
    }
}

fn sanitize(text: &str, limit: usize) -> String {
    keep_head(strip_terminal_control_sequences(text).trim(), limit).0
}

fn tool_kind(name: &str) -> ToolKind {
    match name {
        "Bash" | "BashOutput" | "KillShell" | "KillBash" => ToolKind::Command,
        "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => ToolKind::File,
        "Read" | "Glob" | "Grep" | "LS" | "WebFetch" | "WebSearch" => ToolKind::Search,
        other if other.starts_with("mcp__") => ToolKind::Mcp,
        _ => ToolKind::Other,
    }
}

/// Plain text of a `tool_result` content (a string or content blocks).
fn result_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .map(|block| match block.get("type").and_then(Value::as_str) {
                Some("text") => block
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                Some("image") => "(image)".to_string(),
                _ => String::new(),
            })
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn todo_list(input: &Value) -> Option<String> {
    let todos = input.get("todos")?.as_array()?;
    Some(
        todos
            .iter()
            .map(|todo| {
                format!(
                    "[{}] {}",
                    todo.get("status")
                        .and_then(Value::as_str)
                        .unwrap_or("pending"),
                    todo.get("content")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

impl ClaudeProvider {
    fn rpc(&self) -> Result<&Arc<RpcProcess>, String> {
        self.rpc
            .get()
            .ok_or_else(|| format!("{LABEL} is not running"))
    }

    fn emit(&self, event: ProviderEvent) {
        (self.sink)(event);
    }

    fn open_error(&self, error: &RpcError, resuming: bool) -> OpenError {
        let startup = self
            .state
            .lock()
            .map(|state| state.startup_errors.join("\n"))
            .unwrap_or_default();
        let text = format!("{error}\n{startup}");
        if resuming && text.contains("No conversation found") {
            return OpenError::ThreadMissing(format!(
                "{LABEL} could not find this conversation any more. CoWiki did not start a new one in its place."
            ));
        }
        let lower = text.to_lowercase();
        if lower.contains("not logged in") || lower.contains("/login") {
            return OpenError::SignedOut(format!("{LABEL} is not signed in"));
        }
        OpenError::Incompatible(format!(
            "{LABEL} did not complete its stream-json handshake ({}). Update it with `claude update`, then try again.",
            sanitize(&error.to_string(), 600)
        ))
    }

    /// The working directory as Claude Code reports paths (canonical).
    fn relative(&self, path: &str) -> String {
        let candidates = [Some(self.cwd.clone()), self.cwd.canonicalize().ok()];
        candidates
            .iter()
            .flatten()
            .find_map(|root| Path::new(path).strip_prefix(root).ok())
            .map(|relative| relative.display().to_string())
            .filter(|relative| !relative.is_empty())
            .unwrap_or_else(|| path.to_string())
    }

    fn tool_name(&self, name: &str, input: &Value) -> String {
        let field = |key: &str| input.get(key).and_then(Value::as_str).map(str::trim);
        let named = match name {
            "Bash" => field("command").map(ToOwned::to_owned),
            "Read" | "Write" | "Edit" | "MultiEdit" => {
                field("file_path").map(|path| format!("{name} {}", self.relative(path)))
            }
            "NotebookEdit" => {
                field("notebook_path").map(|path| format!("Edit {}", self.relative(path)))
            }
            "Glob" => field("pattern").map(|pattern| format!("Find {pattern}")),
            "Grep" => field("pattern").map(|pattern| format!("Search {pattern}")),
            "WebFetch" => field("url").map(|url| format!("Fetch {url}")),
            "WebSearch" => field("query").map(|query| format!("Search the web: {query}")),
            "Task" | "Agent" => field("description").map(ToOwned::to_owned),
            "TodoWrite" => Some("Update the plan".to_string()),
            other => other
                .strip_prefix("mcp__")
                .map(|rest| rest.replacen("__", ": ", 1)),
        };
        sanitize(
            &named
                .filter(|named| !named.is_empty())
                .unwrap_or_else(|| name.to_string()),
            400,
        )
    }

    fn handle(&self, incoming: Incoming) {
        match incoming {
            Incoming::Notification { method, params } => self.message(&method, &params),
            Incoming::Request { id, method, params } => self.request(id, &method, &params),
            Incoming::Diagnostic(message) => self.emit(ProviderEvent::Notice(message)),
        }
    }

    fn current_turn(&self) -> Option<String> {
        self.state.lock().ok().and_then(|state| state.turn.clone())
    }

    fn message(&self, kind: &str, message: &Value) {
        // Subagent traffic stays inside its Task step.
        if message
            .get("parent_tool_use_id")
            .is_some_and(|parent| !parent.is_null())
        {
            return;
        }
        match kind {
            "stream_event" => self.stream_event(message.get("event").unwrap_or(&Value::Null)),
            "assistant" => self.assistant(message),
            "user" => self.tool_results(message),
            "system" => {
                if message.get("subtype").and_then(Value::as_str) == Some("init") {
                    if let Some(model) = message.get("model").and_then(Value::as_str) {
                        self.emit(ProviderEvent::Model(model.to_string()));
                    }
                }
                if message.get("subtype").and_then(Value::as_str) == Some("api_retry")
                    && message.get("error").and_then(Value::as_str) == Some("authentication_failed")
                {
                    if let Ok(mut state) = self.state.lock() {
                        state.auth_failed = true;
                    }
                }
            }
            "result" => {
                let finish = self.state.lock().ok().and_then(|mut state| {
                    let finish = state.finish.take();
                    if finish.is_none() {
                        let errors = message
                            .get("errors")
                            .and_then(Value::as_array)
                            .map(|errors| {
                                errors
                                    .iter()
                                    .filter_map(Value::as_str)
                                    .map(ToOwned::to_owned)
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default();
                        state.startup_errors.extend(errors);
                    }
                    finish
                });
                if let Some(finish) = finish {
                    let _ = finish.send(TurnEnd::Result(message.clone()));
                }
            }
            "control_cancel_request" => {
                let Some(request_id) = message.get("request_id") else {
                    return;
                };
                let cancelled = self.state.lock().ok().and_then(|mut state| {
                    let approval_id = state
                        .approvals
                        .iter()
                        .find(|(_, pending)| &pending.request_id == request_id)
                        .map(|(approval_id, _)| approval_id.clone())?;
                    state.approvals.remove(&approval_id);
                    Some(approval_id)
                });
                if let Some(approval_id) = cancelled {
                    self.emit(ProviderEvent::ApprovalResolved { approval_id });
                }
            }
            _ => {}
        }
    }

    fn stream_event(&self, event: &Value) {
        let Some(turn) = self.current_turn() else {
            return;
        };
        let index = event.get("index").and_then(Value::as_u64).unwrap_or(0);
        match event.get("type").and_then(Value::as_str) {
            Some("message_start") => {
                if let Ok(mut state) = self.state.lock() {
                    state.stream_message = event
                        .pointer("/message/id")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned);
                    state.text_items.clear();
                }
            }
            Some("content_block_start") => {
                if event.pointer("/content_block/type").and_then(Value::as_str) == Some("text") {
                    if let Ok(mut state) = self.state.lock() {
                        let message = state.stream_message.clone().unwrap_or_default();
                        state.text_items.push(format!("{message}:{index}"));
                    }
                }
            }
            Some("content_block_delta") => {
                let message = self
                    .state
                    .lock()
                    .ok()
                    .and_then(|state| state.stream_message.clone())
                    .unwrap_or_default();
                let item_id = format!("{message}:{index}");
                let delta = event.get("delta").unwrap_or(&Value::Null);
                match delta.get("type").and_then(Value::as_str) {
                    Some("text_delta") => {
                        let text = delta.get("text").and_then(Value::as_str).unwrap_or("");
                        if !text.is_empty() {
                            self.emit(ProviderEvent::Message {
                                item_id,
                                turn_id: Some(turn),
                                text: text.to_string(),
                                append: true,
                            });
                        }
                    }
                    Some("thinking_delta") => {
                        let text = delta.get("thinking").and_then(Value::as_str).unwrap_or("");
                        if text.is_empty() {
                            return;
                        }
                        let first = self
                            .state
                            .lock()
                            .map(|mut state| state.thinking_items.insert(item_id.clone()))
                            .unwrap_or(false);
                        self.emit(ProviderEvent::Tool {
                            item_id,
                            turn_id: Some(turn),
                            name: first.then(|| "Thinking".to_string()),
                            kind: ToolKind::Think,
                            status: first.then_some(ToolStatus::Running),
                            output: None,
                            append: Some(text.to_string()),
                        });
                    }
                    _ => {}
                }
            }
            Some("content_block_stop") => {
                let message = self
                    .state
                    .lock()
                    .ok()
                    .and_then(|state| state.stream_message.clone())
                    .unwrap_or_default();
                let item_id = format!("{message}:{index}");
                let thinking = self
                    .state
                    .lock()
                    .is_ok_and(|state| state.thinking_items.contains(&item_id));
                if thinking {
                    self.emit(ProviderEvent::Tool {
                        item_id,
                        turn_id: Some(turn),
                        name: None,
                        kind: ToolKind::Think,
                        status: Some(ToolStatus::Completed),
                        output: None,
                        append: None,
                    });
                }
            }
            _ => {}
        }
    }

    fn assistant(&self, message: &Value) {
        let Some(turn) = self.current_turn() else {
            return;
        };
        let content = message
            .pointer("/message/content")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let message_id = message
            .pointer("/message/id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let texts: Vec<String> = content
            .iter()
            .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|block| block.get("text").and_then(Value::as_str))
            .map(ToOwned::to_owned)
            .collect();
        if let Some(error) = message.get("error").and_then(Value::as_str) {
            // The CLI's own words ("Not logged in · Please run /login",
            // a usage limit) are not a reply.
            if let Ok(mut state) = self.state.lock() {
                if matches!(error, "authentication_failed" | "oauth_org_not_allowed") {
                    state.auth_failed = true;
                } else {
                    let text = texts.join("\n");
                    state.turn_error = Some(if text.trim().is_empty() {
                        error.to_string()
                    } else {
                        sanitize(&text, 2000)
                    });
                }
            }
            return;
        }
        // Claude Code sends one block per assistant message; its text
        // replaces what was streamed for that block.
        let streamed = self
            .state
            .lock()
            .ok()
            .filter(|state| state.stream_message.as_deref() == Some(message_id.as_str()))
            .map(|state| state.text_items.clone())
            .unwrap_or_default();
        for (position, text) in texts.iter().enumerate() {
            let item_id = if texts.len() == 1 {
                streamed.last().cloned()
            } else {
                streamed.get(position).cloned()
            }
            .unwrap_or_else(|| format!("{message_id}:text-{position}"));
            self.emit(ProviderEvent::Message {
                item_id,
                turn_id: Some(turn.clone()),
                text: text.clone(),
                append: false,
            });
        }
        for block in &content {
            if block.get("type").and_then(Value::as_str) != Some("tool_use") {
                continue;
            }
            let Some(id) = block.get("id").and_then(Value::as_str) else {
                continue;
            };
            let name = block.get("name").and_then(Value::as_str).unwrap_or("Tool");
            let input = block.get("input").cloned().unwrap_or(Value::Null);
            if name == "TodoWrite" {
                if let Ok(mut state) = self.state.lock() {
                    state.plans.insert(id.to_string());
                }
            }
            self.emit(ProviderEvent::Tool {
                item_id: id.to_string(),
                turn_id: Some(turn.clone()),
                name: Some(self.tool_name(name, &input)),
                kind: tool_kind(name),
                status: Some(ToolStatus::Running),
                output: if name == "TodoWrite" {
                    todo_list(&input)
                } else {
                    None
                },
                append: None,
            });
        }
    }

    fn tool_results(&self, message: &Value) {
        let Some(turn) = self.current_turn() else {
            return;
        };
        let Some(content) = message
            .pointer("/message/content")
            .and_then(Value::as_array)
        else {
            return;
        };
        for block in content {
            if block.get("type").and_then(Value::as_str) != Some("tool_result") {
                continue;
            }
            let Some(id) = block.get("tool_use_id").and_then(Value::as_str) else {
                continue;
            };
            let failed = block.get("is_error").and_then(Value::as_bool) == Some(true);
            let (status, plan) = {
                let state = self.state.lock();
                let plan = state.as_ref().is_ok_and(|state| state.plans.contains(id));
                let status = match state {
                    _ if !failed => ToolStatus::Completed,
                    Ok(state) if state.declined.contains(id) => ToolStatus::Declined,
                    Ok(state) if state.stop_requested => ToolStatus::Stopped,
                    _ => ToolStatus::Failed,
                };
                (status, plan)
            };
            self.emit(ProviderEvent::Tool {
                item_id: id.to_string(),
                turn_id: Some(turn.clone()),
                name: None,
                kind: ToolKind::Other,
                status: Some(status),
                output: (!plan || failed).then(|| result_text(block.get("content"))),
                append: None,
            });
        }
    }

    fn request(&self, id: Value, subtype: &str, request: &Value) {
        let Ok(rpc) = self.rpc() else { return };
        match subtype {
            "can_use_tool" => self.permission(rpc, id, request),
            "hook_callback" => {
                let response = self.hook(request);
                let _ = rpc.respond(&id, response);
            }
            // Never answer a dialog kind this host did not declare: the
            // CLI cancels it after its own deadline.
            "request_user_dialog" => {}
            other => {
                let _ = rpc.respond_error(&id, -32601, &format!("CoWiki does not provide {other}"));
            }
        }
    }

    /// The PreToolUse hook: a file write outside the working directory is
    /// denied before Claude Code consults any allow rule or mode.
    fn hook(&self, request: &Value) -> Value {
        if request.get("callback_id").and_then(Value::as_str) != Some(WORKSPACE_HOOK) {
            return json!({});
        }
        let input = request.get("input").unwrap_or(&Value::Null);
        let tool_name = input
            .get("tool_name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let tool_input = input.get("tool_input").unwrap_or(&Value::Null);
        let Some(target) = claude_file_target(tool_name, tool_input) else {
            return json!({});
        };
        if target.is_some_and(|path| inside_workspace(&path, &self.cwd)) {
            return json!({});
        }
        let tool_use_id = request
            .get("tool_use_id")
            .or_else(|| input.get("tool_use_id"))
            .and_then(Value::as_str);
        if let (Some(tool_use_id), Ok(mut state)) = (tool_use_id, self.state.lock()) {
            state.declined.insert(tool_use_id.to_string());
        }
        json!({"hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "permissionDecisionReason": format!(
                "CoWiki only lets this agent change files inside {}.",
                self.cwd.display()
            ),
        }})
    }

    fn permission(&self, rpc: &RpcProcess, id: Value, request: &Value) {
        let tool_name = request
            .get("tool_name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let input = request.get("input").cloned().unwrap_or(Value::Null);
        let tool_use_id = request
            .get("tool_use_id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let deny = |message: &str| json!({"behavior": "deny", "message": message});
        let Some(turn_id) = self.current_turn() else {
            let _ = rpc.respond(&id, deny("No CoWiki task is running."));
            return;
        };
        let file = claude_file_target(&tool_name, &input);
        if let Some(target) = file.as_ref() {
            if !target
                .as_deref()
                .is_some_and(|path| inside_workspace(path, &self.cwd))
            {
                if let (Some(tool_use_id), Ok(mut state)) =
                    (tool_use_id.as_ref(), self.state.lock())
                {
                    state.declined.insert(tool_use_id.clone());
                }
                let _ = rpc.respond(
                    &id,
                    deny(&format!(
                        "CoWiki only lets this agent change files inside {}.",
                        self.cwd.display()
                    )),
                );
                return;
            }
        }
        let (kind, command, paths) = match (tool_name.as_str(), file) {
            ("Bash", _) => (
                ApprovalKind::Command,
                input
                    .get("command")
                    .and_then(Value::as_str)
                    .map(|command| sanitize(command, 4000)),
                Vec::new(),
            ),
            (_, Some(Some(path))) => (ApprovalKind::File, None, vec![self.relative(&path)]),
            _ => (ApprovalKind::Tool, None, Vec::new()),
        };
        let field = |key: &str| {
            request
                .get(key)
                .and_then(Value::as_str)
                .map(|text| sanitize(text, 2000))
                .filter(|text| !text.is_empty())
        };
        let title = field("title").unwrap_or_else(|| match kind {
            ApprovalKind::Command => "Run a command".into(),
            ApprovalKind::File => "Change a file".into(),
            ApprovalKind::Tool => format!(
                "Use {}",
                field("display_name").unwrap_or_else(|| self.tool_name(&tool_name, &input))
            ),
        });
        let description = field("description")
            .or_else(|| field("decision_reason"))
            .or_else(|| {
                input
                    .get("description")
                    .and_then(Value::as_str)
                    .map(|text| sanitize(text, 2000))
            })
            .unwrap_or_default();
        let approval_id = format!("approval-{}", uuid::Uuid::new_v4().simple());
        if let Ok(mut state) = self.state.lock() {
            state.approvals.insert(
                approval_id.clone(),
                PendingApproval {
                    request_id: id,
                    input,
                    tool_use_id,
                },
            );
        } else {
            return;
        }
        self.emit(ProviderEvent::Approval(AgentApproval {
            id: approval_id,
            turn_id: Some(turn_id),
            kind,
            title,
            description,
            command,
            cwd: (kind == ApprovalKind::Command).then(|| self.cwd.display().to_string()),
            paths,
        }));
    }

    /// Denies every open approval (the turn ended or is being stopped).
    fn deny_pending(&self, message: &str, interrupt: bool) {
        let pending = self
            .state
            .lock()
            .map(|mut state| {
                let pending = state.approvals.drain().collect::<Vec<_>>();
                // A stopped step shows as Stopped, not as a decision.
                if !interrupt {
                    for (_, approval) in &pending {
                        if let Some(tool_use_id) = approval.tool_use_id.as_ref() {
                            state.declined.insert(tool_use_id.clone());
                        }
                    }
                }
                pending
            })
            .unwrap_or_default();
        let Ok(rpc) = self.rpc() else { return };
        for (approval_id, approval) in pending {
            let _ = rpc.respond(
                &approval.request_id,
                json!({"behavior": "deny", "message": message, "interrupt": interrupt}),
            );
            self.emit(ProviderEvent::ApprovalResolved { approval_id });
        }
    }

    fn exited(&self, reason: &str) {
        let finish = self
            .state
            .lock()
            .ok()
            .and_then(|mut state| state.finish.take());
        if let Some(finish) = finish {
            let _ = finish.send(TurnEnd::Exited(reason.to_string()));
        }
    }

    fn outcome(
        &self,
        end: Result<TurnEnd, mpsc::RecvTimeoutError>,
    ) -> (TurnOutcome, Option<String>) {
        let (stopped, auth_failed, turn_error) = self
            .state
            .lock()
            .map(|mut state| {
                (
                    std::mem::take(&mut state.stop_requested),
                    state.auth_failed,
                    state.turn_error.take(),
                )
            })
            .unwrap_or((false, false, None));
        let result = match end {
            Ok(TurnEnd::Result(result)) => result,
            Ok(TurnEnd::Exited(reason)) => {
                return if stopped {
                    (TurnOutcome::Interrupted, None)
                } else {
                    (TurnOutcome::Failed, Some(reason))
                };
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                return (
                    TurnOutcome::Failed,
                    Some(format!("{LABEL} did not finish this task in 12 hours")),
                )
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return (TurnOutcome::Failed, Some(format!("{LABEL} stopped")));
            }
        };
        let subtype = result
            .get("subtype")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let is_error = result.get("is_error").and_then(Value::as_bool) == Some(true);
        if subtype == "success" && !is_error {
            return (TurnOutcome::Completed, None);
        }
        if stopped {
            return (TurnOutcome::Interrupted, None);
        }
        if auth_failed {
            return (TurnOutcome::Failed, Some(SIGNED_OUT.to_string()));
        }
        if subtype == "error_max_turns" {
            self.emit(ProviderEvent::Notice(format!(
                "{LABEL} stopped at its turn limit."
            )));
            return (TurnOutcome::Completed, None);
        }
        let errors = result
            .get("errors")
            .and_then(Value::as_array)
            .map(|errors| {
                errors
                    .iter()
                    .filter_map(Value::as_str)
                    .filter(|error| !error.starts_with("[ede_diagnostic]"))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .filter(|errors| !errors.trim().is_empty());
        let reason = turn_error
            .or(errors)
            .or_else(|| {
                result
                    .get("result")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
                    .filter(|text| !text.trim().is_empty())
            })
            .map(|text| sanitize(&text, 2000))
            .unwrap_or_else(|| format!("{LABEL} ended the task with an error ({subtype})"));
        (TurnOutcome::Failed, Some(reason))
    }
}

impl Provider for ClaudeProvider {
    fn start_turn(&self, turn_id: &str, text: &str) -> Result<(), String> {
        let rpc = self.rpc()?.clone();
        let (sender, receiver) = mpsc::channel();
        let session_id = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| "Claude Code state is unavailable".to_string())?;
            if state.turn.is_some() {
                return Err(format!("{LABEL} is already working on a task"));
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
            state.turn = Some(turn_id.to_string());
            state.finish = Some(sender);
            state.auth_failed = false;
            state.turn_error = None;
            state.stream_message = None;
            state.text_items.clear();
            state.session_id.clone()
        };
        self.emit(ProviderEvent::TurnStarted {
            turn_id: turn_id.to_string(),
        });
        let sent = rpc.send(json!({
            "type": "user",
            "message": {"role": "user", "content": text},
            "parent_tool_use_id": null,
            "session_id": session_id,
        }));
        let end = match sent {
            Ok(()) => receiver.recv_timeout(TURN_TIMEOUT),
            Err(error) => Ok(TurnEnd::Exited(error.to_string())),
        };
        if let Ok(mut state) = self.state.lock() {
            state.turn = None;
            state.finish = None;
        }
        self.deny_pending("The task ended before this was answered.", false);
        let (outcome, error) = self.outcome(end);
        self.emit(ProviderEvent::TurnFinished {
            turn_id: Some(turn_id.to_string()),
            outcome,
            error,
        });
        Ok(())
    }

    fn interrupt(&self) -> Result<(), String> {
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| "Claude Code state is unavailable".to_string())?;
            state.stop_requested = true;
            if state.turn.is_none() {
                // The task that is about to start is never sent.
                return Ok(());
            }
        }
        self.deny_pending("The user stopped this task.", true);
        self.rpc()?
            .request("interrupt", json!({}), INTERRUPT_TIMEOUT)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    fn answer(&self, approval_id: &str, allow: bool) -> Result<(), String> {
        let approval = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| "Claude Code state is unavailable".to_string())?;
            let approval = state
                .approvals
                .remove(approval_id)
                .ok_or_else(|| "This approval is no longer pending".to_string())?;
            if !allow {
                if let Some(tool_use_id) = approval.tool_use_id.as_ref() {
                    state.declined.insert(tool_use_id.clone());
                }
            }
            approval
        };
        // One action only: never `updatedPermissions`, never a rule.
        let response = if allow {
            json!({"behavior": "allow", "updatedInput": approval.input})
        } else {
            json!({"behavior": "deny", "message": "The user declined this action in CoWiki."})
        };
        self.rpc()?
            .respond(&approval.request_id, response)
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
