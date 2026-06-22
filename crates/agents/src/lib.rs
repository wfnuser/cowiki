//! cowiki-agents — Agent lifecycle management, AgentHandle, AgentPool.
//!
//! Pure infra crate — no cowiki business concepts (compile/review/etc).
//!
//! ## Module map
//!
//! - `protocol/` — Agent protocol adapters: ACP (Copilot), pi RPC (NDJSON)
//! - `harness/` — AgentHandle trait + PiAgentHandle + CopilotAgentHandle (setup only)
//! - `manager/` — AgentManager::execute() + SessionManager + process + skills
//! - `types/` — Shared data types: protocol (AgentResult), bridge, error

pub mod harness;
pub mod manager;
pub mod protocol;
pub mod types;
