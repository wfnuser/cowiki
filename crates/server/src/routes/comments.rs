use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use std::sync::Arc;

use crate::error::{AppError, Result};
use crate::routes::guard::{self, Permission, WorkspaceGuard};
use crate::AppState;

#[derive(Deserialize)]
pub struct CreateCommentRequest {
    pub file_path: String,
    pub line_number: Option<i32>,
    pub body: String,
    pub parent_id: Option<uuid::Uuid>,
}

/// Membership gate shared by every comment route (#54): the caller must be a member
/// of `ws_slug` with at least `perm`, and the submission must belong to that
/// workspace — otherwise any authenticated user could read or mutate review threads
/// in workspaces they have no access to just by guessing ids.
async fn guard_submission(
    state: &Arc<AppState>,
    headers: &axum::http::HeaderMap,
    ws_slug: &str,
    submission_id: uuid::Uuid,
    perm: Permission,
) -> Result<WorkspaceGuard> {
    let guard = guard::require_membership(state, headers, ws_slug).await?;
    guard::require(&guard, perm)?;
    let submission = cowiki_db::submissions::find_by_id(&state.db, submission_id)
        .await?
        .ok_or_else(|| AppError::NotFound("submission not found".into()))?;
    if submission.workspace_slug != ws_slug {
        return Err(AppError::NotFound(
            "submission not found in this workspace".into(),
        ));
    }
    Ok(guard)
}

/// Load a comment and check it actually hangs off the submission in the path
/// (prevents cross-submission/workspace comment ids smuggled into the URL).
async fn load_bound_comment(
    state: &Arc<AppState>,
    submission_id: uuid::Uuid,
    comment_id: uuid::Uuid,
) -> Result<cowiki_db::review_comments::ReviewComment> {
    let comment = cowiki_db::review_comments::find_by_id(&state.db, comment_id)
        .await?
        .ok_or_else(|| AppError::NotFound("comment not found".into()))?;
    if comment.submission_id != submission_id {
        return Err(AppError::NotFound(
            "comment not found on this submission".into(),
        ));
    }
    Ok(comment)
}

/// List all comments for a submission — any member can read.
pub async fn list_comments(
    State(state): State<Arc<AppState>>,
    Path((ws_slug, submission_id)): Path<(String, uuid::Uuid)>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Vec<cowiki_db::review_comments::ReviewComment>>> {
    guard_submission(
        &state,
        &headers,
        &ws_slug,
        submission_id,
        Permission::ViewContent,
    )
    .await?;
    let comments =
        cowiki_db::review_comments::list_for_submission(&state.db, submission_id).await?;
    Ok(Json(comments))
}

/// Create a comment — any member can join the discussion (viewers included).
pub async fn create_comment(
    State(state): State<Arc<AppState>>,
    Path((ws_slug, submission_id)): Path<(String, uuid::Uuid)>,
    headers: axum::http::HeaderMap,
    Json(input): Json<CreateCommentRequest>,
) -> Result<Json<cowiki_db::review_comments::ReviewComment>> {
    let guard = guard_submission(
        &state,
        &headers,
        &ws_slug,
        submission_id,
        Permission::ViewContent,
    )
    .await?;
    if let Some(parent_id) = input.parent_id {
        // Replies must target a thread on this same submission.
        load_bound_comment(&state, submission_id, parent_id).await?;
    }
    let comment = cowiki_db::review_comments::create(
        &state.db,
        submission_id,
        guard.user.id,
        &input.file_path,
        input.line_number,
        &input.body,
        input.parent_id,
    )
    .await?;
    Ok(Json(comment))
}

/// Resolve a comment thread — reviewers/editors only.
pub async fn resolve_comment(
    State(state): State<Arc<AppState>>,
    Path((ws_slug, submission_id, comment_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    headers: axum::http::HeaderMap,
) -> Result<Json<cowiki_db::review_comments::ReviewComment>> {
    guard_submission(
        &state,
        &headers,
        &ws_slug,
        submission_id,
        Permission::EditContent,
    )
    .await?;
    load_bound_comment(&state, submission_id, comment_id).await?;
    let comment = cowiki_db::review_comments::resolve(&state.db, comment_id).await?;
    Ok(Json(comment))
}

/// Unresolve a comment thread — reviewers/editors only.
pub async fn unresolve_comment(
    State(state): State<Arc<AppState>>,
    Path((ws_slug, submission_id, comment_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    headers: axum::http::HeaderMap,
) -> Result<Json<cowiki_db::review_comments::ReviewComment>> {
    guard_submission(
        &state,
        &headers,
        &ws_slug,
        submission_id,
        Permission::EditContent,
    )
    .await?;
    load_bound_comment(&state, submission_id, comment_id).await?;
    let comment = cowiki_db::review_comments::unresolve(&state.db, comment_id).await?;
    Ok(Json(comment))
}

/// Delete a comment — its author, or a member with EditContent (moderation).
pub async fn delete_comment(
    State(state): State<Arc<AppState>>,
    Path((ws_slug, submission_id, comment_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>> {
    let guard = guard_submission(
        &state,
        &headers,
        &ws_slug,
        submission_id,
        Permission::ViewContent,
    )
    .await?;
    let comment = load_bound_comment(&state, submission_id, comment_id).await?;
    if comment.user_id != guard.user.id {
        guard::require(&guard, Permission::EditContent).map_err(|_| {
            AppError::Forbidden("only the comment author or an editor can delete it".into())
        })?;
    }
    cowiki_db::review_comments::delete(&state.db, comment_id).await?;
    Ok(Json(serde_json::json!({"ok": true})))
}
