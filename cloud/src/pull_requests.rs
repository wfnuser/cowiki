use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AppState;
use crate::auth::AuthenticatedUser;
use crate::db::{self, PullRequestRecord};
use crate::error::{AppError, AppResult};
use crate::git_repo::GitRepoError;
use crate::model::{MemberRole, PullRequestStatus};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreatePullRequest {
    title: String,
    #[serde(default)]
    body: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MergePullRequest {
    expected_head_oid: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestResponse {
    pub id: Uuid,
    pub space_id: Uuid,
    pub number: i64,
    pub author_id: Uuid,
    pub title: String,
    pub body: String,
    pub base_ref: String,
    pub head_ref: String,
    pub base_oid: String,
    pub head_oid: String,
    pub status: PullRequestStatus,
    pub merged_by: Option<Uuid>,
    pub approval_count: i64,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/spaces/{space_id}/pull-requests",
            post(create_pull_request).get(list_pull_requests),
        )
        .route(
            "/api/spaces/{space_id}/pull-requests/{pull_request_id}",
            get(get_pull_request),
        )
        .route(
            "/api/spaces/{space_id}/pull-requests/{pull_request_id}/approve",
            post(approve_pull_request),
        )
        .route(
            "/api/spaces/{space_id}/pull-requests/{pull_request_id}/merge",
            post(merge_pull_request),
        )
}

async fn create_pull_request(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(space_id): Path<Uuid>,
    Json(input): Json<CreatePullRequest>,
) -> AppResult<(StatusCode, Json<PullRequestResponse>)> {
    let role = require_member(&state, space_id, user.user.id).await?;
    if !role.can_push() {
        return Err(AppError::Forbidden);
    }
    let title = input.title.trim();
    if title.is_empty() || title.chars().count() > 240 {
        return Err(AppError::BadRequest(
            "pull request title must be between 1 and 240 characters".into(),
        ));
    }
    let head_ref = format!("user/{}", user.user.id);
    let base_oid = required_ref(&state, space_id, "main")?;
    let head_oid = required_ref(&state, space_id, &head_ref)?;
    if base_oid == head_oid {
        return Err(AppError::Conflict(
            "the user branch has no changes relative to Cloud main".into(),
        ));
    }
    let (record, created) = db::create_or_update_pull_request(
        &state.pool,
        space_id,
        user.user.id,
        title,
        input.body.trim(),
        &base_oid,
        &head_oid,
    )
    .await?;
    let response = response(&state, record).await?;
    Ok((
        if created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        Json(response),
    ))
}

async fn list_pull_requests(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(space_id): Path<Uuid>,
) -> AppResult<Json<Vec<PullRequestResponse>>> {
    require_member(&state, space_id, user.user.id).await?;
    let records = db::list_pull_requests(&state.pool, space_id).await?;
    let mut responses = Vec::with_capacity(records.len());
    for record in records {
        let record = reconcile_live_head(&state, record).await?;
        responses.push(response(&state, record).await?);
    }
    Ok(Json(responses))
}

async fn get_pull_request(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path((space_id, pull_request_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<PullRequestResponse>> {
    require_member(&state, space_id, user.user.id).await?;
    let record = db::get_pull_request(&state.pool, space_id, pull_request_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let record = reconcile_live_head(&state, record).await?;
    Ok(Json(response(&state, record).await?))
}

async fn approve_pull_request(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path((space_id, pull_request_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<PullRequestResponse>> {
    let role = require_member(&state, space_id, user.user.id).await?;
    if !role.can_push() {
        return Err(AppError::Forbidden);
    }
    let lock = state.repos.space_lock(space_id).map_err(git_error)?;
    let _guard = lock.lock().await;
    let record = db::get_pull_request(&state.pool, space_id, pull_request_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let record = reconcile_live_head(&state, record).await?;
    if record.status != PullRequestStatus::Open {
        return Err(AppError::Conflict("pull request is not open".into()));
    }
    db::approve_pull_request(&state.pool, &record, user.user.id).await?;
    Ok(Json(response(&state, record).await?))
}

async fn merge_pull_request(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path((space_id, pull_request_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<MergePullRequest>,
) -> AppResult<Json<PullRequestResponse>> {
    let role = require_member(&state, space_id, user.user.id).await?;
    if !role.can_merge() {
        return Err(AppError::Forbidden);
    }
    let lock = state.repos.space_lock(space_id).map_err(git_error)?;
    let _guard = lock.lock().await;
    let mut transaction = state.pool.begin().await?;
    let mut record = db::lock_pull_request_for_merge(&mut transaction, space_id, pull_request_id)
        .await?
        .ok_or(AppError::NotFound)?;

    if record.status == PullRequestStatus::Merged {
        if record.head_oid != input.expected_head_oid {
            return Err(AppError::Conflict(format!(
                "pull request head is {}; refresh before merging",
                record.head_oid
            )));
        }
        transaction.commit().await?;
        return Ok(Json(response(&state, record).await?));
    }
    if record.status != PullRequestStatus::Open {
        return Err(AppError::Conflict("pull request is not open".into()));
    }

    let base_oid = required_ref(&state, space_id, "main")?;
    let head_oid = required_ref(&state, space_id, &record.head_ref)?;
    record = db::reconcile_pull_request_head_in_transaction(
        &mut transaction,
        &record,
        &base_oid,
        &head_oid,
    )
    .await?;
    if input.expected_head_oid != record.head_oid {
        return Err(AppError::Conflict(format!(
            "pull request head changed to {}; refresh before merging",
            record.head_oid
        )));
    }

    state
        .repos
        .fast_forward_main(space_id, &record.head_ref, &input.expected_head_oid)
        .map_err(git_error)?;
    let merged = db::mark_pull_request_merged(&mut transaction, &record, user.user.id).await?;
    transaction.commit().await?;
    Ok(Json(response(&state, merged).await?))
}

async fn require_member(state: &AppState, space_id: Uuid, user_id: Uuid) -> AppResult<MemberRole> {
    db::member_role(&state.pool, space_id, user_id)
        .await?
        .ok_or(AppError::NotFound)
}

fn required_ref(state: &AppState, space_id: Uuid, branch: &str) -> AppResult<String> {
    state
        .repos
        .ref_oid(space_id, branch)
        .map_err(git_error)?
        .ok_or_else(|| {
            AppError::Conflict(format!(
                "Cloud ref {branch} is missing; initialize or push the Space first"
            ))
        })
}

async fn reconcile_live_head(
    state: &AppState,
    record: PullRequestRecord,
) -> AppResult<PullRequestRecord> {
    if record.status != PullRequestStatus::Open {
        return Ok(record);
    }
    let base_oid = required_ref(state, record.space_id, "main")?;
    let head_oid = required_ref(state, record.space_id, &record.head_ref)?;
    if record.base_oid == base_oid && record.head_oid == head_oid {
        return Ok(record);
    }
    db::reconcile_pull_request_head(&state.pool, record, &base_oid, &head_oid)
        .await
        .map_err(AppError::from)
}

async fn response(state: &AppState, record: PullRequestRecord) -> AppResult<PullRequestResponse> {
    let approval_count = db::approval_count(&state.pool, record.id, &record.head_oid).await?;
    Ok(PullRequestResponse {
        id: record.id,
        space_id: record.space_id,
        number: record.number,
        author_id: record.author_id,
        title: record.title,
        body: record.body,
        base_ref: record.base_ref,
        head_ref: record.head_ref,
        base_oid: record.base_oid,
        head_oid: record.head_oid,
        status: record.status,
        merged_by: record.merged_by,
        approval_count,
    })
}

fn git_error(error: GitRepoError) -> AppError {
    match error {
        GitRepoError::StaleHead { .. } | GitRepoError::NotFastForward => {
            AppError::Conflict(error.to_string())
        }
        GitRepoError::MissingRef(reference) => {
            AppError::Conflict(format!("Cloud ref {reference} is missing"))
        }
        GitRepoError::InvalidReceive(message) => AppError::BadRequest(message),
        other => AppError::Internal(other.to_string()),
    }
}
