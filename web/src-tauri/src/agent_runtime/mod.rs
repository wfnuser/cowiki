//! Native Agent runtime: structured protocols instead of terminals.
//!
//! Rust owns every provider process, session, approval and record; React
//! only receives typed snapshots (`agent:event`) and calls the commands in
//! [`commands`]. Providers: Codex (`codex app-server`), Claude Code (its
//! own stream-json control protocol, the one the Claude Agent SDK uses), and
//! Agent Client Protocol agents: Grok (`grok agent stdio`) and others the
//! probe confirms.

pub mod acp;
pub mod auth;
pub mod claude;
pub mod codex;
pub mod commands;
pub mod contract;
pub mod environment;
pub mod permissions;
pub mod provider;
pub mod registry;
pub mod rpc;
pub mod store;

#[cfg(test)]
mod tests;

use crate::terminal::{agent_display_name, resolve_agent_executable, AgentKind};
use provider::{EventSink, OpenError, Opened, ProviderConfig, ProviderLauncher};
use std::path::PathBuf;

/// Opens a provider with the adapter for its protocol. `config.launch`
/// already names the program, argv prefix and environment.
pub fn open_with_protocol(
    config: ProviderConfig,
    resume: Option<String>,
    sink: EventSink,
) -> Result<Opened, OpenError> {
    match config.agent {
        AgentKind::Codex => codex::open(config, resume, sink),
        AgentKind::Claude => claude::open(config, resume, sink),
        AgentKind::Grok | AgentKind::OpenCode | AgentKind::Hermes => {
            acp::open(config, resume, sink)
        }
        AgentKind::Antigravity => Err(OpenError::Incompatible(
            "Antigravity CLI has no structured protocol CoWiki can drive yet".into(),
        )),
    }
}

/// Production launcher: the CLI found the same way as the terminal finds it,
/// started directly (no shell) with the user's login environment.
pub struct LocalLauncher;

impl ProviderLauncher for LocalLauncher {
    fn open(
        &self,
        mut config: ProviderConfig,
        resume: Option<String>,
        sink: EventSink,
    ) -> Result<Opened, OpenError> {
        let executable = native_executable(config.agent)
            .ok_or_else(|| OpenError::Failed(not_installed_message(config.agent)))?;
        config.launch.program = executable;
        config.launch.env = environment::agent_environment();
        open_with_protocol(config, resume, sink)
    }
}

/// The program a native session starts: the agent's own CLI, found the same
/// way the terminal finds it. Nothing else needs to be installed.
pub fn native_executable(agent: AgentKind) -> Option<PathBuf> {
    resolve_agent_executable(agent)
}

pub fn not_installed_message(agent: AgentKind) -> String {
    match agent {
        AgentKind::Claude => "Claude Code is not installed or is not on PATH. Install Claude Code and sign in once, then check again.".to_string(),
        other => format!("{} is not installed or is not on PATH", agent_display_name(other)),
    }
}
