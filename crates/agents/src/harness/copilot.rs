//! CopilotAgentHandle — spawn + setup, delegate to protocol::acp.

use std::path::PathBuf;

use async_trait::async_trait;
use tokio::process::Command;

use super::{AgentContext, AgentHandle, AgentStart, AgentState};
use crate::types::error::AgentError;
use crate::types::protocol::UsageInfo;

pub struct CopilotAgentHandle {
    pub agent_home: PathBuf,
    pub server_url: String,
    pub llm: cowiki_utils::LlmConfig,
    pub hard_timeout_secs: u64,
    pub github_token: Option<String>,
}

impl CopilotAgentHandle {
    pub fn new(
        agent_home: PathBuf,
        server_url: String,
        llm: cowiki_utils::LlmConfig,
        hard_timeout_secs: u64,
        github_token: Option<String>,
    ) -> Self {
        Self {
            agent_home,
            server_url,
            llm,
            hard_timeout_secs,
            github_token,
        }
    }

    fn spawn(&self, api_key: &str) -> Result<tokio::process::Child, AgentError> {
        let (ptype, base_url, model) = resolve_provider(&self.llm);

        let mut cmd = Command::new("copilot");
        cmd.arg("--acp");
        cmd.arg("--allow-all");
        cmd.current_dir(&self.agent_home);
        cmd.env("COPILOT_PROVIDER_TYPE", &ptype);
        cmd.env("COPILOT_PROVIDER_BASE_URL", &base_url);
        cmd.env("COPILOT_PROVIDER_API_KEY", &self.llm.api_key);
        cmd.env("COPILOT_MODEL", &model);
        if self.llm.provider == "deepseek" {
            cmd.env("COPILOT_PROVIDER_MAX_PROMPT_TOKENS", "840000");
            cmd.env("COPILOT_PROVIDER_MAX_OUTPUT_TOKENS", "128000");
        }
        if let Some(ref t) = self.github_token {
            cmd.env("COPILOT_GITHUB_TOKEN", t);
        }
        cmd.env("COWIKI_BASE_URL", &self.server_url);
        if !api_key.is_empty() {
            cmd.env("COWIKI_API_KEY", api_key);
        }
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        cmd.kill_on_drop(true);

        tracing::info!(provider=%ptype, model=%model, "copilot spawn");
        cmd.spawn()
            .map_err(|e| AgentError::Transport(format!("spawn copilot: {e}")))
    }
}

#[async_trait]
impl AgentHandle for CopilotAgentHandle {
    async fn connect(
        &self,
        ctx: AgentContext,
        start: AgentStart,
    ) -> Result<AgentState, AgentError> {
        let child = self.spawn(&ctx.api_key)?;
        let r = crate::protocol::acp::run(child, &start.prompt, self.hard_timeout_secs).await;
        match r {
            Ok(rr) => Ok(AgentState {
                success: rr.success,
                rounds: 0,
                usage: UsageInfo::default(),
                error: rr.error,
            }),
            Err(e) => Ok(AgentState {
                success: false,
                rounds: 0,
                usage: UsageInfo::default(),
                error: Some(format!("{e}")),
            }),
        }
    }
    async fn close(&self) -> Result<(), AgentError> {
        Ok(())
    }
    fn agent_type(&self) -> &str {
        "copilot"
    }
}

fn resolve_deepseek_base(api_base: &str) -> String {
    if api_base.is_empty() {
        return "https://api.deepseek.com/anthropic".into();
    }
    if api_base.ends_with("/anthropic") {
        return api_base.to_string();
    }
    if api_base.ends_with('/') {
        format!("{api_base}anthropic")
    } else {
        format!("{api_base}/anthropic")
    }
}

fn resolve_provider(llm: &cowiki_utils::LlmConfig) -> (String, String, String) {
    match llm.provider.as_str() {
        "deepseek" => {
            let b = resolve_deepseek_base(&llm.api_base);
            let m = if llm.model.is_empty() {
                "deepseek-v4-flash".into()
            } else {
                llm.model.clone()
            };
            ("anthropic".into(), b, m)
        }
        "openai" => {
            let b = if llm.api_base.is_empty() {
                "https://api.openai.com/v1".into()
            } else {
                llm.api_base.clone()
            };
            let m = if llm.model.is_empty() {
                "gpt-4o".into()
            } else {
                llm.model.clone()
            };
            ("openai".into(), b, m)
        }
        "anthropic" | "claude" => {
            let b = if llm.api_base.is_empty() {
                "https://api.anthropic.com".into()
            } else {
                llm.api_base.clone()
            };
            let m = if llm.model.is_empty() {
                "claude-sonnet-4-20250514".into()
            } else {
                llm.model.clone()
            };
            ("anthropic".into(), b, m)
        }
        other => {
            let b = if llm.api_base.is_empty() {
                "https://api.openai.com/v1".into()
            } else {
                llm.api_base.clone()
            };
            let m = if llm.model.is_empty() {
                "gpt-4o".into()
            } else {
                llm.model.clone()
            };
            (other.to_string(), b, m)
        }
    }
}
