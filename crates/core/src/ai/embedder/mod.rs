use std::collections::HashMap;

use async_trait::async_trait;

use super::token_usage::TokenUsage;

pub mod openai;

/// Configuration for an embedding provider.
#[derive(Debug, Clone)]
pub struct EmbedderConfig {
    pub provider: String,
    pub model: String,
    pub api_key: String,
    pub api_base: String,
    pub dimension: u32,
}

/// Result of an embedding operation.
#[derive(Debug, Clone)]
pub struct EmbedResult {
    pub vector: Vec<f32>,
}

/// Trait for embedding providers.
#[async_trait]
pub trait Embedder: Send + Sync {
    /// Embed a single text.
    async fn embed(&self, text: &str, is_query: bool) -> Result<EmbedResult, String>;

    /// Embed multiple texts in batch.
    async fn embed_batch(
        &self,
        texts: &[String],
        is_query: bool,
    ) -> Result<Vec<EmbedResult>, String>;

    /// Get a snapshot of tracked token usage by model.
    fn usage_snapshot(&self) -> HashMap<String, TokenUsage>;
}

/// Factory: create an Embedder instance from configuration.
/// Dispatches to the correct provider implementation based on `config.provider`.
pub fn create_embedder(config: EmbedderConfig) -> Box<dyn Embedder> {
    match config.provider.as_str() {
        "openai" => Box::new(openai::OpenAIEmbedder::new(config)),
        other => panic!("unsupported embedder provider: {other}"),
    }
}
