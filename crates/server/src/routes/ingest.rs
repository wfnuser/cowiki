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
    headers: axum::http::HeaderMap,
    Json(input): Json<IngestRequest>,
) -> Result<Json<IngestResponse>> {
    // Ingest writes a source file to the named branch — members with write
    // permission only, and only onto the caller's own draft branch (previously
    // this even allowed branch == "main").
    let guard = crate::routes::guard::require_membership(&state, &headers, &ws_slug).await?;
    crate::routes::guard::require(&guard, crate::routes::guard::Permission::EditContent)?;
    super::pages::require_own_branch(&input.branch, guard.user.id)?;
    let repo = state
        .repo_manager
        .get(&ws_slug)
        .map_err(|e| AppError::Internal(format!("repo error: {e}")))?;
    super::pages::ensure_user_branch_if_needed(&repo, &input.branch)?;
    do_ingest(&repo, input).await
}

async fn do_ingest(
    repo: &cowiki_core::git::WikiRepo,
    input: IngestRequest,
) -> Result<Json<IngestResponse>> {
    let content = match input.source_type.as_str() {
        "url" => {
            let result = cowiki_extractor::extract_url(&input.content)
                .await
                .map_err(|e| AppError::Internal(format!("URL extraction failed: {e}")))?;
            format!(
                "---\ntitle: \"{}\"\nsource_url: \"{}\"\ndomain: \"{}\"\n---\n\n{}",
                result.title.replace('"', "\\\""),
                result.url,
                result.metadata.domain,
                result.text,
            )
        }
        "text" | "file" => input.content.clone(),
        _ => return Err(AppError::BadRequest("invalid source_type".into())),
    };

    let hash = format!("{:x}", Sha256::digest(content.as_bytes()));
    let filename = input.filename.unwrap_or_else(|| {
        let short_hash = &hash[..8];
        format!("source-{short_hash}.md")
    });

    super::validate_source_filename(&filename)?;

    // Ensure user branch exists before writing (needed for first ingest on a workspace)
    if input.branch != "main" {
        repo.ensure_branch_exists(&input.branch).map_err(|e| {
            AppError::Internal(format!("failed to create branch {}: {e}", input.branch))
        })?;
    }

    let path = format!("sources/{filename}");
    repo.write_file(
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
