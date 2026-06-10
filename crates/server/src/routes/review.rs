use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;

use crate::error::{AppError, Result};
use crate::routes::guard::{self, Permission};
use crate::AppState;

pub async fn list_reviews(
    State(state): State<Arc<AppState>>,
    Path(ws_slug): Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Vec<cowiki_db::submissions::Submission>>> {
    let guard = guard::require_membership(&state, &headers, &ws_slug).await?;
    guard::require(&guard, Permission::ViewContent)?;
    let subs = cowiki_db::submissions::list_pending_for_workspace(&state.db, &guard.workspace.slug)
        .await?;
    Ok(Json(subs))
}

#[derive(Serialize)]
pub struct ReviewDetail {
    pub submission: cowiki_db::submissions::Submission,
    pub diffs: Vec<cowiki_core::git::FileDiff>,
    pub comments: Vec<cowiki_db::review_comments::ReviewComment>,
}

pub async fn get_review(
    State(state): State<Arc<AppState>>,
    Path((ws_slug, id)): Path<(String, uuid::Uuid)>,
    headers: axum::http::HeaderMap,
) -> Result<Json<ReviewDetail>> {
    let guard = guard::require_membership(&state, &headers, &ws_slug).await?;
    guard::require(&guard, Permission::ViewContent)?;

    let submission = cowiki_db::submissions::find_by_id(&state.db, id)
        .await?
        .ok_or_else(|| AppError::NotFound("submission not found".into()))?;
    if submission.workspace_slug != ws_slug {
        return Err(AppError::NotFound(
            "submission not found in this workspace".into(),
        ));
    }

    let repo = state
        .repo_manager
        .get(&ws_slug)
        .map_err(|e| AppError::Internal(format!("repo error: {e}")))?;
    let diffs = repo
        .diff_files(&submission.source_branch, &submission.page_slugs)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let comments = cowiki_db::review_comments::list_for_submission(&state.db, id).await?;

    Ok(Json(ReviewDetail {
        submission,
        diffs,
        comments,
    }))
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
        return Err(AppError::NotFound(
            "submission not found in this workspace".into(),
        ));
    }

    // Authorization: reviewer must have EditContent permission
    let guard = guard::require_membership(&state, &headers, &ws_slug).await?;
    guard::require(&guard, Permission::EditContent)?;

    match input.action.as_str() {
        "approve" => {
            cowiki_db::submissions::update_status(&state.db, id, "approved", guard.user.id).await?;
        }
        "merge" => {
            if submission.status != "approved" {
                return Err(AppError::BadRequest(
                    "submission must be approved before merge".into(),
                ));
            }

            let repo = state
                .repo_manager
                .get(&ws_slug)
                .map_err(|e| AppError::Internal(format!("repo error: {e}")))?;

            // Bring the branch up to date with main first. If it conflicts, the
            // author must resolve before this can merge.
            if let cowiki_core::git::RebaseOutcome::Conflict(paths) = repo
                .rebase_onto_main(&submission.source_branch)
                .map_err(|e| AppError::Internal(e.to_string()))?
            {
                return Err(AppError::Conflict(format!(
                    "branch conflicts with main; author must resolve: {}",
                    paths.join(", ")
                )));
            }

            let file_paths: Vec<String> = submission
                .page_slugs
                .iter()
                .map(|s| format!("wiki/{s}.md"))
                .collect();

            // Merge is authored by the original submitter, not the reviewer.
            let author = cowiki_db::users::find_by_id(&state.db, submission.user_id)
                .await
                .ok()
                .flatten()
                .map(|u| u.name)
                .unwrap_or_else(|| guard.user.name.clone());

            match repo
                .merge_to_main(
                    &submission.source_branch,
                    &file_paths,
                    &author,
                    &format!("approve: {}", submission.summary),
                )
                .map_err(|e| AppError::Internal(e.to_string()))?
            {
                cowiki_core::git::MergeOutcome::Merged => {}
                cowiki_core::git::MergeOutcome::Conflict(paths) => {
                    return Err(AppError::Conflict(format!(
                        "merge conflict on: {}; author must resolve against main",
                        paths.join(", ")
                    )));
                }
            }

            // Merge should not block on embedding; search indexing can catch up separately.
            for slug in &submission.page_slugs {
                let path = format!("wiki/{slug}.md");
                if let Some(content) = repo
                    .read_file("main", &path)
                    .map_err(|e| AppError::Internal(e.to_string()))?
                {
                    let text = String::from_utf8_lossy(&content);
                    let hash = format!("{:x}", Sha256::digest(text.as_bytes()));
                    cowiki_db::pages::upsert(
                        &state.db,
                        slug,
                        slug,
                        "",
                        "main",
                        &hash,
                        None,
                        guard.user.id,
                    )
                    .await
                    .ok();
                }
            }

            cowiki_db::submissions::update_status(&state.db, id, "merged", guard.user.id).await?;
        }
        "reject" => {
            cowiki_db::submissions::update_status(&state.db, id, "rejected", guard.user.id).await?;
        }
        _ => return Err(AppError::BadRequest("invalid action".into())),
    }

    Ok(Json(serde_json::json!({"ok": true})))
}
