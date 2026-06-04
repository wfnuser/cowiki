use axum::extract::{Path, State};
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

/// Workspace-scoped ingest
pub async fn ingest_ws(
    State(state): State<Arc<AppState>>,
    Path(ws_slug): Path<String>,
    Json(input): Json<IngestRequest>,
) -> Result<Json<IngestResponse>> {
    let repo = state.repo_manager.get(&ws_slug)
        .map_err(|e| AppError::Internal(format!("repo error: {e}")))?;
    super::pages::ensure_user_branch_if_needed(&repo, &input.branch)?;
    do_ingest(&repo, input).await
}

async fn do_ingest(repo: &cowiki_core::git::WikiRepo, input: IngestRequest) -> Result<Json<IngestResponse>> {
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

    // Validate filename to prevent path traversal
    if filename.is_empty()
        || filename.contains("..")
        || filename.contains('/')
        || filename.contains('\\')
        || filename.starts_with('.')
        || filename.len() > 255
    {
        return Err(AppError::BadRequest("invalid filename".into()));
    }

    // Ensure user branch exists before writing (needed for first ingest on a workspace)
    if input.branch != "main" {
        repo.ensure_branch_exists(&input.branch)
            .map_err(|e| AppError::Internal(format!("failed to create branch {}: {e}", input.branch)))?;
    }

    let path = format!("sources/{filename}");
    repo.write_file(
        &input.branch, &path, content.as_bytes(),
        &format!("ingest: {filename}"), &input.branch,
    ).map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(IngestResponse { filename, content_hash: hash }))
}

async fn fetch_url(url: &str) -> Result<String> {
    reqwest::get(url)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .text()
        .await
        .map_err(|e| AppError::Internal(e.to_string()))
}
