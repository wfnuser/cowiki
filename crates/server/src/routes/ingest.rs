use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;

use crate::error::{AppError, Result};
use crate::AppState;

#[derive(Deserialize)]
pub struct IngestRequest {
    pub source_type: String,
    pub content: String,
    /// Optional encoding: "base64" for binary files, omit for plain text/URL
    pub encoding: Option<String>,
    pub filename: Option<String>,
    pub branch: String,
}

#[derive(Serialize)]
pub struct IngestResponse {
    pub filename: String,
    pub content_hash: String,
    /// Whether extraction succeeded (false = original saved but extraction failed)
    pub extracted: Option<bool>,
    /// Error message if extraction failed
    pub extract_error: Option<String>,
}

/// Legacy ingest (uses default repo)
pub async fn ingest(
    State(state): State<Arc<AppState>>,
    Json(input): Json<IngestRequest>,
) -> Result<Json<IngestResponse>> {
    super::pages::ensure_user_branch_if_needed(&state.wiki_repo, &input.branch)?;
    do_ingest(&state, &state.wiki_repo, input).await
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
    do_ingest(&state, &repo, input).await
}

async fn do_ingest(
    state: &AppState,
    repo: &cowiki_core::git::WikiRepo,
    input: IngestRequest,
) -> Result<Json<IngestResponse>> {
    // Parse source type
    let source_type = cowiki_extractor::SourceType::parse(&input.source_type);

    // Determine fallback filename for hash-based naming
    let hash = format!("{:x}", Sha256::digest(input.content.as_bytes()));
    let short_hash = &hash[..8];

    // Build extractor config (user tokens from DB will be populated here in future)
    let config = HashMap::new();

    let extract_input = cowiki_extractor::ExtractInput {
        source_type: source_type.unwrap_or(cowiki_extractor::SourceType::Auto),
        content: input.content.clone(),
        encoding: input.encoding.clone(),
        filename: input.filename.clone(),
        config,
    };

    // Try extraction via ExtractorRegistry
    let extraction_result = state.extractor_registry.extract(extract_input).await;

    match extraction_result {
        Ok(extract_result) => {
            let extracted_text = &extract_result.text;
            let extracted_filename = &extract_result.suggested_filename;

            // Save original file to sources/
            let orig_filename = input.filename.clone().unwrap_or_else(|| {
                format!("source-{}.bin", short_hash)
            });
            let orig_path = format!("sources/{}", sanitize_filename(&orig_filename));

            repo.write_file(
                &input.branch, &orig_path,
                &extract_result.original_content,
                &format!("ingest original: {}", orig_filename),
                &input.branch,
            ).map_err(|e| AppError::Internal(e.to_string()))?;

            // Save extracted Markdown to sources/
            let md_path = format!("sources/{}", sanitize_filename(extracted_filename));
            repo.write_file(
                &input.branch, &md_path,
                extracted_text.as_bytes(),
                &format!("ingest extracted: {}", extracted_filename),
                &input.branch,
            ).map_err(|e| AppError::Internal(e.to_string()))?;

            let content_hash = format!("{:x}", Sha256::digest(extracted_text.as_bytes()));

            Ok(Json(IngestResponse {
                filename: extracted_filename.clone(),
                content_hash,
                extracted: Some(true),
                extract_error: None,
            }))
        }
        Err(extract_err) => {
            // Extraction failed — save original file, return error
            let orig_filename = input.filename.clone().unwrap_or_else(|| {
                format!("source-{}.bin", short_hash)
            });
            let orig_path = format!("sources/{}", sanitize_filename(&orig_filename));

            // For text content (no encoding), save as-is
            let original_bytes = if input.encoding.as_deref() == Some("base64") {
                base64::Engine::decode(
                    &base64::engine::general_purpose::STANDARD,
                    &input.content,
                ).unwrap_or_else(|_| input.content.as_bytes().to_vec())
            } else {
                input.content.as_bytes().to_vec()
            };

            repo.write_file(
                &input.branch, &orig_path,
                &original_bytes,
                &format!("ingest (extraction failed): {}", orig_filename),
                &input.branch,
            ).map_err(|e| AppError::Internal(e.to_string()))?;

            let content_hash = format!("{:x}", Sha256::digest(&original_bytes));

            Ok(Json(IngestResponse {
                filename: orig_filename.clone(),
                content_hash,
                extracted: Some(false),
                extract_error: Some(extract_err.to_string()),
            }))
        }
    }
}

/// Sanitize a filename to prevent path traversal.
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric() || *c == '.' || *c == '-' || *c == '_')
        .collect::<String>()
        .trim_start_matches('.')
        .to_string()
}
