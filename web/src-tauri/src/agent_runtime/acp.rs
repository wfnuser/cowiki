//! Agent Client Protocol (ACP v1) adapter over stdio. Used for Grok
//! (`grok agent stdio`, checked against Grok Build 1.0.67) and for other
//! agents whose ACP entry point the readiness probe confirms at runtime.

use super::contract::{AgentApproval, ApprovalKind, ToolKind, ToolStatus};
use super::permissions::acp_option;
use super::provider::{
    EventSink, OpenError, Opened, Provider, ProviderConfig, ProviderEvent, TurnOutcome,
};
use super::rpc::{Framing, Incoming, RpcError, RpcProcess};
use crate::terminal::AgentKind;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Duration;

pub const ACP_PROTOCOL_VERSION: u64 = 1;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);
const SESSION_TIMEOUT: Duration = Duration::from_secs(120);
/// A prompt request stays open for the whole turn.
const PROMPT_TIMEOUT: Duration = Duration::from_secs(12 * 60 * 60);
const SHUTDOWN_GRACE: Duration = Duration::from_millis(1500);

/// How each ACP-capable agent is started, and the environment that keeps it
/// inside CoWiki's boundary.
/// argv after the program, and fixed environment variables.
pub type AcpLaunch = (Vec<&'static str>, Vec<(&'static str, &'static str)>);

pub fn launch_args(agent: AgentKind) -> Option<AcpLaunch> {
    match agent {
        // Grok applies GROK_SANDBOX to its whole process (Seatbelt on macOS):
        // write only the working directory, /tmp and ~/.grok. Approval prompts
        // stay on (default permission mode); nothing is auto-approved.
        AgentKind::Grok => Some((
            vec!["agent", "stdio"],
            vec![
                ("GROK_SANDBOX", "workspace"),
                ("GROK_DEFAULT_PERMISSION_MODE", "default"),
            ],
        )),
        AgentKind::OpenCode => Some((vec!["acp"], vec![])),
        AgentKind::Hermes => Some((vec!["acp"], vec![])),
        _ => None,
    }
}

pub fn initialize_params() -> Value {
    json!({
        "protocolVersion": ACP_PROTOCOL_VERSION,
        // CoWiki does not lend the agent its own file system or terminal:
        // the agent uses its own tools inside its sandbox.
        "clientCapabilities": {"fs": {"readTextFile": false, "writeTextFile": false}, "terminal": false},
        "clientInfo": {"name": "cowiki", "title": "CoWiki", "version": env!("CARGO_PKG_VERSION")},
    })
}

/// Rejects agents that do not speak ACP v1.
pub fn check_initialize(result: &Value) -> Result<(), String> {
    match result.get("protocolVersion").and_then(Value::as_u64) {
        Some(ACP_PROTOCOL_VERSION) => Ok(()),
        Some(other) => Err(format!(
            "this agent speaks ACP version {other}; CoWiki supports version 1"
        )),
        None => Err("this agent did not report an ACP protocol version".into()),
    }
}

pub fn is_auth_required(error: &RpcError) -> bool {
    match error {
        RpcError::Remote { code, message, .. } => {
            *code == -32000 && message.to_lowercase().contains("auth")
        }
        _ => false,
    }
}

struct PendingPermission {
    request_id: Value,
    options: Vec<Value>,
}

#[derive(Default)]
struct AcpState {
    session_id: String,
    turn: Option<String>,
    message_item: Option<String>,
    thought_item: Option<String>,
    approvals: HashMap<String, PendingPermission>,
    replaying: bool,
}

pub struct AcpProvider {
    label: String,
    rpc: OnceLock<Arc<RpcProcess>>,
    state: Mutex<AcpState>,
    sink: EventSink,
}

pub fn open(
    config: ProviderConfig,
    resume: Option<String>,
    sink: EventSink,
) -> Result<Opened, OpenError> {
    let (args, env) = launch_args(config.agent)
        .ok_or_else(|| OpenError::Incompatible("this agent has no ACP entry point".into()))?;
    let label = crate::terminal::agent_display_name(config.agent).to_string();
    let provider = Arc::new(AcpProvider {
        label: label.clone(),
        rpc: OnceLock::new(),
        state: Mutex::new(AcpState::default()),
        sink: sink.clone(),
    });
    let mut launch = config.launch.clone();
    launch.args.extend(args.iter().map(|arg| arg.to_string()));
    launch
        .env
        .retain(|(key, _)| !env.iter().any(|(fixed, _)| fixed == key));
    launch.env.extend(
        env.iter()
            .map(|(key, value)| (key.to_string(), value.to_string())),
    );
    let weak: Weak<AcpProvider> = Arc::downgrade(&provider);
    let on_incoming = Arc::new(move |incoming: Incoming| {
        if let Some(provider) = weak.upgrade() {
            provider.handle(incoming);
        }
    });
    let on_exit = {
        let sink = sink.clone();
        Box::new(move |reason: String| sink(ProviderEvent::Exited { reason }))
    };
    let rpc = RpcProcess::spawn(&label, &launch, Framing::JsonRpc2, on_incoming, on_exit)
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

fn mcp_servers(config: &ProviderConfig) -> Value {
    match config.mcp.as_ref() {
        Some(mcp) => json!([{
            "name": "cowiki",
            "command": mcp.command.to_string_lossy(),
            "args": mcp.args,
            "env": [],
        }]),
        None => json!([]),
    }
}

fn handshake(
    provider: &Arc<AcpProvider>,
    rpc: &RpcProcess,
    config: &ProviderConfig,
    resume: Option<String>,
) -> Result<(String, Option<String>), OpenError> {
    let label = &provider.label;
    let initialized = rpc
        .request("initialize", initialize_params(), HANDSHAKE_TIMEOUT)
        .map_err(|error| {
            OpenError::Incompatible(format!(
                "{label} did not complete the ACP handshake: {error}"
            ))
        })?;
    check_initialize(&initialized).map_err(OpenError::Incompatible)?;
    let cwd = config.launch.cwd.to_string_lossy().into_owned();
    let signed_out = |error: &RpcError| {
        is_auth_required(error).then(|| OpenError::SignedOut(format!("{label} is not signed in")))
    };
    let session_id = match resume {
        Some(session_id) => {
            let resumable = initialized
                .pointer("/agentCapabilities/sessionCapabilities/resume")
                .is_some();
            let loadable = initialized
                .pointer("/agentCapabilities/loadSession")
                .and_then(Value::as_bool)
                == Some(true);
            let params =
                json!({"sessionId": session_id, "cwd": cwd, "mcpServers": mcp_servers(config)});
            let method = if resumable {
                "session/resume"
            } else if loadable {
                // session/load replays the history as updates; the saved
                // transcript already has it.
                provider
                    .state
                    .lock()
                    .map_err(|_| OpenError::Failed("state unavailable".into()))?
                    .replaying = true;
                "session/load"
            } else {
                return Err(OpenError::ThreadMissing(format!(
                    "{label} cannot resume a saved conversation"
                )));
            };
            let result = rpc.request(method, params, SESSION_TIMEOUT);
            if let Ok(mut state) = provider.state.lock() {
                state.replaying = false;
            }
            let result = result.map_err(|error| {
                signed_out(&error).unwrap_or_else(|| {
                    OpenError::ThreadMissing(format!(
                        "{label} could not reopen this conversation ({error}). CoWiki did not start a new one in its place."
                    ))
                })
            })?;
            enforce_asking_mode(rpc, label, &session_id, &result)?;
            session_id
        }
        None => {
            let result = rpc
                .request(
                    "session/new",
                    json!({"cwd": cwd, "mcpServers": mcp_servers(config)}),
                    SESSION_TIMEOUT,
                )
                .map_err(|error| {
                    signed_out(&error).unwrap_or_else(|| OpenError::Failed(error.to_string()))
                })?;
            let session_id = result
                .get("sessionId")
                .and_then(Value::as_str)
                .ok_or_else(|| OpenError::Failed(format!("{label} did not return a session id")))?
                .to_string();
            enforce_asking_mode(rpc, label, &session_id, &result)?;
            session_id
        }
    };
    provider
        .state
        .lock()
        .map_err(|_| OpenError::Failed("state unavailable".into()))?
        .session_id = session_id.clone();
    let model = initialized
        .pointer("/_meta/modelState/currentModelId")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    Ok((session_id, model))
}

/// Session modes that run tools without asking.
pub fn is_unattended_mode(mode_id: &str) -> bool {
    let mode = mode_id.to_ascii_lowercase();
    [
        "bypass", "dontask", "dont_ask", "always", "yolo", "full", "auto", "accept",
    ]
    .iter()
    .any(|marker| mode.contains(marker))
}

/// An agent whose settings start it in a mode that never asks is switched
/// to its asking mode, or the session is refused.
fn enforce_asking_mode(
    rpc: &RpcProcess,
    label: &str,
    session_id: &str,
    result: &Value,
) -> Result<(), OpenError> {
    let Some(current) = result
        .pointer("/modes/currentModeId")
        .and_then(Value::as_str)
    else {
        return Ok(());
    };
    if !is_unattended_mode(current) {
        return Ok(());
    }
    let available = result
        .pointer("/modes/availableModes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let asking = available
        .iter()
        .filter_map(|mode| mode.get("id").and_then(Value::as_str))
        .find(|id| *id == "default")
        .or_else(|| {
            available
                .iter()
                .filter_map(|mode| mode.get("id").and_then(Value::as_str))
                .find(|id| !is_unattended_mode(id) && !id.contains("plan"))
        });
    let Some(asking) = asking else {
        return Err(OpenError::Unsafe(format!(
            "{label} starts in the {current} mode, which runs tools without asking. CoWiki will not start it that way."
        )));
    };
    rpc.request(
        "session/set_mode",
        json!({"sessionId": session_id, "modeId": asking}),
        HANDSHAKE_TIMEOUT,
    )
    .map_err(|error| {
        OpenError::Unsafe(format!(
            "{label} starts in the {current} mode and could not switch to {asking}: {error}"
        ))
    })?;
    Ok(())
}

fn tool_kind(kind: Option<&str>) -> ToolKind {
    match kind.unwrap_or("other") {
        "execute" => ToolKind::Command,
        "edit" | "delete" | "move" => ToolKind::File,
        "read" | "search" | "fetch" => ToolKind::Search,
        "think" => ToolKind::Think,
        _ => ToolKind::Other,
    }
}

fn tool_status(status: Option<&str>) -> Option<ToolStatus> {
    match status? {
        "pending" | "in_progress" => Some(ToolStatus::Running),
        "completed" => Some(ToolStatus::Completed),
        "failed" => Some(ToolStatus::Failed),
        _ => None,
    }
}

fn content_text(content: &Value) -> String {
    let Some(parts) = content.as_array() else {
        return String::new();
    };
    parts
        .iter()
        .filter_map(|part| match part.get("type").and_then(Value::as_str) {
            Some("content") => part
                .pointer("/content/text")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            Some("diff") => {
                let path = part.get("path").and_then(Value::as_str).unwrap_or_default();
                let new_text = part
                    .get("newText")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                Some(if part.get("oldText").is_none_or(Value::is_null) {
                    format!("add {path}\n{new_text}")
                } else {
                    format!("update {path}\n{new_text}")
                })
            }
            Some("terminal") => Some("(terminal output)".into()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn raw_command(raw: Option<&Value>) -> Option<String> {
    let raw = raw?;
    match raw.get("command") {
        Some(Value::String(command)) => Some(command.clone()),
        Some(Value::Array(parts)) => Some(
            parts
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" "),
        ),
        _ => None,
    }
}

impl AcpProvider {
    fn rpc(&self) -> Result<&Arc<RpcProcess>, String> {
        self.rpc
            .get()
            .ok_or_else(|| format!("{} is not running", self.label))
    }

    fn emit(&self, event: ProviderEvent) {
        (self.sink)(event);
    }

    fn handle(&self, incoming: Incoming) {
        match incoming {
            Incoming::Notification { method, params } if method == "session/update" => {
                self.update(&params)
            }
            Incoming::Notification { .. } => {}
            Incoming::Request { id, method, params } => self.request(id, &method, &params),
            Incoming::Diagnostic(message) => self.emit(ProviderEvent::Notice(message)),
        }
    }

    fn update(&self, params: &Value) {
        let Some(update) = params.get("update") else {
            return;
        };
        let kind = update
            .get("sessionUpdate")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let (turn, item_id) = {
            let Ok(mut state) = self.state.lock() else {
                return;
            };
            if state.replaying
                || params.get("sessionId").and_then(Value::as_str)
                    != Some(state.session_id.as_str())
            {
                return;
            }
            let Some(turn) = state.turn.clone() else {
                // Updates outside a prompt (commands list, mode, usage).
                return;
            };
            let item_id = match kind {
                "agent_message_chunk" => {
                    state.thought_item = None;
                    state
                        .message_item
                        .get_or_insert_with(|| format!("message-{}", uuid::Uuid::new_v4().simple()))
                        .clone()
                }
                "agent_thought_chunk" => {
                    state.message_item = None;
                    state
                        .thought_item
                        .get_or_insert_with(|| format!("thought-{}", uuid::Uuid::new_v4().simple()))
                        .clone()
                }
                "tool_call" | "tool_call_update" => {
                    state.message_item = None;
                    state.thought_item = None;
                    update
                        .get("toolCallId")
                        .and_then(Value::as_str)
                        .unwrap_or("tool")
                        .to_string()
                }
                "plan" => "plan".to_string(),
                _ => return,
            };
            (turn, item_id)
        };
        let turn_id = Some(turn);
        let text = update
            .pointer("/content/text")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let event = match kind {
            "agent_message_chunk" => ProviderEvent::Message {
                item_id,
                turn_id,
                text: text.to_string(),
                append: true,
            },
            "agent_thought_chunk" => ProviderEvent::Tool {
                item_id,
                turn_id,
                name: Some("Thinking".into()),
                kind: ToolKind::Think,
                status: Some(ToolStatus::Running),
                output: None,
                append: Some(text.to_string()),
            },
            "tool_call" | "tool_call_update" => {
                let title = update
                    .get("title")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
                let command = raw_command(update.get("rawInput"));
                let name = title
                    .or(command)
                    .or_else(|| (kind == "tool_call").then(|| "Tool".to_string()));
                ProviderEvent::Tool {
                    item_id,
                    turn_id,
                    name,
                    kind: tool_kind(update.get("kind").and_then(Value::as_str)),
                    status: tool_status(update.get("status").and_then(Value::as_str))
                        .or((kind == "tool_call").then_some(ToolStatus::Running)),
                    output: update.get("content").map(content_text),
                    append: None,
                }
            }
            "plan" => {
                let entries = update
                    .get("entries")
                    .and_then(Value::as_array)
                    .map(|entries| {
                        entries
                            .iter()
                            .map(|entry| {
                                format!(
                                    "[{}] {}",
                                    entry
                                        .get("status")
                                        .and_then(Value::as_str)
                                        .unwrap_or("pending"),
                                    entry
                                        .get("content")
                                        .and_then(Value::as_str)
                                        .unwrap_or_default()
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default();
                ProviderEvent::Tool {
                    item_id,
                    turn_id,
                    name: Some("Plan".into()),
                    kind: ToolKind::Other,
                    status: Some(ToolStatus::Completed),
                    output: Some(entries),
                    append: None,
                }
            }
            _ => return,
        };
        self.emit(event);
    }

    fn request(&self, id: Value, method: &str, params: &Value) {
        let Ok(rpc) = self.rpc() else { return };
        if method != "session/request_permission" {
            let _ = rpc.respond_error(&id, -32601, &format!("CoWiki does not provide {method}"));
            if method.starts_with("fs/") || method.starts_with("terminal/") {
                self.emit(ProviderEvent::Notice(format!(
                    "{} asked CoWiki for {method}; CoWiki does not lend its file system or terminal.",
                    self.label
                )));
            }
            return;
        }
        let cancelled = json!({"outcome": {"outcome": "cancelled"}});
        let accept = {
            let Ok(state) = self.state.lock() else { return };
            params.get("sessionId").and_then(Value::as_str) == Some(state.session_id.as_str())
                && state.turn.is_some()
        };
        if !accept {
            let _ = rpc.respond(&id, cancelled);
            return;
        }
        let tool_call = params.get("toolCall").cloned().unwrap_or(Value::Null);
        let options = params
            .get("options")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let kind = match tool_kind(tool_call.get("kind").and_then(Value::as_str)) {
            ToolKind::Command => ApprovalKind::Command,
            ToolKind::File => ApprovalKind::File,
            _ => ApprovalKind::Tool,
        };
        let command = raw_command(tool_call.get("rawInput"));
        let paths = tool_call
            .get("locations")
            .and_then(Value::as_array)
            .map(|locations| {
                locations
                    .iter()
                    .filter_map(|location| {
                        location
                            .get("path")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned)
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let title = tool_call
            .get("title")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| match kind {
                ApprovalKind::Command => "Run a command".into(),
                ApprovalKind::File => "Change files".into(),
                ApprovalKind::Tool => "Use a tool".into(),
            });
        let description = if acp_option(&options, true).is_none() {
            format!(
                "{} only offers a permanent approval here; CoWiki can only decline it.",
                self.label
            )
        } else {
            String::new()
        };
        let approval_id = format!("approval-{}", uuid::Uuid::new_v4().simple());
        let turn_id = {
            let Ok(mut state) = self.state.lock() else {
                return;
            };
            state.approvals.insert(
                approval_id.clone(),
                PendingPermission {
                    request_id: id,
                    options,
                },
            );
            state.turn.clone()
        };
        self.emit(ProviderEvent::Approval(AgentApproval {
            id: approval_id,
            turn_id,
            kind,
            title,
            description,
            command,
            cwd: None,
            paths,
        }));
    }

    fn cancel_pending(&self) {
        let pending = self
            .state
            .lock()
            .map(|mut state| state.approvals.drain().collect::<Vec<_>>())
            .unwrap_or_default();
        let Ok(rpc) = self.rpc() else { return };
        for (approval_id, permission) in pending {
            let _ = rpc.respond(
                &permission.request_id,
                json!({"outcome": {"outcome": "cancelled"}}),
            );
            self.emit(ProviderEvent::ApprovalResolved { approval_id });
        }
    }
}

impl Provider for AcpProvider {
    fn start_turn(&self, turn_id: &str, text: &str) -> Result<(), String> {
        let rpc = self.rpc()?.clone();
        let session_id = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| "state unavailable".to_string())?;
            if state.turn.is_some() {
                return Err(format!("{} is already working on a task", self.label));
            }
            state.turn = Some(turn_id.to_string());
            state.message_item = None;
            state.thought_item = None;
            state.session_id.clone()
        };
        self.emit(ProviderEvent::TurnStarted {
            turn_id: turn_id.to_string(),
        });
        let result = rpc.request(
            "session/prompt",
            json!({"sessionId": session_id, "prompt": [{"type": "text", "text": text}]}),
            PROMPT_TIMEOUT,
        );
        if let Ok(mut state) = self.state.lock() {
            state.turn = None;
        }
        self.cancel_pending();
        let (outcome, error) = match &result {
            Ok(response) => match response.get("stopReason").and_then(Value::as_str) {
                Some("cancelled") => (TurnOutcome::Interrupted, None),
                Some("refusal") => (
                    TurnOutcome::Failed,
                    Some(format!("{} refused to continue this task.", self.label)),
                ),
                Some("max_tokens") | Some("max_turn_requests") => {
                    self.emit(ProviderEvent::Notice(format!(
                        "{} stopped at its turn limit.",
                        self.label
                    )));
                    (TurnOutcome::Completed, None)
                }
                _ => (TurnOutcome::Completed, None),
            },
            Err(error) => (TurnOutcome::Failed, Some(error.to_string())),
        };
        self.emit(ProviderEvent::TurnFinished {
            turn_id: Some(turn_id.to_string()),
            outcome,
            error,
        });
        Ok(())
    }

    fn interrupt(&self) -> Result<(), String> {
        let session_id = {
            let state = self
                .state
                .lock()
                .map_err(|_| "state unavailable".to_string())?;
            if state.turn.is_none() {
                return Ok(());
            }
            state.session_id.clone()
        };
        // ACP: answer outstanding permission requests as cancelled, then cancel.
        self.cancel_pending();
        self.rpc()?
            .notify("session/cancel", json!({"sessionId": session_id}))
            .map_err(|error| error.to_string())
    }

    fn answer(&self, approval_id: &str, allow: bool) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "state unavailable".to_string())?;
        let permission = state
            .approvals
            .get(approval_id)
            .ok_or_else(|| "This approval is no longer pending".to_string())?;
        let outcome = match acp_option(&permission.options, allow) {
            Some(option_id) => json!({"outcome": {"outcome": "selected", "optionId": option_id}}),
            None if allow => {
                return Err(format!(
                    "{} only offers a permanent approval for this. CoWiki approves one action at a time; decline it instead.",
                    self.label
                ))
            }
            None => json!({"outcome": {"outcome": "cancelled"}}),
        };
        let permission = state.approvals.remove(approval_id).expect("checked above");
        drop(state);
        self.rpc()?
            .respond(&permission.request_id, outcome)
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
