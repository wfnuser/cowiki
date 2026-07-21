use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AppState;
use crate::auth::AuthenticatedUser;
use crate::db::{self, SpaceMembership};
use crate::error::{AppError, AppResult};
use crate::model::MemberRole;

#[derive(Debug, Deserialize)]
pub struct CreateSpaceRequest {
    pub name: String,
    pub slug: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudSpace {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub role: MemberRole,
    pub git_url: String,
    pub main_ref: String,
    pub user_ref: String,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/spaces", post(create_space).get(list_spaces))
        .route("/api/spaces/{space_id}", get(get_space))
}

async fn create_space(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(input): Json<CreateSpaceRequest>,
) -> AppResult<(StatusCode, Json<CloudSpace>)> {
    let name = input.name.trim();
    let slug = input.slug.trim();
    validate_space_input(name, slug).map_err(AppError::BadRequest)?;
    let space_id = Uuid::new_v4();
    let membership = match db::create_space(&state.pool, space_id, user.user.id, name, slug).await {
        Ok(space) => space,
        Err(error) if is_unique_violation(&error) => {
            return Err(AppError::Conflict("Space slug is already in use".into()));
        }
        Err(error) => return Err(error.into()),
    };
    if let Err(error) = state.repos.ensure_space(space_id) {
        if let Err(cleanup) = db::delete_space_after_repository_failure(&state.pool, space_id).await
        {
            tracing::error!(%space_id, %cleanup, "failed to compensate Space repository creation");
        }
        return Err(AppError::Internal(error.to_string()));
    }
    let response = space_response(&state, membership, user.user.id)?;
    Ok((StatusCode::CREATED, Json(response)))
}

async fn list_spaces(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> AppResult<Json<Vec<CloudSpace>>> {
    let spaces = db::list_spaces_for_user(&state.pool, user.user.id).await?;
    let spaces = spaces
        .into_iter()
        .map(|space| space_response(&state, space, user.user.id))
        .collect::<AppResult<Vec<_>>>()?;
    Ok(Json(spaces))
}

async fn get_space(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(space_id): Path<Uuid>,
) -> AppResult<Json<CloudSpace>> {
    let space = db::get_space_for_user(&state.pool, space_id, user.user.id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(space_response(&state, space, user.user.id)?))
}

fn space_response(
    state: &AppState,
    membership: SpaceMembership,
    user_id: Uuid,
) -> AppResult<CloudSpace> {
    let git_url = state
        .config
        .public_origin
        .join(&format!("/git/{}.git", membership.id))
        .map_err(|error| AppError::Internal(error.to_string()))?;
    Ok(CloudSpace {
        id: membership.id,
        name: membership.name,
        slug: membership.slug,
        role: membership.role,
        git_url: git_url.to_string(),
        main_ref: "main".into(),
        user_ref: format!("user/{user_id}"),
    })
}

pub fn validate_space_input(name: &str, slug: &str) -> Result<(), String> {
    if name.is_empty() || name.chars().count() > 120 {
        return Err("Space name must be between 1 and 120 characters".into());
    }
    let valid_slug = !slug.is_empty()
        && slug.len() <= 63
        && slug.is_ascii()
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && slug
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && slug
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric);
    if !valid_slug {
        return Err("Space slug must use lowercase letters, numbers, and interior hyphens".into());
    }
    Ok(())
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .is_some_and(sqlx::error::DatabaseError::is_unique_violation)
}
