//! cowiki-agents — Agent lifecycle management, AgentHandle, AgentPool.
//!
//! ## Module map
//!
//! - `harness/` — AgentHandle trait + PiAgentHandle + ClaudeAgentHandle
//! - `manager/` — Agent lifecycle: spawn_and_wait (OneShot), dispatch routing,
//!               Stream-mode registry, event broadcast
//! - `stream/` — AgentEvent + broadcast channel (SSE → frontend)
//! - `types/` — Shared data types: protocol, bridge, error

pub mod harness;
pub mod manager;
pub mod stream;
pub mod types;
