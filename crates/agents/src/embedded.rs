use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use reqwest::Client;
use serde::{Deserialize, Serialize};

use cowiki_utils::token_usage::{TokenUsage, TokenUsageTracker};

// ── Configuration ─────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct EmbeddedAgentConfig {
    pub provider: String,
    pub model: String,
    pub api_key: String,
    pub api_base: String,
    pub temperature: f64,
    pub max_tokens: Option<u32>,
    pub max_rounds: u32,
    pub token_budget: u32,
}

impl Default for EmbeddedAgentConfig {
    fn default() -> Self {
        Self {
            provider: "openai".into(),
            model: "gpt-4o-mini".into(),
            api_key: String::new(),
            api_base: "https://api.openai.com/v1".into(),
            temperature: 0.3,
            max_tokens: Some(8192),
            max_rounds: 5,
            token_budget: 50_000,
        }
    }
}

// ── Messages ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,          // "system" | "user" | "assistant" | "tool"
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

// ── Agent Response ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct EmbeddedAgentResponse {
    pub content: String,
    pub rounds: u32,
    pub usage: Option<TokenUsage>,
    pub finish_reason: Option<String>,
}

// ── Embedded Agent ─────────────────────────────────────────────

/// Simple multi-turn agent with tool-calling loop.
///
/// Stateless — no session persistence, no compaction.
/// Suitable for short Text sources (< 4KB).
/// Directly calls Anthropic/OpenAI APIs in-process.
pub struct EmbeddedAgent {
    config: EmbeddedAgentConfig,
    client: Client,
    tracker: Arc<Mutex<TokenUsageTracker>>,
}

// ── OpenAI API types ──

#[derive(Serialize)]
struct OpenAIChatRequest<'a> {
    model: &'a str,
    messages: &'a [OpenAIMessage],
    temperature: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<&'a [OpenAIToolDef]>,
}

#[derive(Serialize, Clone)]
struct OpenAIMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAIToolCall>>,
}

#[derive(Serialize, Clone)]
struct OpenAIToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: OpenAIFunctionCall,
}

#[derive(Serialize, Clone)]
struct OpenAIFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Serialize, Clone)]
struct OpenAIToolDef {
    #[serde(rename = "type")]
    tool_type: String,
    function: OpenAIFunctionSchema,
}

#[derive(Serialize, Clone)]
struct OpenAIFunctionSchema {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Deserialize)]
struct OpenAIChatResponse {
    choices: Vec<OpenAIChoice>,
    usage: Option<OpenAIUsage>,
}

#[derive(Deserialize)]
struct OpenAIChoice {
    message: OpenAIAssistantMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct OpenAIAssistantMessage {
    content: Option<String>,
    tool_calls: Option<Vec<OpenAIToolCallResponse>>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct OpenAIToolCallResponse {
    id: String,
    function: OpenAIFunctionResponse,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct OpenAIFunctionResponse {
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct OpenAIUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

impl EmbeddedAgent {
    pub fn new(config: EmbeddedAgentConfig) -> Self {
        Self {
            config,
            client: Client::new(),
            tracker: Arc::new(Mutex::new(TokenUsageTracker::default())),
        }
    }

    pub fn tracker_arc(&self) -> Arc<Mutex<TokenUsageTracker>> {
        Arc::clone(&self.tracker)
    }

    /// Simple chat — no tools, single-response.
    /// Used for quick inline tasks where the dispatcher routes to embedded.
    pub async fn chat(
        &self,
        messages: &[ChatMessage],
    ) -> Result<EmbeddedAgentResponse, String> {
        self.chat_with_tools(messages, &[]).await
    }

    /// Multi-turn chat with optional tools.
    /// The caller (dispatcher) handles tool execution externally and
    /// feeds results back by appending tool-result messages to the
    /// conversation and calling again.
    pub async fn chat_with_tools(
        &self,
        messages: &[ChatMessage],
        tools: &[crate::protocol::ToolDef],
    ) -> Result<EmbeddedAgentResponse, String> {
        let url = format!(
            "{}/chat/completions",
            self.config.api_base.trim_end_matches('/')
        );

        let openai_messages: Vec<OpenAIMessage> = messages
            .iter()
            .map(|m| OpenAIMessage {
                role: m.role.clone(),
                content: m.content.clone(),
                tool_call_id: m.tool_call_id.clone(),
                tool_calls: None,
            })
            .collect();

        let openai_tools: Vec<OpenAIToolDef> = if tools.is_empty() {
            Vec::new()
        } else {
            tools
                .iter()
                .map(|t| OpenAIToolDef {
                    tool_type: "function".into(),
                    function: OpenAIFunctionSchema {
                        name: t.name.clone(),
                        description: t.description.clone(),
                        parameters: t.parameters.clone(),
                    },
                })
                .collect()
        };

        let request = OpenAIChatRequest {
            model: &self.config.model,
            messages: &openai_messages,
            temperature: self.config.temperature,
            max_tokens: self.config.max_tokens,
            tools: if openai_tools.is_empty() {
                None
            } else {
                Some(&openai_tools)
            },
        };

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.config.api_key)
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                tracing::error!("EmbeddedAgent API request failed: {e}");
                e.to_string()
            })?
            .json::<OpenAIChatResponse>()
            .await
            .map_err(|e| {
                tracing::error!("EmbeddedAgent parse failed: {e}");
                e.to_string()
            })?;

        let usage = resp.usage.map(|u| {
            if let Ok(mut tracker) = self.tracker.lock() {
                tracker.record(&self.config.model, u.prompt_tokens, u.completion_tokens);
            }
            let mut tu = TokenUsage::default();
            tu.update(u.prompt_tokens, u.completion_tokens);
            tu
        });

        let choice = resp.choices.into_iter().next().unwrap_or(OpenAIChoice {
            message: OpenAIAssistantMessage {
                content: None,
                tool_calls: None,
            },
            finish_reason: None,
        });

        let content = choice.message.content.unwrap_or_default();

        Ok(EmbeddedAgentResponse {
            content,
            rounds: 1,
            usage,
            finish_reason: choice.finish_reason,
        })
    }

    pub fn usage_snapshot(&self) -> HashMap<String, TokenUsage> {
        self.tracker
            .lock()
            .map(|t| t.all().clone())
            .unwrap_or_default()
    }

}

// ── Factory ───────────────────────────────────────────────────

/// Create an embedded agent from config.
pub fn create_embedded_agent(config: EmbeddedAgentConfig) -> EmbeddedAgent {
    EmbeddedAgent::new(config)
}
