//! Legacy synchronous compiler — embed-only fallback.
//!
//! This compiler is being phased out in favor of agent dispatch
//! (EmbeddedAgent for simple Text sources, PiAgent for everything else).
//! It retains only the embedder for pgvector indexing.

use std::collections::HashMap;

use cowiki_db::embed::Embedder;

pub struct Compiler {
    embedder: Box<dyn Embedder>,
}

impl Compiler {
    pub fn new(embedder: Box<dyn Embedder>) -> Self {
        Self { embedder }
    }

    /// Embed text for pgvector indexing. This is the only capability this
    /// legacy compiler retains — LLM compilation is handled by agents.
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        let result = self.embedder.embed(text, false).await?;
        Ok(result.vector)
    }

    /// Get embedder token usage snapshot.
    pub fn embedder_usage(&self) -> HashMap<String, cowiki_utils::token_usage::TokenUsage> {
        self.embedder.usage_snapshot()
    }
}
