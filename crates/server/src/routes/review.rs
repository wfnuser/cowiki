use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;

use crate::error::{AppError, Result};
use crate::routes::auth::extract_user;
use crate::routes::guard::{self, Permission};
use crate::AppState;

pub async fn list_reviews(
    State(state): State<Arc<AppState>>,
    Path(ws_slug): Path<String>,
) -> Result<Json<Vec<cowiki_db::submissions::Submission>>> {
    let subs = cowiki_db::submissions::list_pending_for_workspace(&state.db, &ws_slug).await?;
    Ok(Json(subs))
}

#[derive(Serialize)]
pub struct ReviewDetail {
    pub submission: cowiki_db::submissions::Submission,
    pub diffs: Vec<cowiki_core::git::FileDiff>,
}

pub async fn get_review(
    State(state): State<Arc<AppState>>,
    Path((ws_slug, id)): Path<(String, uuid::Uuid)>,
) -> Result<Json<ReviewDetail>> {
    let submission = cowiki_db::submissions::find_by_id(&state.db, id)
        .await?
        .ok_or_else(|| AppError::NotFound("submission not found".into()))?;
    if submission.workspace_slug != ws_slug {
        return Err(AppError::NotFound("submission not found in this workspace".into()));
    }

    let repo = state.repo_manager.get(&ws_slug)
        .map_err(|e| AppError::Internal(format!("repo error: {e}")))?;
    let diffs = repo
        .diff_files(&submission.source_branch, &submission.page_slugs)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(ReviewDetail { submission, diffs }))
}

#[derive(Deserialize)]
pub struct ReviewAction {
    pub action: String,
}

pub async fn review_action(
    State(state): State<Arc<AppState>>,
    Path((ws_slug, id)): Path<(String, uuid::Uuid)>,
    headers: axum::http::HeaderMap,
    Json(input): Json<ReviewAction>,
) -> Result<Json<serde_json::Value>> {
    let submission = cowiki_db::submissions::find_by_id(&state.db, id)
        .await?
        .ok_or_else(|| AppError::NotFound("submission not found".into()))?;
    if submission.workspace_slug != ws_slug {
        return Err(AppError::NotFound("submission not found in this workspace".into()));
    }

    // Authorization: reviewer must have EditContent permission
    let guard = guard::require_membership(&state, &headers, &ws_slug).await?;
    guard::require(&guard, Permission::EditContent)?;

    let repo = state.repo_manager.get(&ws_slug)
        .map_err(|e| AppError::Internal(format!("repo error: {e}")))?;

    match input.action.as_str() {
        "approve" => {
            let file_paths: Vec<String> = submission
                .page_slugs
                .iter()
                .map(|s| format!("wiki/{s}.md"))
                .collect();

            repo
                .merge_to_main(
                    &submission.source_branch,
                    &file_paths,
                    &guard.user.name,
                    &format!("approve: {}", submission.summary),
                )
                .map_err(|e| AppError::Internal(e.to_string()))?;

            // Update page records to main branch
            for slug in &submission.page_slugs {
                let path = format!("wiki/{slug}.md");
                if let Some(content) = repo
                    .read_file("main", &path)
                    .map_err(|e| AppError::Internal(e.to_string()))?
                {
                    let text = String::from_utf8_lossy(&content);
                    let hash = format!("{:x}", Sha256::digest(text.as_bytes()));
                    if let Ok(emb) = state.compiler.embed(&text).await {
                        cowiki_db::pages::upsert(
                            &state.db,
                            slug,
                            slug,
                            "",
                            "main",
                            &hash,
                            Some(&emb),
                            guard.user.id,
                        )
                        .await
                        .ok();
                    }
                }
            }

            cowiki_db::submissions::update_status(&state.db, id, "approved", guard.user.id).await?;
        }
        "reject" => {
            cowiki_db::submissions::update_status(&state.db, id, "rejected", guard.user.id).await?;
        }
        _ => return Err(AppError::BadRequest("invalid action".into())),
    }

    Ok(Json(serde_json::json!({"ok": true})))
}
