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
    let proc_entry = {
        let mut procs = manager.processes.write().await;
        procs.remove(name)
    };

    let mut proc = match proc_entry {
        Some(p) => p,
        None => return Ok(()),
    };

    if matches!(proc.status, AgentStatus::Stopped) {
        return Ok(());
    }
    proc.status = AgentStatus::Stopped;

    // Try child handle first, fall back to PID
    if let Some(ref mut c) = proc.child {
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
    } else if let Some(pid) = proc.pid {
        tracing::info!(%name, pid, "sending SIGTERM to agent (by PID)");
        // Send SIGTERM via command
        let pid_str = pid.to_string();
        let _ = tokio::process::Command::new("kill").arg(&pid_str).spawn();
        sleep(Duration::from_secs(SHUTDOWN_GRACE_PERIOD_SECS)).await;
        let _ = tokio::process::Command::new("kill")
            .arg("-9")
            .arg(&pid_str)
            .spawn();
    }

    tracing::info!(%name, "agent stopped");
    Ok(())
}
