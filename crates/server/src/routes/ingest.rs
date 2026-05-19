use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;

use crate::error::{AppError, Result};
use crate::AppState;

#[derive(Deserialize)]
pub struct IngestRequest {
    pub source_type: String,
    pub content: String,
    pub filename: Option<String>,
    pub branch: String,
}

#[derive(Serialize)]
pub struct IngestResponse {
    pub filename: String,
    pub content_hash: String,
}

pub async fn ingest(
    State(state): State<Arc<AppState>>,
    Json(input): Json<IngestRequest>,
) -> Result<Json<IngestResponse>> {
    let content = match input.source_type.as_str() {
        "url" => fetch_url(&input.content).await?,
        "text" | "file" => input.content.clone(),
        _ => return Err(AppError::BadRequest("invalid source_type".into())),
    };

    let hash = format!("{:x}", Sha256::digest(content.as_bytes()));
    let filename = input.filename.unwrap_or_else(|| {
        let short_hash = &hash[..8];
        format!("source-{short_hash}.md")
    });

    let path = format!("sources/{filename}");
    state
        .wiki_repo
        .write_file(
            &input.branch,
            &path,
            content.as_bytes(),
            &format!("ingest: {filename}"),
            &input.branch,
        )
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(IngestResponse {
        filename,
        content_hash: hash,
    }))
}

async fn fetch_url(url: &str) -> Result<String> {
    reqwest::get(url)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .text()
        .await
        .map_err(|e| AppError::Internal(e.to_string()))
}
