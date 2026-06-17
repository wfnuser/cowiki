//! Core agent protocol types: request/response, tool definitions, config.

use serde::{Deserialize, Serialize};

// ── Agent Request/Response ──────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRequest {
    /// Task type: "compile" | "deep-compile" | "review"
    pub task_type: String,
    /// Session ID for routing to the correct agent session
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// System prompt for the agent
    pub system_prompt: String,
    /// User input / content to process
    pub user_input: String,
    /// Workspace path for file operations
    pub workspace_path: String,
    /// Git branch for this request (derived from authenticated user by cowiki).
    /// Tool execution uses this branch for all file operations.
    #[serde(default = "default_branch")]
    pub branch: String,
    /// Available tools the agent can call
    pub tools: Vec<ToolDef>,
    /// Optional JSON schema for structured output
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
    /// Agent runtime configuration
    pub config: AgentConfig,
    /// Source paths the agent is allowed to read. Empty = no restriction.
    /// For compile tasks: populated with changed source paths from do_compile.
    /// Gateway enforces: any list/read on sources/ must be within this scope.
    #[serde(default)]
    pub source_scope: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    /// JSON Schema for the tool's parameters
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    #[serde(default = "default_max_rounds")]
    pub max_rounds: u32,
    #[serde(default = "default_token_budget")]
    pub token_budget: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Timeout in seconds
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_rounds: default_max_rounds(),
            token_budget: default_token_budget(),
            model: None,
            timeout_secs: default_timeout_secs(),
        }
    }
}

fn default_max_rounds() -> u32 {
    20
}
fn default_token_budget() -> u32 {
    100_000
}
fn default_timeout_secs() -> u64 {
    300
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<UsageInfo>,
    pub rounds: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Pages written by the agent during this task.
    /// Populated by ToolGateway as the agent calls write().
    /// Server uses this instead of scanning directories.
    #[serde(default)]
    pub written_pages: Vec<String>,
    /// Pages removed by the agent (deep-compile only).
    #[serde(default)]
    pub removed_pages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageInfo {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

impl Default for UsageInfo {
    fn default() -> Self {
        Self {
            input_tokens: 0,
            output_tokens: 0,
        }
    }
}

// ── Defaults ────────────────────────────────────────────────────

fn default_branch() -> String {
    "main".into()
}

// ── Task Types (constants) ─────────────────────────────────────

/// Well-known task types for agent dispatch
pub mod task_type {
    pub const COMPILE: &str = "compile";
    pub const DEEP_COMPILE: &str = "deep-compile";
    pub const REVIEW: &str = "review";
}
