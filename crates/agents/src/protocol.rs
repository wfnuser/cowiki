use serde::{Deserialize, Serialize};

// ── Agent Request/Response ──────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRequest {
    /// Task type: "shallow_compile" | "deep_compile" | "search" | "review_submission"
    pub task_type: String,
    /// System prompt for the agent
    pub system_prompt: String,
    /// User input / content to process
    pub user_input: String,
    /// Workspace path for file operations
    pub workspace_path: String,
    /// Available tools the agent can call
    pub tools: Vec<ToolDef>,
    /// Optional JSON schema for structured output
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
    /// Agent runtime configuration
    pub config: AgentConfig,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageInfo {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

// ── Harness Registration ───────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessRegistration {
    /// Unique name for this harness, e.g. "compile-simple"
    pub name: String,
    /// Task type this harness handles, e.g. "shallow_compile"
    pub task_type: String,
    /// HTTP endpoint, e.g. "http://localhost:9100/agent/run"
    pub endpoint: String,
    /// Transport protocol
    pub transport: TransportType,
    /// Maximum concurrent requests this harness can handle
    pub max_concurrency: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransportType {
    Http,
    Grpc,
}

// ── Pool Configuration ─────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolConfig {
    pub shallow_compile: PoolEntry,
    pub deep_compile: PoolEntry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolEntry {
    pub size: u32,
    pub harness: String,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            shallow_compile: PoolEntry {
                size: 1,
                harness: "compile-simple".into(),
            },
            deep_compile: PoolEntry {
                size: 1,
                harness: "deep-compile".into(),
            },
        }
    }
}

// ── Tier Limits ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierLimit {
    pub tier: String,
    pub max_shallow_compile_agents: u32,
    pub max_deep_compile_agents: u32,
}

impl TierLimit {
    pub fn for_tier(tier: &str) -> Self {
        match tier {
            "free" => Self {
                tier: "free".into(),
                max_shallow_compile_agents: 1,
                max_deep_compile_agents: 1,
            },
            "pro" => Self {
                tier: "pro".into(),
                max_shallow_compile_agents: 4,
                max_deep_compile_agents: 1,
            },
            "enterprise" => Self {
                tier: "enterprise".into(),
                max_shallow_compile_agents: 16,
                max_deep_compile_agents: 1,
            },
            _ => Self {
                tier: "free".into(),
                max_shallow_compile_agents: 1,
                max_deep_compile_agents: 1,
            },
        }
    }

    /// Get max agents allowed for a given task type
    pub fn max_for(&self, task_type: &str) -> u32 {
        match task_type {
            "shallow_compile" => self.max_shallow_compile_agents,
            "deep_compile" => self.max_deep_compile_agents,
            _ => 1,
        }
    }
}

// ── Task Types (constants) ─────────────────────────────────────

/// Well-known task types for agent dispatch
pub mod task_type {
    pub const SHALLOW_COMPILE: &str = "shallow_compile";
    pub const DEEP_COMPILE: &str = "deep_compile";
    pub const SEARCH: &str = "search";
    pub const REVIEW_SUBMISSION: &str = "review_submission";
}
