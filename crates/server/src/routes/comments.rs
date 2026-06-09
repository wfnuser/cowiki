use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use std::sync::Arc;

use crate::error::Result;
use crate::routes::auth::extract_user;
use crate::AppState;

#[derive(Deserialize)]
pub struct CreateCommentRequest {
    pub file_path: String,
    pub line_number: Option<i32>,
    pub body: String,
    pub parent_id: Option<uuid::Uuid>,
}

/// List all comments for a submission
pub async fn list_comments(
    State(state): State<Arc<AppState>>,
    Path((_ws_slug, submission_id)): Path<(String, uuid::Uuid)>,
) -> Result<Json<Vec<cowiki_db::review_comments::ReviewComment>>> {
    let comments =
        cowiki_db::review_comments::list_for_submission(&state.db, submission_id).await?;
    Ok(Json(comments))
}

/// Create a comment on a submission
pub async fn create_comment(
    State(state): State<Arc<AppState>>,
    Path((_ws_slug, submission_id)): Path<(String, uuid::Uuid)>,
    headers: axum::http::HeaderMap,
    Json(input): Json<CreateCommentRequest>,
) -> Result<Json<cowiki_db::review_comments::ReviewComment>> {
    let user = extract_user(&state.db, &headers).await?;
    let comment = cowiki_db::review_comments::create(
        &state.db,
        submission_id,
        user.id,
        &input.file_path,
        input.line_number,
        &input.body,
        input.parent_id,
    )
    .await?;
    Ok(Json(comment))
}

/// Resolve a comment thread
pub async fn resolve_comment(
    State(state): State<Arc<AppState>>,
    Path((_ws_slug, _submission_id, comment_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    headers: axum::http::HeaderMap,
) -> Result<Json<cowiki_db::review_comments::ReviewComment>> {
    let _user = extract_user(&state.db, &headers).await?;
    let comment = cowiki_db::review_comments::resolve(&state.db, comment_id).await?;
    Ok(Json(comment))
}

/// Unresolve a comment thread
pub async fn unresolve_comment(
    State(state): State<Arc<AppState>>,
    Path((_ws_slug, _submission_id, comment_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    headers: axum::http::HeaderMap,
) -> Result<Json<cowiki_db::review_comments::ReviewComment>> {
    let _user = extract_user(&state.db, &headers).await?;
    let comment = cowiki_db::review_comments::unresolve(&state.db, comment_id).await?;
    Ok(Json(comment))
}

/// Delete a comment
pub async fn delete_comment(
    State(state): State<Arc<AppState>>,
    Path((_ws_slug, _submission_id, comment_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>> {
    let _user = extract_user(&state.db, &headers).await?;
    cowiki_db::review_comments::delete(&state.db, comment_id).await?;
    Ok(Json(serde_json::json!({"ok": true})))
}
