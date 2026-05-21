use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::{AppError, Result};
use crate::AppState;

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub name: String,
    pub email: Option<String>,
}

#[derive(Serialize)]
pub struct AuthResponse {
    pub user: UserInfo,
    pub api_key: String,
}

#[derive(Serialize)]
pub struct UserInfo {
    pub id: String,
    pub name: String,
    pub email: Option<String>,
}

/// Register a new user, returns user info + API key
pub async fn register(
    State(state): State<Arc<AppState>>,
    Json(input): Json<RegisterRequest>,
) -> Result<Json<AuthResponse>> {
    // Check if name is taken
    if cowiki_db::users::find_by_name(&state.db, &input.name).await?.is_some() {
        return Err(AppError::BadRequest("name already taken".into()));
    }

    let user = cowiki_db::users::create(&state.db, &input.name, input.email.as_deref(), None).await?;

    // Create user branch in Git
    let branch_name = state
        .wiki_repo
        .ensure_user_branch(&user.id.to_string())
        .map_err(|e| AppError::Internal(e.to_string()))?;
    tracing::info!("created branch {} for user {}", branch_name, user.name);

    Ok(Json(AuthResponse {
        api_key: user.api_key.clone(),
        user: UserInfo {
            id: user.id.to_string(),
            name: user.name,
            email: user.email,
        },
    }))
}

/// Get current user info (from API key)
pub async fn me(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<UserInfo>> {
    let user = extract_user(&state.db, &headers).await?;
    Ok(Json(UserInfo {
        id: user.id.to_string(),
        name: user.name,
        email: user.email,
    }))
}

/// Regenerate API key
pub async fn regenerate_key(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<AuthResponse>> {
    let user = extract_user(&state.db, &headers).await?;
    let updated = cowiki_db::users::regenerate_api_key(&state.db, user.id).await?;

    Ok(Json(AuthResponse {
        api_key: updated.api_key.clone(),
        user: UserInfo {
            id: updated.id.to_string(),
            name: updated.name,
            email: updated.email,
        },
    }))
}

/// Extract user from Authorization header (Bearer <api_key>)
pub async fn extract_user(
    db: &sqlx::PgPool,
    headers: &axum::http::HeaderMap,
) -> Result<cowiki_db::users::User> {
    let auth_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::BadRequest("missing Authorization header".into()))?;

    let api_key = auth_header
        .strip_prefix("Bearer ")
        .ok_or_else(|| AppError::BadRequest("invalid Authorization format, use: Bearer <api_key>".into()))?;

    cowiki_db::users::find_by_api_key(db, api_key)
        .await?
        .ok_or_else(|| AppError::BadRequest("invalid API key".into()))
}

/// Helper: get the user's personal branch name
pub fn user_branch(user: &cowiki_db::users::User) -> String {
    format!("user/{}", user.id)
}
