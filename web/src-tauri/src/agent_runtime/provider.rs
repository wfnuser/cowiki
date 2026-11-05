//! What every protocol adapter (Codex app-server, ACP, Claude stream-json)
//! offers the session registry, and the normalized events it sends back.

use super::contract::{AgentApproval, ToolKind, ToolStatus};
use super::rpc::LaunchSpec;
use crate::terminal::AgentKind;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnOutcome {
    Completed,
    Interrupted,
    Failed,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProviderEvent {
    TurnStarted {
        turn_id: String,
    },
    TurnFinished {
        turn_id: Option<String>,
        outcome: TurnOutcome,
        error: Option<String>,
    },
    /// Replace (`append == false`) or extend an assistant message.
    Message {
        item_id: String,
        turn_id: Option<String>,
        text: String,
        append: bool,
    },
    /// Create or update one tool step. `None` fields keep their value.
    Tool {
        item_id: String,
        turn_id: Option<String>,
        name: Option<String>,
        kind: ToolKind,
        status: Option<ToolStatus>,
        output: Option<String>,
        append: Option<String>,
    },
    Approval(AgentApproval),
    ApprovalResolved {
        approval_id: String,
    },
    Model(String),
    Notice(String),
    Error(String),
    Exited {
        reason: String,
    },
}

pub type EventSink = Arc<dyn Fn(ProviderEvent) + Send + Sync>;

#[derive(Debug, Clone)]
pub struct McpLaunch {
    pub command: PathBuf,
    pub args: Vec<String>,
}

/// Everything an adapter needs to start one session. `cwd` was resolved by
/// the registry from LocalEngine; the renderer never supplies it.
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub agent: AgentKind,
    pub launch: LaunchSpec,
    pub instructions: String,
    pub mcp: Option<McpLaunch>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenError {
    SignedOut(String),
    /// The saved thread/session no longer exists; nothing was created.
    ThreadMissing(String),
    /// Local settings would widen the permission boundary.
    Unsafe(String),
    Incompatible(String),
    Failed(String),
}

impl OpenError {
    pub fn message(&self) -> &str {
        match self {
            Self::SignedOut(message)
            | Self::ThreadMissing(message)
            | Self::Unsafe(message)
            | Self::Incompatible(message)
            | Self::Failed(message) => message,
        }
    }
}

pub struct Opened {
    pub provider: Arc<dyn Provider>,
    pub thread_id: String,
    pub model: Option<String>,
}

pub trait Provider: Send + Sync {
    /// Starts one turn. Blocks until the provider accepted it (Codex) or the
    /// turn ended (ACP); events stream through the sink meanwhile.
    fn start_turn(&self, turn_id: &str, text: &str) -> Result<(), String>;
    /// Stops the running turn, or the turn that is about to start.
    fn interrupt(&self) -> Result<(), String>;
    /// Answers one pending approval exactly once.
    fn answer(&self, approval_id: &str, allow: bool) -> Result<(), String>;
    /// Ends the provider process group and confirms nothing is left running.
    fn shutdown(&self) -> Result<(), String>;
    fn is_alive(&self) -> bool;
}

/// Opens a provider for a session: spawns it, completes the handshake and
/// starts (`resume == None`) or resumes a thread.
pub trait ProviderLauncher: Send + Sync {
    fn open(
        &self,
        config: ProviderConfig,
        resume: Option<String>,
        sink: EventSink,
    ) -> Result<Opened, OpenError>;
}
