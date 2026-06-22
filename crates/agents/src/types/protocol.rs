//! Core agent protocol types: result, usage, config.
//!
//! Agents crate is pure infra — no cowiki business concepts (compile/review/etc).
//! Those live in core.

use serde::{Deserialize, Serialize};

// ── AgentResult ─────────────────────────────────────────────────

/// Result of an AgentManager::execute() call.
/// File tracking is per-session (in-memory), collected via SessionManager.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResult {
    pub success: bool,
    pub rounds: u32,
    pub usage: UsageInfo,
    /// File paths written during this session (from per-session tracking).
    pub written_files: Vec<String>,
    /// File paths removed during this session (from per-session tracking).
    pub removed_files: Vec<String>,
    pub error: Option<String>,
}

// ── UsageInfo ───────────────────────────────────────────────────

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

// ── AgentConfig ─────────────────────────────────────────────────

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
