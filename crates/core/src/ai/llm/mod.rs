pub mod openai;
pub mod registry;

use std::collections::HashMap;

use async_trait::async_trait;
use serde::Serialize;

use super::token_usage::TokenUsage;

/// Configuration for an LLM (Language Model) provider.
#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub provider: String,
    pub model: String,
    pub api_key: String,
    pub api_base: String,
    pub temperature: f64,
    pub max_tokens: Option<u32>,
}

/// Response from an LLM chat completion.
#[derive(Debug, Clone, Serialize)]
pub struct LlmResponse {
    pub content: String,
    pub usage: Option<TokenUsage>,
    pub finish_reason: Option<String>,
}

/// Trait for LLM providers.
#[async_trait]
pub trait Llm: Send + Sync {
    /// Send a chat completion request with system + user messages.
    async fn chat(&self, system: &str, user: &str) -> Result<LlmResponse, String>;

    /// Get a snapshot of tracked token usage by model.
    fn usage_snapshot(&self) -> HashMap<String, TokenUsage>;
}

/// Factory: create an LLM instance from configuration.
/// Dispatches to the correct provider implementation based on `config.provider`.
pub fn create_llm(config: LlmConfig) -> Box<dyn Llm> {
    match config.provider.as_str() {
        "openai" => Box::new(openai::OpenAILlm::new(config)),
        other => panic!("unsupported LLM provider: {other}"),
    }
}
