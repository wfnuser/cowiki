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
    // Diff the frozen `pr/{id}` snapshot against main (whole branch). Once merged/rejected
    // the snapshot refs are cleaned up, so fall back to an empty diff for historical rows.
    let diffs = repo
        .diff_ref_against_main(&format!("pr/{}", submission.id))
        .unwrap_or_default();

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

    use cowiki_db::submissions::SubmissionStatus;
    match input.action.as_str() {
        "approve" => {
            cowiki_db::submissions::update_status(
                &state.db,
                id,
                SubmissionStatus::Approved,
                guard.user.id,
            )
            .await?;
        }
        "merge" => {
            if submission.status != SubmissionStatus::Approved.as_str() {
                return Err(AppError::BadRequest(
                    "submission must be approved before merge".into(),
                ));
            }

            let repo = state
                .repo_manager
                .get(&ws_slug)
                .map_err(|e| AppError::Internal(format!("repo error: {e}")))?;

            let pr_ref = format!("pr/{}", submission.id);
            // Pages changed by this submission (for search indexing after the merge).
            let changed: Vec<String> = repo
                .diff_ref_against_main(&pr_ref)
                .unwrap_or_default()
                .into_iter()
                .map(|d| d.path)
                .collect();

            // Merge is authored by the original submitter, not the reviewer.
            let author = cowiki_db::users::find_by_id(&state.db, submission.user_id)
                .await
                .ok()
                .flatten()
                .map(|u| u.name)
                .unwrap_or_else(|| guard.user.name.clone());

            // Linear squash + rebase + fast-forward. The snapshot is frozen, so a
            // conflict with the now-advanced main can't be fixed in place — close this
            // submission (refs cleaned up) and tell the author to sync & resubmit; the
            // resubmit takes a fresh snapshot off the rebased branch.
            match repo
                .merge_pr(
                    &submission.id.to_string(),
                    &author,
                    &format!("approve: {}", submission.summary),
                )
                .map_err(|e| AppError::Internal(e.to_string()))?
            {
                cowiki_core::git::MergeOutcome::Merged => {}
                cowiki_core::git::MergeOutcome::Conflict(paths) => {
                    repo.cleanup_submission(&submission.id.to_string());
                    cowiki_db::submissions::update_status(
                        &state.db,
                        id,
                        SubmissionStatus::Rejected,
                        guard.user.id,
                    )
                    .await?;
                    return Err(AppError::Conflict(format!(
                        "main has moved and this snapshot now conflicts ({}); submission closed — sync your branch and submit again",
                        paths.join(", ")
                    )));
                }
            }

            // Index changed pages from main (best-effort; search can catch up separately).
            for path in &changed {
                let slug = path
                    .strip_prefix("wiki/")
                    .and_then(|s| s.strip_suffix(".md"))
                    .unwrap_or(path);
                if let Ok(Some(content)) = repo.read_file("main", path) {
                    let text = String::from_utf8_lossy(&content);
                    if let Ok((title, summary)) = super::pages::require_page_title(&text) {
                        let hash = format!("{:x}", Sha256::digest(text.as_bytes()));
                        if let Err(e) = cowiki_db::pages::upsert(
                            &state.db,
                            slug,
                            &title,
                            &summary,
                            "main",
                            &hash,
                            None,
                            guard.user.id,
                            &ws_slug,
                        )
                        .await
                        {
                            tracing::warn!("failed to index merged page '{slug}': {e}");
                        }
                    }
                }
            }

            // Drop the snapshot refs and catch the author's branch up to the new main.
            repo.cleanup_submission(&submission.id.to_string());
            let _ = repo.rebase_onto_main(&submission.source_branch);

            cowiki_db::submissions::update_status(
                &state.db,
                id,
                SubmissionStatus::Merged,
                guard.user.id,
            )
            .await?;
        }
        "reject" => {
            // Drop the snapshot refs; the author keeps their changes on `source_branch`.
            if let Ok(repo) = state.repo_manager.get(&ws_slug) {
                repo.cleanup_submission(&submission.id.to_string());
            }
            cowiki_db::submissions::update_status(
                &state.db,
                id,
                SubmissionStatus::Rejected,
                guard.user.id,
            )
            .await?;
        }
        _ => return Err(AppError::BadRequest("invalid action".into())),
    }

    Ok(Json(serde_json::json!({"ok": true})))
}

#[derive(Deserialize)]
pub struct ReviewEdit {
    /// Repo path, e.g. "wiki/foo.md".
    pub path: String,
    pub content: String,
}

/// Apply a review-requested change to a submission's `pr/{id}` snapshot (a single amended
/// commit `R` on top of the frozen base `S`). Edits happen in the review screen against the
/// snapshot branch, never the author's live `user/{id}` branch.
pub async fn edit_review(
    State(state): State<Arc<AppState>>,
    Path((ws_slug, id)): Path<(String, uuid::Uuid)>,
    headers: axum::http::HeaderMap,
    Json(input): Json<ReviewEdit>,
) -> Result<Json<serde_json::Value>> {
    let submission = cowiki_db::submissions::find_by_id(&state.db, id)
        .await?
        .ok_or_else(|| AppError::NotFound("submission not found".into()))?;
    if submission.workspace_slug != ws_slug {
        return Err(AppError::NotFound(
            "submission not found in this workspace".into(),
        ));
    }
    let guard = guard::require_membership(&state, &headers, &ws_slug).await?;
    guard::require(&guard, Permission::EditContent)?;

    use cowiki_db::submissions::SubmissionStatus;
    if submission.status == SubmissionStatus::Merged.as_str()
        || submission.status == SubmissionStatus::Rejected.as_str()
    {
        return Err(AppError::BadRequest(
            "submission is closed; cannot edit".into(),
        ));
    }

    super::pages::require_page_title(&input.content)?;

    let repo = state
        .repo_manager
        .get(&ws_slug)
        .map_err(|e| AppError::Internal(format!("repo error: {e}")))?;
    repo.write_review_fix(
        &submission.id.to_string(),
        &input.path,
        input.content.as_bytes(),
        "review fix",
        &guard.user.name,
    )
    .map_err(|e| AppError::Internal(e.to_string()))?;

    // A post-approval edit must be re-reviewed: reset to pending so it can't merge unseen.
    if submission.status == SubmissionStatus::Approved.as_str() {
        cowiki_db::submissions::update_status(
            &state.db,
            id,
            SubmissionStatus::Pending,
            guard.user.id,
        )
        .await?;
    }

    Ok(Json(serde_json::json!({"ok": true})))
}
