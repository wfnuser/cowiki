//! ACP protocol — handshake + event loop via `agent-client-protocol` SDK.
//!
//! Agent-agnostic: accepts an already-spawned `Child` process.
//! Wraps tokio ChildStdio in `futures::compat` so the SDK's `ByteStreams` can use it.

use std::time::Duration;

use agent_client_protocol::schema::v1::{InitializeRequest, SessionNotification, SessionUpdate};
use agent_client_protocol::{ByteStreams, Client};
use tokio::process::Child;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::types::error::AgentError;

pub use agent_client_protocol as acp_sdk;

pub struct RunResult {
    pub success: bool,
    pub error: Option<String>,
}

/// Run ACP handshake on an already-spawned agent process.
/// Uses the SDK's Client builder + ByteStreams transport wrapping the child's stdio.
pub async fn run(
    mut child: Child,
    prompt: &str,
    hard_timeout_secs: u64,
) -> Result<RunResult, AgentError> {
    tracing::info!(prompt_len = prompt.len(), "acp: starting SDK handshake");

    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| AgentError::Transport("acp stdin unavailable".into()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AgentError::Transport("acp stdout unavailable".into()))?;

    // Spawn stderr reader
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(async move {
            use tokio::io::AsyncBufReadExt;
            let reader = tokio::io::BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::warn!(stderr = %line, "acp agent stderr");
            }
        });
    }

    // Wrap tokio types in futures::compat for the SDK
    let transport = ByteStreams::new(stdin.compat_write(), stdout.compat());

    let hard_timeout = Duration::from_secs(hard_timeout_secs);

    let result = tokio::time::timeout(hard_timeout, async {
        Client
            .builder()
            .name("cowiki")
            .on_receive_notification(
                async |notif: SessionNotification, _cx| {
                    match &notif.update {
                        SessionUpdate::ToolCall(tc) => {
                            let cmd = tc
                                .raw_input
                                .as_ref()
                                .and_then(|v| v.get("command"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            tracing::info!(title = %tc.title, %cmd, "acp: tool call");
                        }
                        SessionUpdate::ToolCallUpdate(tu) => {
                            let status = tu
                                .fields
                                .status
                                .as_ref()
                                .map(|s| format!("{:?}", s))
                                .unwrap_or_default();
                            tracing::info!(%status, "acp: tool update");
                        }
                        _ => {}
                    }
                    Ok(())
                },
                agent_client_protocol::on_receive_notification!(),
            )
            .connect_with(
                transport,
                async |cx| -> Result<(), agent_client_protocol::Error> {
                    cx.send_request(InitializeRequest::new(
                        agent_client_protocol::schema::ProtocolVersion::V1,
                    ))
                    .block_task()
                    .await?;
                    tracing::info!("acp: initialize done");

                    cx.build_session_cwd()?
                    .block_task()
                    .run_until(async |mut session| -> Result<(), agent_client_protocol::Error> {
                        session.send_prompt(prompt)?;
                        tracing::info!("acp: prompt sent");
                        let output = session.read_to_string().await?;
                        tracing::info!(len = output.len(), text = %output, "acp: agent finished");
                        Ok(())
                    })
                    .await
                },
            )
            .await
    })
    .await;

    let _ = child.wait().await;

    match result {
        Ok(Ok(())) => Ok(RunResult {
            success: true,
            error: None,
        }),
        Ok(Err(e)) => Ok(RunResult {
            success: false,
            error: Some(format!("{e}")),
        }),
        Err(_) => Err(AgentError::Timeout),
    }
}
