use reqwest::Client;
use serde::{Deserialize, Serialize};

pub struct OpenAIClient {
    client: Client,
    api_key: String,
    base_url: String,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    temperature: f64,
}

#[derive(Serialize, Deserialize, Clone)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Serialize)]
struct EmbeddingRequest {
    model: String,
    input: String,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

impl OpenAIClient {
    pub fn new(api_key: &str, base_url: &str) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.to_string(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    pub async fn chat(&self, system: &str, user: &str) -> Result<String, String> {
        let resp = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&ChatRequest {
                model: "gpt-4o-mini".into(),
                messages: vec![
                    Message {
                        role: "system".into(),
                        content: system.into(),
                    },
                    Message {
                        role: "user".into(),
                        content: user.into(),
                    },
                ],
                temperature: 0.3,
            })
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json::<ChatResponse>()
            .await
            .map_err(|e| e.to_string())?;

        Ok(resp
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default())
    }

    pub async fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        let resp = self
            .client
            .post(format!("{}/embeddings", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&EmbeddingRequest {
                model: "text-embedding-3-small".into(),
                input: text.to_string(),
            })
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json::<EmbeddingResponse>()
            .await
            .map_err(|e| e.to_string())?;

        Ok(resp
            .data
            .first()
            .map(|d| d.embedding.clone())
            .unwrap_or_default())
    }
}
