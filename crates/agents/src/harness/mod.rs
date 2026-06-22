//! Agent harness — AgentHandle trait + implementations (pi, copilot).
//!
//! ## Architecture
//!
//! ```text
//! AgentHandle (trait)
//!   ├── PiAgentHandle      ← spawn `pi --mode rpc`, stdin/stdout NDJSON
//!   └── CopilotAgentHandle  ← spawn `copilot --acp`, ACP JSON-RPC 2.0 (default)
//! ```

pub mod copilot;
pub mod pi;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::types::error::AgentError;
use crate::types::protocol::UsageInfo;

// ── AgentHandle trait ─────────────────────────────────────────

/// Abstract agent communication interface.
/// Each handle holds its own configuration fields (server_url, timeouts, llm config, etc.)
/// and uses them inside connect().
#[async_trait]
pub trait AgentHandle: Send + Sync {
    /// One-time setup before first connect (write config files, install skills, etc.).
    /// Default no-op — Copilot overrides this.
    async fn setup(&self, task_type: &str) -> Result<(), AgentError> {
        let _ = task_type;
        Ok(())
    }

    /// Spawn agent process, run the prompt, return final state.
    async fn connect(&self, ctx: AgentContext, start: AgentStart)
        -> Result<AgentState, AgentError>;

    /// Cleanup (kill process, remove temp files).
    async fn close(&self) -> Result<(), AgentError> {
        Ok(())
    }

    fn agent_type(&self) -> &str;
}

// ── Shared types ──────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AgentContext {
    pub execution_id: String,
    pub workspace: String,
    pub server_url: String,
    /// User's API key for cowiki CLI auth (COWIKI_API_KEY)
    pub api_key: String,
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
