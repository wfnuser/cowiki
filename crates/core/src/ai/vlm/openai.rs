use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::super::token_usage::{TokenUsage, TokenUsageTracker};
use super::{Vlm, VlmConfig, VlmResponse};

pub struct OpenAIVlm {
    client: Client,
    config: VlmConfig,
    tracker: Arc<Mutex<TokenUsageTracker>>,
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<Message<'a>>,
    temperature: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

#[derive(Serialize, Clone)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
    usage: Option<UsageInfo>,
}

#[derive(Deserialize)]
struct Choice {
    message: ChoiceMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ChoiceMessage {
    content: Option<String>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct UsageInfo {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

impl OpenAIVlm {
    pub fn new(config: VlmConfig) -> Self {
        Self {
            client: Client::new(),
            config,
            tracker: Arc::new(Mutex::new(TokenUsageTracker::default())),
        }
    }

    pub fn tracker_arc(&self) -> Arc<Mutex<TokenUsageTracker>> {
        Arc::clone(&self.tracker)
    }
}

#[async_trait]
impl Vlm for OpenAIVlm {
    async fn chat(&self, system: &str, user: &str) -> Result<VlmResponse, String> {
        let url = format!(
            "{}/chat/completions",
            self.config.api_base.trim_end_matches('/')
        );

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.config.api_key)
            .json(&ChatRequest {
                model: &self.config.model,
                messages: vec![
                    Message {
                        role: "system",
                        content: system,
                    },
                    Message {
                        role: "user",
                        content: user,
                    },
                ],
                temperature: self.config.temperature,
                max_tokens: self.config.max_tokens,
            })
            .send()
            .await
            .map_err(|e| {
                tracing::error!(
                    "VLM API request failed for model {}: {e}",
                    self.config.model
                );
                e.to_string()
            })?
            .json::<ChatResponse>()
            .await
            .map_err(|e| {
                tracing::error!(
                    "VLM API response parse failed for model {}: {e}",
                    self.config.model
                );
                e.to_string()
            })?;

        let content = resp
            .choices
            .first()
            .and_then(|c| c.message.content.clone())
            .unwrap_or_default();

        let finish_reason = resp.choices.first().and_then(|c| c.finish_reason.clone());

        let usage = resp.usage.map(|u| {
            if let Ok(mut tracker) = self.tracker.lock() {
                tracker.record(&self.config.model, u.prompt_tokens, u.completion_tokens);
            }
            let mut tu = TokenUsage::default();
            tu.update(u.prompt_tokens, u.completion_tokens);
            tu
        });

        Ok(VlmResponse {
            content,
            usage,
            finish_reason,
        })
    }

    fn usage_snapshot(&self) -> HashMap<String, TokenUsage> {
        self.tracker
            .lock()
            .map(|t| t.all().clone())
            .unwrap_or_default()
    }
}
