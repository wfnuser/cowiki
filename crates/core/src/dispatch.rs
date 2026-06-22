//! Agent dispatch — trait + shared types.
//!
//! The AgentDispatcher trait bridges core business logic with the agents crate.
//! Server implements this trait to dispatch any task to AgentManager.
//! Core uses AgentResult from cowiki_agents directly.

use std::collections::HashMap;

pub use cowiki_agents::manager::AgentType;
pub use cowiki_agents::types::protocol::AgentResult;

/// Trait for dispatching a task to an agent.
///
/// Server implements this to connect core business logic with the agents crate.
/// The dispatcher is responsible for:
/// 1. Building the final prompt (slash command + semantic prompt)
/// 2. Calling AgentManager::execute() with the right params
/// 3. Returning AgentResult
#[async_trait::async_trait]
pub trait AgentDispatcher: Send + Sync {
    /// Dispatch a task to an agent and return the result.
    ///
    /// `agent_name` — e.g., "compiler"
    /// `prompt` — semantic prompt (no slash command); dispatcher adds slash command
    /// `skills` — skill names to install, e.g. ["cowiki-compiler"]
    /// `workspace` — workspace slug
    /// `extra_args` — passed through to agent as COWIKI_EXTRA_ARGS
    async fn dispatch(
        &self,
        agent_name: &str,
        prompt: &str,
        skills: &[&str],
        workspace: &str,
        extra_args: HashMap<String, String>,
    ) -> Result<AgentResult, String>;
}
