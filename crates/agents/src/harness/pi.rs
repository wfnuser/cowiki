//! PiAgentHandle — spawn `pi -p`, wait for process exit.
//!
//! pi runs in non-interactive mode (`-p`), calling MCP tools to operate on wiki.
//! No stdout parsing needed — written_pages tracked by WikiFsGateway.

use std::path::PathBuf;

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::broadcast;

use super::{AgentContext, AgentHandle, AgentStart, AgentState};
use crate::stream::AgentEvent;
use crate::types::error::AgentError;
use crate::types::protocol::UsageInfo;

pub struct PiAgentHandle {
    pub agent_home: PathBuf,
    pub pi_binary: PathBuf,
    pub mcp_url: String,
    pub llm: cowiki_utils::LlmConfig,
    /// Optional event channel for broadcasting tool_start/tool_end events
    pub event_tx: Option<broadcast::Sender<AgentEvent>>,
}

impl PiAgentHandle {
    pub fn new(agent_home: PathBuf, mcp_url: String, llm: cowiki_utils::LlmConfig) -> Self {
        let pi_binary = std::env::var("PI_BINARY")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("pi"));
        Self {
            agent_home,
            pi_binary,
            mcp_url,
            llm,
            event_tx: None,
        }
    }

    /// Attach an event channel for broadcasting tool execution events.
    pub fn with_event_tx(mut self, tx: broadcast::Sender<AgentEvent>) -> Self {
        self.event_tx = Some(tx);
        self
    }

    /// Ensure agent home + write models.json + write skill files.
    pub async fn setup(&self, task_type: &str) -> Result<(), AgentError> {
        tokio::fs::create_dir_all(&self.agent_home)
            .await
            .map_err(|e| AgentError::InvalidConfig(format!("agent home: {e}")))?;

        // models.json — pi reads provider/model from here, apiKey via env var
        let base = if self.llm.api_base.is_empty() {
            "https://api.openai.com/v1"
        } else {
            &self.llm.api_base
        };
        let provider = &self.llm.provider;
        let model = &self.llm.model;
        let json = serde_json::json!({
            "providers": { provider: {
                "baseUrl": base,
                "api": "openai-completions",
                "apiKey": "$OPENAI_API_KEY",
                "models": [{ "id": model }]
            }}
        });
        let models_dir = self.agent_home.join(".pi").join("agent");
        tokio::fs::create_dir_all(&models_dir)
            .await
            .map_err(|e| AgentError::InvalidConfig(format!(".pi/agent dir: {e}")))?;
        tokio::fs::write(
            models_dir.join("models.json"),
            serde_json::to_string_pretty(&json).unwrap(),
        )
        .await
        .map_err(|e| AgentError::InvalidConfig(format!("models.json: {e}")))?;

        // Skills
        write_skills(&self.agent_home, task_type).await?;

        // MCP config — pi-mcp-adapter auto-discovers .mcp.json
        write_mcp_config(&self.agent_home, &self.mcp_url).await?;

        Ok(())
    }
}

#[async_trait]
impl AgentHandle for PiAgentHandle {
    async fn connect(
        &self,
        ctx: AgentContext,
        start: AgentStart,
    ) -> Result<AgentState, AgentError> {
        self.setup(&ctx.task_type).await?;

        let pi_config_dir = self.agent_home.join(".pi").join("agent");
        let prompt = build_prompt(&start.prompt, &ctx);

        tracing::info!("Prompt: {}", prompt);

        let mut cmd = Command::new(&self.pi_binary);
        // RPC mode: structured NDJSON on stdin/stdout
        cmd.arg("--mode").arg("rpc");
        cmd.arg("--no-session");
        // Tool restrictions: `read` (skill files) + `mcp` (adapter session mgmt) + cowiki_* direct tools.
        // No grep, glob, bash, write, edit — prevents filesystem exploration.
        // All wiki operations go through cowiki_* direct tools (pi-mcp-adapter directTools mode).
        // `mcp` is kept for adapter's internal session management (auth, reconnect).
        cmd.arg("--no-builtin-tools");
        let tools = match ctx.task_type.as_str() {
            "review" => "read,mcp,cowiki_list,cowiki_read,cowiki_search",
            _ => "read,mcp,cowiki_list,cowiki_read,cowiki_write,cowiki_remove,cowiki_search",
        };
        cmd.arg("--tools").arg(tools);
        // Explicit skill path (so pi reads from here, not scanning the entire fs)
        let skills_dir = self.agent_home.join(".pi").join("agent").join("skills");
        cmd.arg("--skill")
            .arg(skills_dir.to_string_lossy().as_ref());
        // Load MCP extension explicitly — PI_CODING_AGENT_DIR overrides default
        // config path, so pi can't auto-discover globally installed extensions.
        let home = std::env::var("HOME").unwrap_or_else(|_| "/home/root".into());
        let mcp_ext = std::path::PathBuf::from(home)
            .join(".pi")
            .join("agent")
            .join("npm")
            .join("node_modules")
            .join("pi-mcp-adapter");
        if mcp_ext.exists() {
            cmd.arg("--extension")
                .arg(mcp_ext.to_string_lossy().as_ref());
        }
        // MCP config — pi-mcp-adapter reads .mcp.json from this path
        let mcp_config = self.agent_home.join(".mcp.json");
        cmd.arg("--mcp-config")
            .arg(mcp_config.to_string_lossy().as_ref());
        // Provider/model
        cmd.arg("--provider");
        cmd.arg(&self.llm.provider);
        cmd.arg("--model");
        cmd.arg(&self.llm.model);
        // Approve all operations
        cmd.arg("--approve");
        // Working dir and env — restrict to agent_home scope
        // .mcp.json is auto-discovered from working dir
        cmd.current_dir(&self.agent_home);
        cmd.env(
            "PI_CODING_AGENT_DIR",
            pi_config_dir.to_string_lossy().as_ref(),
        );
        cmd.env("OPENAI_API_KEY", &self.llm.api_key);
        // Stdio
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        cmd.kill_on_drop(true);

        tracing::info!(
            pi = %self.pi_binary.display(),
            config_dir = %pi_config_dir.display(),
            agent_home = %self.agent_home.display(),
            workspace = %ctx.workspace,
            branch = %ctx.branch,
            "spawning pi agent (rpc mode)"
        );

        let mut child = cmd
            .spawn()
            .map_err(|e| AgentError::Transport(format!("spawn pi: {e}")))?;

        // Send prompt via stdin (RPC protocol).
        // NOTE: keep stdin open while pi works — pi needs the pipe alive to
        // process the prompt. Close it only after agent_end to signal exit.
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| AgentError::Transport("pi stdin unavailable".into()))?;
        let prompt_json = serde_json::json!({
            "type": "prompt",
            "message": prompt,
        });
        let mut line = serde_json::to_string(&prompt_json).unwrap();
        line.push('\n');
        stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|e| AgentError::Transport(format!("write prompt to pi: {e}")))?;
        stdin
            .flush()
            .await
            .map_err(|e| AgentError::Transport(format!("flush pi stdin: {e}")))?;

        // Parse stdout events (NDJSON)
        let event_tx = self.event_tx.clone();
        let agent_name = "compiler".to_string();
        let task_id = ctx.execution_id.clone();
        let mut success = false;
        let mut rounds: u32 = 0;

        if let Some(stdout) = child.stdout.take() {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let line = line.trim().to_string();
                if line.is_empty() {
                    continue;
                }

                match serde_json::from_str::<serde_json::Value>(&line) {
                    Ok(event) => {
                        let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
                        match event_type {
                            "response" => {
                                let cmd =
                                    event.get("command").and_then(|v| v.as_str()).unwrap_or("");
                                let ok = event
                                    .get("success")
                                    .and_then(|v| v.as_bool())
                                    .unwrap_or(false);
                                if !ok && cmd == "prompt" {
                                    let err = event
                                        .get("error")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("unknown");
                                    tracing::error!(%err, "pi prompt rejected");
                                }
                            }
                            "tool_execution_start" => {
                                let tool = event
                                    .get("toolName")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("unknown");
                                let args = event.get("args").cloned();
                                tracing::info!(%tool, ?args, task_id=%task_id, "pi tool start");
                                if let Some(ref tx) = event_tx {
                                    let _ = tx.send(AgentEvent::ToolStart {
                                        agent: agent_name.clone(),
                                        task_id: task_id.clone(),
                                        tool: tool.to_string(),
                                        input: args,
                                    });
                                }
                            }
                            "tool_execution_end" => {
                                let tool = event
                                    .get("toolName")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("unknown");
                                let is_error = event
                                    .get("isError")
                                    .and_then(|v| v.as_bool())
                                    .unwrap_or(false);
                                tracing::info!(%tool, is_error, task_id=%task_id, "pi tool end");
                                if let Some(ref tx) = event_tx {
                                    let _ = tx.send(AgentEvent::ToolEnd {
                                        agent: agent_name.clone(),
                                        task_id: task_id.clone(),
                                        tool: tool.to_string(),
                                        success: !is_error,
                                        summary: String::new(),
                                    });
                                }
                            }
                            "agent_end" => {
                                success = true;
                                let msgs = event.get("messages").and_then(|v| v.as_array());
                                if let Some(msgs) = msgs {
                                    rounds = msgs.len() as u32;
                                }
                                tracing::info!(rounds, task_id=%task_id, "pi agent end");
                                break;
                            }
                            "agent_start" => {
                                if let Some(ref tx) = event_tx {
                                    let _ = tx.send(AgentEvent::TaskStarted {
                                        agent: agent_name.clone(),
                                        task_id: task_id.clone(),
                                    });
                                }
                            }
                            _ => {
                                tracing::debug!(%event_type, ?event, "pi event");
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(%line, error=%e, "failed to parse pi stdout event");
                    }
                }
            }
        }

        // stderr → tracing (spawn background task)
        if let Some(stderr) = child.stderr.take() {
            let label = "pi".to_string();
            tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::warn!(agent = %label, stderr = %line, "pi stderr");
                }
            });
        }

        // Close stdin to signal pi to exit, then wait for the process.
        drop(stdin);

        let status = child
            .wait()
            .await
            .map_err(|e| AgentError::Transport(format!("pi wait: {e}")))?;

        tracing::info!(exit_code = ?status.code(), task_id=%task_id, "pi process exited");

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
                error: Some(format!("pi exited: {status}")),
            })
        }
    }

    async fn close(&self) -> Result<(), AgentError> {
        Ok(())
    }
    fn agent_type(&self) -> &str {
        "pi"
    }
}

// ── Helpers ────────────────────────────────────────────────────

use crate::manager::{
    DEFAULT_COMPILER_SKILL, DEFAULT_CONVENTIONS_MD, DEFAULT_DEEP_COMPILE_SKILL,
    DEFAULT_REVIEW_SKILL, DEFAULT_TOOLS_MD,
};

// ── Prompt builder ──────────────────────────────────────────────

/// Build the pi prompt with workspace/branch/execution_id context.
fn build_prompt(user_input: &str, ctx: &AgentContext) -> String {
    format!(
        "{}\n\n---\n## Session Context\n\
         workspace: {}\nbranch: {}\nexecution_id: {}\n\
         When calling MCP tools, always use the cowiki_ prefix: cowiki_list, cowiki_read, cowiki_write, cowiki_remove, cowiki_search.\n\
         Always pass _workspace=\"{}\", _branch=\"{}\", _execution_id=\"{}\" as parameters.",
        user_input, ctx.workspace, ctx.branch, ctx.execution_id,
        ctx.workspace, ctx.branch, ctx.execution_id
    )
}

// ── MCP config (.mcp.json for pi-mcp-adapter) ────────────────

async fn write_mcp_config(home: &std::path::Path, mcp_url: &str) -> Result<(), AgentError> {
    let config = serde_json::json!({
        "mcpServers": { "cowiki": {
            "url": mcp_url,
            "lifecycle": "keep-alive",
            "directTools": true
        }}
    });
    tokio::fs::write(
        home.join(".mcp.json"),
        serde_json::to_string_pretty(&config).unwrap(),
    )
    .await
    .map_err(|e| AgentError::InvalidConfig(format!(".mcp.json: {e}")))?;
    Ok(())
}

async fn write_skills(home: &std::path::Path, task_type: &str) -> Result<(), AgentError> {
    let dir = home.join(".pi").join("agent").join("skills");
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| AgentError::InvalidConfig(format!("skills dir: {e}")))?;

    // Write common files (shared across all task types)
    let common: &[(&str, &str)] = &[
        ("_tools.md", DEFAULT_TOOLS_MD),
        ("_conventions.md", DEFAULT_CONVENTIONS_MD),
    ];
    for (name, content) in common {
        let path = dir.join(name);
        tokio::fs::write(&path, content)
            .await
            .map_err(|e| AgentError::InvalidConfig(format!("skill {name}: {e}")))?;
    }

    // Write task-specific SKILL.md
    let (skill_name, skill_content) = match task_type {
        "compile" => ("SKILL.md", DEFAULT_COMPILER_SKILL),
        "deep-compile" => ("SKILL.md", DEFAULT_DEEP_COMPILE_SKILL),
        "review" => ("SKILL.md", DEFAULT_REVIEW_SKILL),
        _ => ("SKILL.md", DEFAULT_COMPILER_SKILL),
    };
    let skill_path = dir.join(skill_name);
    tokio::fs::write(&skill_path, skill_content)
        .await
        .map_err(|e| AgentError::InvalidConfig(format!("skill {skill_name}: {e}")))?;

    let skills_json = serde_json::json!([{
        "name": task_type,
        "description": format!("{task_type} task workflow"),
        "dir": ".pi/agent/skills"
    }]);
    tokio::fs::write(
        home.join(".pi").join("agent").join("skills.json"),
        serde_json::to_string_pretty(&skills_json).unwrap(),
    )
    .await
    .map_err(|e| AgentError::InvalidConfig(format!("skills.json: {e}")))?;
    Ok(())
}
