//! Agent harness — AgentHandle trait + implementations (pi, cowiki_compiler).
//!
//! ## Architecture
//!
//! ```text
//! AgentHandle (trait)
//!   ├── PiAgentHandle   ← spawn `pi --no-session`, stdin/stdout
//!   └── CowikiCompilerHandle ← embedded 4-step pipeline with events
//! ```

pub mod pi;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::types::error::AgentError;
use crate::types::protocol::UsageInfo;

// ── AgentHandle trait ─────────────────────────────────────────

/// Abstract agent communication interface.
#[async_trait]
pub trait AgentHandle: Send + Sync {
    async fn connect(&self, ctx: AgentContext, start: AgentStart)
        -> Result<AgentState, AgentError>;

    async fn close(&self) -> Result<(), AgentError>;

    fn agent_type(&self) -> &str;
}

// ── Shared types ──────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AgentContext {
    pub execution_id: String,
    pub workspace: String,
    pub branch: String,
    pub source_scope: Vec<String>,
    pub mcp_url: String,
    pub task_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStart {
    pub prompt: String,
    pub source_paths: Vec<String>,
    pub max_rounds: u32,
}

#[derive(Debug, Clone)]
pub struct AgentState {
    pub success: bool,
    pub rounds: u32,
    pub usage: UsageInfo,
    pub error: Option<String>,
}

/// The final JSON line that an agent writes to stdout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResultLine {
    pub success: bool,
    pub rounds: u32,
    pub usage: UsageInfo,
    #[serde(default)]
    pub error: Option<String>,
}

// ── Shared types ──────────────────────────────────────────────
