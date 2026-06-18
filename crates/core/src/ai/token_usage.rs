use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::HashMap;

/// Per-model token usage statistics.
/// Mirrors OpenViking's models/vlm/token_usage.py TokenUsage.
#[derive(Debug, Clone, Serialize, Default)]
pub struct TokenUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    pub call_count: u64,
    pub last_updated: Option<DateTime<Utc>>,
}

impl TokenUsage {
    /// Record a new API call's token usage.
    pub fn update(&mut self, prompt_tokens: u64, completion_tokens: u64) {
        self.prompt_tokens += prompt_tokens;
        self.completion_tokens += completion_tokens;
        self.total_tokens = self.prompt_tokens + self.completion_tokens;
        self.call_count += 1;
        self.last_updated = Some(Utc::now());
    }
}

/// Tracks token usage across multiple models.
/// Mirrors OpenViking's TokenUsageTracker.
#[derive(Debug, Clone, Default)]
pub struct TokenUsageTracker {
    usage_by_model: HashMap<String, TokenUsage>,
}

impl TokenUsageTracker {
    /// Record token usage for a specific model.
    pub fn record(&mut self, model: &str, prompt_tokens: u64, completion_tokens: u64) {
        self.usage_by_model
            .entry(model.to_string())
            .or_default()
            .update(prompt_tokens, completion_tokens);
    }

    /// Get usage for a specific model.
    pub fn get(&self, model: &str) -> Option<&TokenUsage> {
        self.usage_by_model.get(model)
    }

    /// Sum all token usage across all models.
    pub fn total(&self) -> TokenUsage {
        let mut total = TokenUsage::default();
        for usage in self.usage_by_model.values() {
            total.prompt_tokens += usage.prompt_tokens;
            total.completion_tokens += usage.completion_tokens;
            total.total_tokens += usage.total_tokens;
            total.call_count += usage.call_count;
        }
        total.last_updated = self
            .usage_by_model
            .values()
            .filter_map(|u| u.last_updated)
            .max();
        total
    }

    /// All model usage entries.
    pub fn all(&self) -> &HashMap<String, TokenUsage> {
        &self.usage_by_model
    }

    /// Reset all tracked usage.
    pub fn reset(&mut self) {
        self.usage_by_model.clear();
    }
}

impl Serialize for TokenUsageTracker {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let total = self.total();
        let mut map = serializer.serialize_map(Some(self.usage_by_model.len() + 1))?;
        map.serialize_entry("total", &total)?;
        for (model, usage) in &self.usage_by_model {
            map.serialize_entry(model, usage)?;
        }
        map.end()
    }
}
