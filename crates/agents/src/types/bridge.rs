//! Agent ↔ Cowiki bridge types.
//!
//! Defines agent manifest and status types for stdin/stdout agent communication.
//! Agents use cowiki CLI for wiki operations; LLM calls go directly from the agent.

use serde::{Deserialize, Serialize};

// ── Agent Manifest (agent.toml) ─────────────────────────────────

/// Parsed from `agents/{name}/agent.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentManifest {
    pub agent: AgentManifestEntry,
    /// Optional runtime state — persisted by AgentManager on state transitions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<AgentRuntimeState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentManifestEntry {
    /// Unique name within the workspace
    pub name: String,
    /// Runtime type: "stdio", "embedded", etc.
    #[serde(rename = "type")]
    pub agent_type: String,
    /// Task type this agent handles
    pub task: String,
    /// Dispatch mode: "function" (one-shot) or "stream" (long-lived)
    #[serde(default = "default_mode")]
    pub mode: String,
}

fn default_mode() -> String {
    "function".into()
}

/// Runtime state persisted to agent.toml [state] section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRuntimeState {
    /// Current lifecycle status
    pub status: AgentStatus,
    /// Last known agent_id
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// ISO 8601 timestamp of last activity
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_active: Option<String>,
    /// Why the agent was stopped
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shutdown_reason: Option<String>,
    /// Number of completed sessions
    #[serde(default)]
    pub session_count: u32,
}

/// Agent lifecycle status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    /// Manifest known, process never started
    Registered,
    /// Process spawned
    Starting,
    /// Running and accepting tasks
    Active,
    /// Running but no active task
    Idle,
    /// Gracefully stopped
    Stopped,
    /// Process exited unexpectedly
    Error,
}

impl AgentManifest {
    /// Create the default compiler agent manifest.
    pub fn default_compiler() -> Self {
        Self {
            agent: AgentManifestEntry {
                name: "compiler".into(),
                agent_type: "stdio".into(),
                task: "compile".into(),
                mode: "function".into(),
            },
            state: None,
        }
    }
}
