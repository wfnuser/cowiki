//! The app-level contract the renderer consumes. Provider protocols (Codex
//! app-server, Claude Code stream-json, ACP) are normalized into these types;
//! raw provider objects never reach React.

use crate::terminal::AgentKind;
use serde::{Deserialize, Serialize};

/// Longest tool output kept in a snapshot. Older bytes are dropped and the
/// tool is marked `truncated`.
pub const MAX_TOOL_OUTPUT_BYTES: usize = 64 * 1024;
/// Longest assistant message kept in a snapshot.
pub const MAX_MESSAGE_BYTES: usize = 512 * 1024;
/// Items kept per session; the oldest are dropped first.
pub const MAX_ITEMS: usize = 400;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Starting,
    Idle,
    Running,
    Waiting,
    Stopping,
    Stopped,
    Failed,
    Interrupted,
}

impl AgentStatus {
    /// A turn may still change files while the session is in one of these.
    pub fn is_busy(self) -> bool {
        matches!(
            self,
            Self::Starting | Self::Running | Self::Waiting | Self::Stopping
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentMode {
    Live,
    Background,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolStatus {
    Running,
    Completed,
    Failed,
    Declined,
    /// The turn was stopped (or the Agent exited) while the step ran.
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolKind {
    Command,
    File,
    Search,
    Mcp,
    Think,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AgentItem {
    #[serde(rename_all = "camelCase")]
    Message {
        id: String,
        role: MessageRole,
        text: String,
        turn_id: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Tool {
        id: String,
        turn_id: Option<String>,
        name: String,
        tool_kind: ToolKind,
        status: ToolStatus,
        output: String,
        truncated: bool,
    },
}

impl AgentItem {
    pub fn id(&self) -> &str {
        match self {
            Self::Message { id, .. } | Self::Tool { id, .. } => id,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalKind {
    Command,
    File,
    Tool,
}

/// One pending permission request. `id` is CoWiki's own opaque id; the
/// provider's JSON-RPC request id never leaves the backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentApproval {
    pub id: String,
    pub turn_id: Option<String>,
    pub kind: ApprovalKind,
    pub title: String,
    pub description: String,
    pub command: Option<String>,
    pub cwd: Option<String>,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSnapshot {
    pub session_id: String,
    pub provider: AgentKind,
    pub title: String,
    pub space_slug: String,
    pub mode: AgentMode,
    pub change_id: Option<String>,
    pub thread_id: Option<String>,
    pub model: Option<String>,
    pub seq: u64,
    pub status: AgentStatus,
    pub items: Vec<AgentItem>,
    pub approvals: Vec<AgentApproval>,
    pub error: Option<String>,
    pub notice: Option<String>,
    /// Set once the bound Agent Change is merged or discarded: the session
    /// stays readable but can never run again.
    pub closed: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentEvent {
    pub session_id: String,
    pub seq: u64,
    pub snapshot: AgentSnapshot,
}

pub const AGENT_EVENT: &str = "agent:event";
pub const AGENT_LOGIN_EVENT: &str = "agent:login";

pub fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

/// Keeps the newest `limit` bytes of `text` on a char boundary.
pub fn keep_tail(text: &mut String, limit: usize) -> bool {
    if text.len() <= limit {
        return false;
    }
    let mut cut = text.len() - limit;
    while !text.is_char_boundary(cut) {
        cut += 1;
    }
    text.drain(..cut);
    true
}

/// Keeps the first `limit` bytes of `text` on a char boundary.
pub fn keep_head(text: &str, limit: usize) -> (String, bool) {
    if text.len() <= limit {
        return (text.to_string(), false);
    }
    let mut cut = limit;
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    (text[..cut].to_string(), true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_uses_the_renderer_field_names() {
        let snapshot = AgentSnapshot {
            session_id: "agent:1".into(),
            provider: AgentKind::Codex,
            title: "Task".into(),
            space_slug: "demo".into(),
            mode: AgentMode::Background,
            change_id: Some("c1".into()),
            thread_id: None,
            model: None,
            seq: 3,
            status: AgentStatus::Waiting,
            items: vec![AgentItem::Tool {
                id: "t".into(),
                turn_id: Some("turn".into()),
                name: "ls".into(),
                tool_kind: ToolKind::Command,
                status: ToolStatus::Running,
                output: String::new(),
                truncated: false,
            }],
            approvals: vec![],
            error: None,
            notice: None,
            closed: false,
            created_at: 1,
            updated_at: 2,
        };
        let value = serde_json::to_value(&snapshot).unwrap();
        assert_eq!(value["sessionId"], "agent:1");
        assert_eq!(value["provider"], "codex");
        assert_eq!(value["status"], "waiting");
        assert_eq!(value["changeId"], "c1");
        assert_eq!(value["items"][0]["type"], "tool");
        assert_eq!(value["items"][0]["toolKind"], "command");
        assert_eq!(value["items"][0]["turnId"], "turn");
    }

    #[test]
    fn truncation_keeps_utf8_boundaries() {
        let mut text = "日本語のテキスト".to_string();
        assert!(keep_tail(&mut text, 7));
        assert!(text.len() <= 7);
        assert!(text.ends_with('ト'));
        let (head, cut) = keep_head("日本語", 4);
        assert!(cut);
        assert_eq!(head, "日");
    }
}
