use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AppState;
use crate::auth::AuthenticatedUser;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::git_repo::{GitRepoError, MarkdownTreeEntry};

#[derive(Debug, Deserialize)]
struct RefQuery {
    #[serde(rename = "ref", default = "main_ref")]
    reference: String,
}

#[derive(Debug, Deserialize)]
struct ContentQuery {
    #[serde(rename = "ref", default = "main_ref")]
    reference: String,
    path: String,
}

#[derive(Debug, Serialize)]
struct TreeResponse {
    #[serde(rename = "ref")]
    reference: String,
    oid: String,
    entries: Vec<TreeEntryResponse>,
}

#[derive(Debug, Serialize)]
struct TreeEntryResponse {
    path: String,
    kind: String,
}

#[derive(Debug, Serialize)]
struct ContentResponse {
    #[serde(rename = "ref")]
    reference: String,
    oid: String,
    path: String,
    content: String,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/spaces/{space_id}/tree", get(tree))
        .route("/api/spaces/{space_id}/content", get(content))
}

async fn tree(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(space_id): Path<Uuid>,
    Query(query): Query<RefQuery>,
) -> AppResult<Json<TreeResponse>> {
    require_member(&state, space_id, user.user.id).await?;
    require_main(&query.reference)?;
    let repos = state.repos.clone();
    let reference = query.reference.clone();
    let snapshot =
        tokio::task::spawn_blocking(move || repos.read_markdown_tree(space_id, &reference))
            .await
            .map_err(|error| AppError::Internal(format!("Cloud content task failed: {error}")))?
            .map_err(git_error)?;
    Ok(Json(TreeResponse {
        reference: query.reference,
        oid: snapshot.oid,
        entries: snapshot
            .entries
            .into_iter()
            .map(tree_entry_response)
            .collect(),
    }))
}

async fn content(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(space_id): Path<Uuid>,
    Query(query): Query<ContentQuery>,
) -> AppResult<Json<ContentResponse>> {
    require_member(&state, space_id, user.user.id).await?;
    require_main(&query.reference)?;
    let repos = state.repos.clone();
    let reference = query.reference.clone();
    let requested_path = query.path.clone();
    let blob = tokio::task::spawn_blocking(move || {
        repos.read_content_blob(space_id, &reference, &requested_path)
    })
    .await
    .map_err(|error| AppError::Internal(format!("Cloud content task failed: {error}")))?
    .map_err(git_error)?;
    if !std::path::Path::new(&blob.path)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
    {
        return Err(AppError::UnsupportedMediaType);
    }
    let content = String::from_utf8(blob.bytes).map_err(|_| AppError::UnsupportedMediaType)?;
    Ok(Json(ContentResponse {
        reference: query.reference,
        oid: blob.oid,
        path: blob.path,
        content,
    }))
}

async fn require_member(state: &AppState, space_id: Uuid, user_id: Uuid) -> AppResult<()> {
    db::member_role(&state.pool, space_id, user_id)
        .await?
        .map(|_| ())
        .ok_or(AppError::NotFound)
}

fn require_main(reference: &str) -> AppResult<()> {
    if reference == "main" {
        Ok(())
    } else {
        Err(AppError::BadRequest(
            "Cloud content can only read the main ref".into(),
        ))
    }
}

fn tree_entry_response(entry: MarkdownTreeEntry) -> TreeEntryResponse {
    TreeEntryResponse {
        path: entry.path,
        kind: entry.kind,
    }
}

fn git_error(error: GitRepoError) -> AppError {
    match error {
        GitRepoError::InvalidPath(path) => {
            AppError::BadRequest(format!("invalid repository path: {path}"))
        }
        GitRepoError::MissingRef(_) | GitRepoError::ObjectNotFound(_) => AppError::NotFound,
        GitRepoError::InvalidReceive(message) => AppError::BadRequest(message),
        other => AppError::Internal(other.to_string()),
    }
}

fn main_ref() -> String {
    "main".into()
}
