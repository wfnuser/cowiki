use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::super::token_usage::{TokenUsage, TokenUsageTracker};
use super::{EmbedResult, Embedder, EmbedderConfig};

pub struct OpenAIEmbedder {
    client: Client,
    config: EmbedderConfig,
    tracker: Arc<Mutex<TokenUsageTracker>>,
}

// ── OpenAI API types ──

#[derive(Serialize)]
struct EmbeddingRequest<'a> {
    model: &'a str,
    input: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    dimensions: Option<u32>,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
    usage: Option<EmbedUsageInfo>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

#[derive(Deserialize)]
struct EmbedUsageInfo {
    prompt_tokens: u32,
}

// ── Batch request ──

#[derive(Serialize)]
struct EmbeddingBatchRequest<'a> {
    model: &'a str,
    input: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    dimensions: Option<u32>,
}

#[derive(Deserialize)]
struct EmbeddingBatchResponse {
    data: Vec<EmbeddingData>,
    usage: Option<EmbedUsageInfo>,
}

impl OpenAIEmbedder {
    pub fn new(config: EmbedderConfig) -> Self {
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

    /// Some models (e.g. nvidia/nv-embed-v1) reject the `dimensions` field.
    /// For these, we request a full embedding and truncate client-side.
    fn skip_dimensions_param(&self) -> bool {
        self.config.model.contains("nv-embed")
    }

    /// Truncate a vector to the configured dimension by taking the first N elements.
    /// The HNSW index dimension is immutable at creation time, so truncation is the
    /// correct approach — not rebuilding the index.
    fn truncate_vector(&self, mut vector: Vec<f32>) -> Vec<f32> {
        if self.config.dimension > 0 && vector.len() > self.config.dimension as usize {
            tracing::debug!(
                "truncating embedding from {} to {} dimensions for model {}",
                vector.len(),
                self.config.dimension,
                self.config.model,
            );
            vector.truncate(self.config.dimension as usize);
        }
        vector
    }

    async fn try_embed_inner(
        &self,
        text: &str,
        dimensions: Option<u32>,
    ) -> Result<EmbedResult, String> {
        let url = format!("{}/embeddings", self.config.api_base.trim_end_matches('/'));

        let response = self
            .client
            .post(&url)
            .bearer_auth(&self.config.api_key)
            .json(&EmbeddingRequest {
                model: &self.config.model,
                input: text,
                dimensions,
            })
            .send()
            .await
            .map_err(|e| {
                tracing::error!(
                    "Embedder API request failed for model {}: {e}",
                    self.config.model
                );
                e.to_string()
            })?;

        let status = response.status();
        let resp_text = response.text().await.map_err(|e| {
            tracing::error!(
                "Embedder failed to read response body for model {}: {e}",
                self.config.model
            );
            e.to_string()
        })?;

        let resp: EmbeddingResponse = serde_json::from_str(&resp_text).map_err(|e| {
            let preview: String = resp_text.chars().take(500).collect();
            tracing::error!(
                "Embedder API response parse failed for model {} (status {status}): {e}\nResponse: {preview}",
                self.config.model,
            );
            format!("embedding parse error (status {status}): {e}")
        })?;

        if let Some(u) = &resp.usage {
            if let Ok(mut tracker) = self.tracker.lock() {
                tracker.record(&self.config.model, u.prompt_tokens as u64, 0);
            }
        }

        let vector = resp
            .data
            .first()
            .map(|d| d.embedding.clone())
            .unwrap_or_default();

        Ok(EmbedResult { vector })
    }

    async fn try_embed_batch_inner(
        &self,
        texts: &[String],
        dimensions: Option<u32>,
    ) -> Result<Vec<EmbedResult>, String> {
        let url = format!("{}/embeddings", self.config.api_base.trim_end_matches('/'));

        let response = self
            .client
            .post(&url)
            .bearer_auth(&self.config.api_key)
            .json(&EmbeddingBatchRequest {
                model: &self.config.model,
                input: texts,
                dimensions,
            })
            .send()
            .await
            .map_err(|e| {
                tracing::error!(
                    "Embedder batch API request failed for model {}: {e}",
                    self.config.model
                );
                e.to_string()
            })?;
        let status = response.status();
        let resp_text = response.text().await.map_err(|e| {
            tracing::error!(
                "Embedder batch failed to read response body for model {}: {e}",
                self.config.model
            );
            e.to_string()
        })?;

        let resp: EmbeddingBatchResponse = serde_json::from_str(&resp_text).map_err(|e| {
            let preview: String = resp_text.chars().take(500).collect();
            tracing::error!(
                "Embedder batch API response parse failed for model {} (status {status}): {e}\nResponse: {preview}",
                self.config.model,
            );
            format!("embedding batch parse error (status {status}): {e}")
        })?;

        if let Some(u) = &resp.usage {
            if let Ok(mut tracker) = self.tracker.lock() {
                tracker.record(&self.config.model, u.prompt_tokens as u64, 0);
            }
        }

        let results: Vec<_> = resp
            .data
            .into_iter()
            .map(|d| EmbedResult {
                vector: d.embedding,
            })
            .collect();

        if results.len() != texts.len() {
            return Err(format!(
                "embedder batch: expected {} embeddings, got {}",
                texts.len(),
                results.len()
            ));
        }

        // Guard against zero-length embeddings
        for (i, r) in results.iter().enumerate() {
            if r.vector.is_empty() {
                return Err(format!(
                    "embedder batch: empty embedding at index {i}"
                ));
            }
        }

        Ok(results)
    }
}

#[async_trait]
impl Embedder for OpenAIEmbedder {
    async fn embed(&self, text: &str, is_query: bool) -> Result<EmbedResult, String> {
        let _ = is_query;
        // nvidia/nv-embed-v1 rejects the `dimensions` field — skip it and truncate client-side
        let dimensions = if self.config.dimension > 0 && !self.skip_dimensions_param() {
            Some(self.config.dimension)
        } else {
            None
        };
        let mut result = self.try_embed_inner(text, dimensions).await;
        // If dimensions was sent and failed, retry without it (for non-nv-embed models)
        if let (Err(e), Some(dim)) = (&result, dimensions) {
            tracing::warn!(
                "Embedder dimension={dim} failed for model {}, retrying without dimensions: {e}",
                self.config.model
            );
            result = self.try_embed_inner(text, None).await;
        }
        // Truncate to configured dimension if needed (for models that return longer vectors)
        if let Ok(ref mut r) = result {
            r.vector = self.truncate_vector(std::mem::take(&mut r.vector));
        }
        result
    }

    async fn embed_batch(
        &self,
        texts: &[String],
        is_query: bool,
    ) -> Result<Vec<EmbedResult>, String> {
        let _ = is_query;
        let dimensions = if self.config.dimension > 0 && !self.skip_dimensions_param() {
            Some(self.config.dimension)
        } else {
            None
        };
        let mut result = self.try_embed_batch_inner(texts, dimensions).await;
        if let (Err(e), Some(dim)) = (&result, dimensions) {
            tracing::warn!(
                "Embedder batch dimension={dim} failed for model {}, retrying without dimensions: {e}",
                self.config.model
            );
            result = self.try_embed_batch_inner(texts, None).await;
        }
        if let Ok(ref mut results) = result {
            for r in results.iter_mut() {
                r.vector = self.truncate_vector(std::mem::take(&mut r.vector));
            }
        }
        result
    }

    fn usage_snapshot(&self) -> HashMap<String, TokenUsage> {
        self.tracker
            .lock()
            .map(|t| t.all().clone())
            .unwrap_or_default()
    }
}
