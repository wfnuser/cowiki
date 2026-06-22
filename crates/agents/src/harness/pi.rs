//! PiAgentHandle — spawn + setup, delegate to protocol::pi_rpc.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::process::Command;
use tokio::sync::RwLock;

use super::{AgentContext, AgentHandle, AgentStart, AgentState};
use crate::manager::AgentProcess;
use crate::types::error::AgentError;

pub struct PiAgentHandle {
    pub agent_home: PathBuf,
    pub pi_binary: PathBuf,
    pub server_url: String,
    pub llm: cowiki_utils::LlmConfig,
    pub hard_timeout_secs: u64,
    pub soft_timeout_secs: u64,
    pub process_map: Option<Arc<RwLock<HashMap<String, AgentProcess>>>>,
}

impl PiAgentHandle {
    pub fn new(
        agent_home: PathBuf,
        server_url: String,
        llm: cowiki_utils::LlmConfig,
        hard_timeout_secs: u64,
        soft_timeout_secs: u64,
    ) -> Self {
        let pi_binary = std::env::var("PI_BINARY")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("pi"));
        Self {
            agent_home,
            pi_binary,
            server_url,
            llm,
            hard_timeout_secs,
            soft_timeout_secs,
            process_map: None,
        }
    }

    pub fn with_process_map(mut self, m: Arc<RwLock<HashMap<String, AgentProcess>>>) -> Self {
        self.process_map = Some(m);
        self
    }

    pub async fn setup(&self) -> Result<(), AgentError> {
        tokio::fs::create_dir_all(&self.agent_home)
            .await
            .map_err(|e| AgentError::InvalidConfig(format!("agent home: {e}")))?;
        let base = if self.llm.api_base.is_empty() {
            "https://api.openai.com/v1"
        } else {
            &self.llm.api_base
        };
        let json = serde_json::json!({
            "providers": { &self.llm.provider: {
                "baseUrl": base, "api": "openai-completions", "apiKey": "$OPENAI_API_KEY",
                "models": [{ "id": &self.llm.model }]
            }}
        });
        let d = self.agent_home.join(".pi").join("agent");
        tokio::fs::create_dir_all(&d)
            .await
            .map_err(|e| AgentError::InvalidConfig(format!(".pi/agent: {e}")))?;
        tokio::fs::write(
            d.join("models.json"),
            serde_json::to_string_pretty(&json).unwrap(),
        )
        .await
        .map_err(|e| AgentError::InvalidConfig(format!("models.json: {e}")))?;
        Ok(())
    }

    fn spawn(&self, api_key: &str) -> Result<tokio::process::Child, AgentError> {
        let pi_config = self.agent_home.join(".pi").join("agent");
        let skills = self.agent_home.join("skills");
        let mut cmd = Command::new(&self.pi_binary);
        cmd.arg("--mode").arg("rpc");
        cmd.arg("--no-session");
        cmd.arg("--no-builtin-tools");
        cmd.arg("--tools").arg("read,bash");
        cmd.arg("--skill").arg(skills.to_string_lossy().as_ref());
        cmd.arg("--provider").arg(&self.llm.provider);
        cmd.arg("--model").arg(&self.llm.model);
        cmd.arg("--approve");
        cmd.current_dir(&self.agent_home);
        cmd.env("PI_CODING_AGENT_DIR", pi_config.to_string_lossy().as_ref());
        cmd.env("OPENAI_API_KEY", &self.llm.api_key);
        cmd.env("COWIKI_BASE_URL", &self.server_url);
        if !api_key.is_empty() {
            cmd.env("COWIKI_API_KEY", api_key);
        }
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        cmd.kill_on_drop(true);
        tracing::info!(pi=%self.pi_binary.display(), "pi spawn");
        cmd.spawn()
            .map_err(|e| AgentError::Transport(format!("spawn pi: {e}")))
    }
}

#[async_trait]
impl AgentHandle for PiAgentHandle {
    async fn connect(
        &self,
        ctx: AgentContext,
        start: AgentStart,
    ) -> Result<AgentState, AgentError> {
        self.setup().await?;
        let child = self.spawn(&ctx.api_key)?;
        crate::protocol::pi_rpc::run(
            child,
            &start.prompt,
            &ctx.execution_id,
            self.hard_timeout_secs,
            self.soft_timeout_secs,
        )
        .await
    }
    async fn close(&self) -> Result<(), AgentError> {
        Ok(())
    }
    fn agent_type(&self) -> &str {
        "pi"
    }
}
