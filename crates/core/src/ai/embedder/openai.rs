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
#[allow(dead_code)]
struct EmbedUsageInfo {
    prompt_tokens: u32,
    total_tokens: u32,
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
                tracker.record(&self.config.model, u.prompt_tokens, 0);
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
                tracker.record(&self.config.model, u.prompt_tokens, 0);
            }
        }

        let results: Vec<_> = resp
            .data
            .into_iter()
            .map(|d| EmbedResult {
                vector: d.embedding,
            })
            .collect();

        Ok(results)
    }
}

#[async_trait]
impl Embedder for OpenAIEmbedder {
    async fn embed(&self, text: &str, is_query: bool) -> Result<EmbedResult, String> {
        let _ = is_query;
        let dimensions = if self.config.dimension > 0 {
            Some(self.config.dimension)
        } else {
            None
        };
        let result = self.try_embed_inner(text, dimensions).await;
        match (result, dimensions) {
            (Err(e), Some(dim)) => {
                tracing::warn!(
                    "Embedder dimension={dim} failed for model {}, retrying without dimensions: {e}",
                    self.config.model
                );
                self.try_embed_inner(text, None).await
            }
            (r, _) => r,
        }
    }

    async fn embed_batch(
        &self,
        texts: &[String],
        is_query: bool,
    ) -> Result<Vec<EmbedResult>, String> {
        let _ = is_query;
        let dimensions = if self.config.dimension > 0 {
            Some(self.config.dimension)
        } else {
            None
        };
        let result = self.try_embed_batch_inner(texts, dimensions).await;
        match (result, dimensions) {
            (Err(e), Some(dim)) => {
                tracing::warn!(
                    "Embedder batch dimension={dim} failed for model {}, retrying without dimensions: {e}",
                    self.config.model
                );
                self.try_embed_batch_inner(texts, None).await
            }
            (r, _) => r,
        }
    }

    fn usage_snapshot(&self) -> HashMap<String, TokenUsage> {
        self.tracker
            .lock()
            .map(|t| t.all().clone())
            .unwrap_or_default()
    }
}
