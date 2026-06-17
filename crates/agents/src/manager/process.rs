//! Process lifecycle — stop agents, reaper.
//!
//! Spawn logic lives in `harness/pi.rs` (PiAgentHandle).
//! This module handles kill/reap operations only.

use std::time::Duration;

use tokio::time::sleep;

use super::AgentManager;
use crate::types::bridge::AgentStatus;
use crate::types::error::AgentError;

const SHUTDOWN_GRACE_PERIOD_SECS: u64 = 5;

// ── Task request ──────────────────────────────────────────────

/// Task to dispatch to an agent.
#[derive(Debug, Clone)]
pub struct TaskRequest {
    pub task_id: String,
    pub user_input: String,
    pub workspace: String,
    pub branch: String,
    pub source_scope: Vec<String>,
    pub task_type: String,
}

// ── Stop agent ────────────────────────────────────────────────

/// Kill an agent process: SIGTERM → wait → SIGKILL.
pub async fn stop_agent(manager: &AgentManager, name: &str) -> Result<(), AgentError> {
    let mut child = {
        let mut procs = manager.processes.write().await;
        if let Some(proc) = procs.get_mut(name) {
            if matches!(proc.status, AgentStatus::Stopped) {
                return Ok(());
            }
            proc.status = AgentStatus::Stopped;
            proc.child.take()
        } else {
            return Ok(());
        }
    };

    if let Some(ref mut c) = child {
        tracing::info!(%name, "sending SIGTERM to agent");
        let _ = c.start_kill();
        sleep(Duration::from_secs(SHUTDOWN_GRACE_PERIOD_SECS)).await;
        match c.try_wait() {
            Ok(Some(status)) => tracing::info!(%name, %status, "agent exited"),
            Ok(None) => {
                tracing::warn!(%name, "agent still alive, sending SIGKILL");
                let _ = c.kill().await;
            }
            Err(e) => tracing::error!(%name, error = %e, "failed to check agent status"),
        }
    }

    manager.remove_process(name).await;
    tracing::info!(%name, "agent stopped");
    Ok(())
}
