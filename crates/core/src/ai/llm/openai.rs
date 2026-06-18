use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::super::token_usage::{TokenUsage, TokenUsageTracker};
use super::{Llm, LlmConfig, LlmResponse};

pub struct OpenAILlm {
    client: Client,
    config: LlmConfig,
    tracker: Arc<Mutex<TokenUsageTracker>>,
}

// ── OpenAI API types ──

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
struct UsageInfo {
    prompt_tokens: u32,
    completion_tokens: u32,
}

impl OpenAILlm {
    pub fn new(config: LlmConfig) -> Self {
        Self {
            client: Client::new(),
            config,
            tracker: Arc::new(Mutex::new(TokenUsageTracker::default())),
        }
    }

    /// Get a clone of the Arc<Mutex<>> tracker for sharing.
    pub fn tracker_arc(&self) -> Arc<Mutex<TokenUsageTracker>> {
        Arc::clone(&self.tracker)
    }
}

#[async_trait]
impl Llm for OpenAILlm {
    async fn chat(&self, system: &str, user: &str) -> Result<LlmResponse, String> {
        let url = format!(
            "{}/chat/completions",
            self.config.api_base.trim_end_matches('/')
        );

        let response = self
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
                    "LLM API request failed for model {}: {e}",
                    self.config.model
                );
                e.to_string()
            })?;

        // Surface HTTP errors (401/429/500/…) instead of masking them as a generic
        // JSON-parse failure when the error body fails to deserialize as ChatResponse.
        let status = response.status();
        let body = response.text().await.map_err(|e| {
            tracing::error!(
                "LLM API read body failed for model {}: {e}",
                self.config.model
            );
            e.to_string()
        })?;
        if !status.is_success() {
            let preview: String = body.chars().take(500).collect();
            tracing::error!(
                "LLM API error for model {} (status {status}): {preview}",
                self.config.model
            );
            return Err(format!("LLM API error (status {status}): {preview}"));
        }
        let resp: ChatResponse = serde_json::from_str(&body).map_err(|e| {
            let preview: String = body.chars().take(500).collect();
            tracing::error!(
                "LLM API response parse failed for model {} (status {status}): {e}\nResponse: {preview}",
                self.config.model
            );
            format!("LLM parse error (status {status}): {e}")
        })?;

        let content = resp
            .choices
            .first()
            .and_then(|c| c.message.content.clone())
            .unwrap_or_default();

        let finish_reason = resp.choices.first().and_then(|c| c.finish_reason.clone());

        let usage = resp.usage.map(|u| {
            if let Ok(mut tracker) = self.tracker.lock() {
                tracker.record(
                    &self.config.model,
                    u.prompt_tokens as u64,
                    u.completion_tokens as u64,
                );
            }

            let mut tu = TokenUsage::default();
            tu.update(u.prompt_tokens as u64, u.completion_tokens as u64);
            tu
        });

        Ok(LlmResponse {
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
