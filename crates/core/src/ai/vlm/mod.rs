pub mod openai;
pub mod registry;

use std::collections::HashMap;

use async_trait::async_trait;
use serde::Serialize;

use super::token_usage::TokenUsage;

/// Configuration for a VLM (Vision Language Model) provider.
#[derive(Debug, Clone)]
pub struct VlmConfig {
    pub provider: String,
    pub model: String,
    pub api_key: String,
    pub api_base: String,
    pub temperature: f64,
    pub max_tokens: Option<u32>,
}

/// Response from a VLM completion.
#[derive(Debug, Clone, Serialize)]
pub struct VlmResponse {
    pub content: String,
    pub usage: Option<TokenUsage>,
    pub finish_reason: Option<String>,
}

/// Trait for VLM providers (vision-capable chat).
/// Currently uses the same text-chat API as LLM; vision methods will be added
/// as the trait evolves (e.g. `chat_with_images`).
#[async_trait]
pub trait Vlm: Send + Sync {
    /// Send a chat completion request with system + user messages.
    async fn chat(&self, system: &str, user: &str) -> Result<VlmResponse, String>;

    /// Get a snapshot of tracked token usage by model.
    fn usage_snapshot(&self) -> HashMap<String, TokenUsage>;
}

/// Factory: create a VLM instance from configuration.
pub fn create_vlm(config: VlmConfig) -> Box<dyn Vlm> {
    match config.provider.as_str() {
        "openai" => Box::new(openai::OpenAIVlm::new(config)),
        other => panic!("unsupported VLM provider: {other}"),
    }
}
