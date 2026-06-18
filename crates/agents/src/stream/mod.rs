//! Agent event stream — for frontend visibility into agent activity.
//!
//! Server subscribes to these events and forwards them to the frontend
//! via SSE at `/api/agents/{ws}/events`.

use serde::Serialize;

/// Agent lifecycle and activity events.
///
/// Tagged union — frontend can switch on `type` field.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum AgentEvent {
    /// A task was started
    TaskStarted { workspace: String, agent: String, task_id: String },
    /// An agent started calling a tool
    ToolStart {
        workspace: String,
        agent: String,
        task_id: String,
        tool: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        input: Option<serde_json::Value>,
    },
    /// A tool call completed
    ToolEnd {
        workspace: String,
        agent: String,
        task_id: String,
        tool: String,
        success: bool,
        summary: String,
    },
    /// A task completed
    TaskCompleted {
        workspace: String,
        agent: String,
        task_id: String,
        success: bool,
        rounds: u32,
    },
    /// An agent stopped (idle timeout, crash, etc.)
    AgentStopped { workspace: String, agent: String, reason: String },
}
