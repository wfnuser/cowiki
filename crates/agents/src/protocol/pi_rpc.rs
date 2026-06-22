//! pi RPC protocol — NDJSON stdin/stdout event loop.
//!
//! Agent-agnostic: accepts an already-spawned pi `Child` process.
//! Harness is responsible for spawning pi with the right args/env.

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Child;

use crate::harness::AgentState;
use crate::types::error::AgentError;
use crate::types::protocol::UsageInfo;

// ── Public API ──────────────────────────────────────────────────

/// Send prompt + read NDJSON events on an already-spawned pi process.
pub async fn run(
    mut child: Child,
    prompt: &str,
    task_id: &str,
    hard_timeout_secs: u64,
    soft_timeout_secs: u64,
) -> Result<AgentState, AgentError> {
    // Send prompt on stdin
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| AgentError::Transport("pi stdin unavailable".into()))?;
    let prompt_json = serde_json::json!({"type": "prompt", "message": prompt});
    let mut line = serde_json::to_string(&prompt_json).unwrap();
    line.push('\n');
    stdin
        .write_all(line.as_bytes())
        .await
        .map_err(|e| AgentError::Transport(format!("write prompt: {e}")))?;
    stdin
        .flush()
        .await
        .map_err(|e| AgentError::Transport(format!("flush stdin: {e}")))?;

    // Parse NDJSON stdout events
    let mut success = false;
    let mut rounds: u32 = 0;
    let hard_timeout = std::time::Duration::from_secs(hard_timeout_secs);
    let soft_timeout = std::time::Duration::from_secs(soft_timeout_secs);
    let started_at = std::time::Instant::now();

    if let Some(stdout) = child.stdout.take() {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        loop {
            tokio::select! {
                line = lines.next_line() => {
                    match line {
                        Ok(Some(line)) => {
                            let line = line.trim().to_string();
                            if line.is_empty() { continue; }
                            match serde_json::from_str::<serde_json::Value>(&line) {
                                Ok(event) => {
                                    let etype = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
                                    match etype {
                                        "response" => {
                                            let cmd = event.get("command").and_then(|v| v.as_str()).unwrap_or("");
                                            let ok = event.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
                                            if !ok && cmd == "prompt" {
                                                let err = event.get("error").and_then(|v| v.as_str()).unwrap_or("?");
                                                tracing::error!(%err, "pi prompt rejected");
                                            }
                                        }
                                        "tool_execution_start" => {
                                            let tool = event.get("toolName").and_then(|v| v.as_str()).unwrap_or("?");
                                            tracing::info!(%tool, %task_id, "pi tool start");
                                        }
                                        "tool_execution_end" => {
                                            let tool = event.get("toolName").and_then(|v| v.as_str()).unwrap_or("?");
                                            let is_error = event.get("isError").and_then(|v| v.as_bool()).unwrap_or(false);
                                            tracing::info!(%tool, is_error, %task_id, "pi tool end");
                                        }
                                        "agent_end" => {
                                            success = true;
                                            if let Some(msgs) = event.get("messages").and_then(|v| v.as_array()) {
                                                rounds = msgs.len() as u32;
                                            }
                                            tracing::info!(rounds, %task_id, "pi agent end");
                                            break;
                                        }
                                        "agent_start" => tracing::info!(%task_id, "pi agent start"),
                                        _ => tracing::debug!(%etype, "pi event"),
                                    }
                                }
                                Err(e) => tracing::warn!(%line, error=%e, "pi parse error"),
                            }
                        }
                        Ok(None) => break,
                        Err(e) => { tracing::error!(%e, "pi stdout error"); break; }
                    }
                }
                _ = tokio::time::sleep(hard_timeout) => {
                    tracing::error!(%task_id, "pi hard timeout: {}s", hard_timeout_secs);
                    let _ = child.kill().await;
                    return Err(AgentError::Timeout);
                }
                _ = tokio::time::sleep(soft_timeout.saturating_sub(started_at.elapsed())) => {
                    tracing::warn!(%task_id, elapsed=?started_at.elapsed(), "pi soft timeout, continuing...");
                }
            }
            if success {
                break;
            }
        }
    }

    // Stderr reader
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::warn!(agent="pi", stderr=%line, "pi stderr");
            }
        });
    }

    drop(stdin);
    let status = child
        .wait()
        .await
        .map_err(|e| AgentError::Transport(format!("pi wait: {e}")))?;

    tracing::info!(exit=?status.code(), %task_id, "pi exited");

    if success || status.success() {
        Ok(AgentState {
            success: true,
            rounds,
            usage: UsageInfo::default(),
            error: None,
        })
    } else {
        Ok(AgentState {
            success: false,
            rounds: 0,
            usage: UsageInfo::default(),
            error: Some(format!("pi: {status}")),
        })
    }
}

// ── NDJSON → ACP SessionUpdate ──────────────────────────────────

pub fn to_acp_update(event: &serde_json::Value) -> Option<serde_json::Value> {
    let etype = event.get("type").and_then(|v| v.as_str())?;
    match etype {
        "tool_execution_start" => {
            let id = event
                .get("toolCallId")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("pi-{}", uuid::Uuid::new_v4()));
            Some(
                serde_json::json!({"type":"tool_call","toolCallId":id,"toolName":event.get("toolName"),"args":event.get("args"),"agentType":"pi"}),
            )
        }
        "tool_execution_end" => {
            let id = event
                .get("toolCallId")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("pi-{}", uuid::Uuid::new_v4()));
            let is_error = event
                .get("isError")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let mut u = serde_json::json!({"type":"tool_call_update","toolCallId":id,"status":if is_error {"failed"} else {"completed"},"agentType":"pi"});
            if is_error {
                if let Some(e) = event.get("error") {
                    u["error"] = serde_json::json!(e.as_str().unwrap_or(""))
                }
            }
            if let Some(o) = event.get("output") {
                u["output"] = o.clone();
            }
            Some(u)
        }
        "agent_start" => Some(serde_json::json!({"type":"run_started","agentType":"pi"})),
        "agent_end" => {
            let n = event
                .get("messages")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            Some(
                serde_json::json!({"type":"run_finished","stopReason":"end_turn","messagesCount":n,"agentType":"pi"}),
            )
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_start() {
        let e = serde_json::json!({"type":"tool_execution_start","toolCallId":"t1","toolName":"write","args":{"x":1}});
        let u = to_acp_update(&e).unwrap();
        assert_eq!(u["type"], "tool_call");
        assert_eq!(u["toolCallId"], "t1");
    }

    #[test]
    fn test_agent_end() {
        let e = serde_json::json!({"type":"agent_end","messages":[{},{},{}]});
        let u = to_acp_update(&e).unwrap();
        assert_eq!(u["type"], "run_finished");
        assert_eq!(u["messagesCount"], 3);
    }

    #[test]
    fn test_unknown_is_none() {
        assert!(to_acp_update(&serde_json::json!({"type":"???"})).is_none());
    }
}
