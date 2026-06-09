use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::AppState;

#[derive(Serialize)]
pub struct KeyResponse {
    pub id: String,
    pub name: String,
    pub key_prefix: String, // masked: "cw_****xxxx"
    pub last_used_at: Option<String>,
    pub created_at: String,
}

#[derive(Deserialize)]
pub struct CreateKeyRequest {
    pub name: String,
}

#[derive(Serialize)]
pub struct CreateKeyResponse {
    pub id: String,
    pub name: String,
    pub key_prefix: String,
    pub raw_key: String, // ONLY returned here — store it now!
    pub created_at: String,
}

/// GET /api/keys — list all active API keys for the current user
pub async fn list_keys(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Vec<KeyResponse>>> {
    let user = crate::routes::auth::extract_user(&state.db, &headers).await?;

    let keys = cowiki_db::api_keys::list_for_user(&state.db, user.id).await?;

    let response: Vec<KeyResponse> = keys
        .into_iter()
        .map(|k| KeyResponse {
            id: k.id.to_string(),
            name: k.name,
            key_prefix: cowiki_db::api_keys::mask_key_prefix(&k.key_prefix),
            last_used_at: k.last_used_at.map(|t| t.to_rfc3339()),
            created_at: k.created_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(response))
}

/// POST /api/keys — create a new API key
pub async fn create_key(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(input): Json<CreateKeyRequest>,
) -> Result<Json<CreateKeyResponse>> {
    let user = crate::routes::auth::extract_user(&state.db, &headers).await?;

    let name = input.name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }

    let created = cowiki_db::api_keys::create(&state.db, user.id, name).await?;

    Ok(Json(CreateKeyResponse {
        id: created.id.to_string(),
        name: created.name,
        key_prefix: created.key_prefix,
        raw_key: created.raw_key,
        created_at: created.created_at.to_rfc3339(),
    }))
}

/// DELETE /api/keys/:id — revoke an API key (soft-delete: sets revoked_at)
pub async fn revoke_key(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(key_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>> {
    let user = crate::routes::auth::extract_user(&state.db, &headers).await?;

    let revoked = cowiki_db::api_keys::revoke(&state.db, key_id, user.id).await?;

    if !revoked {
        return Err(AppError::NotFound(
            "key not found or already revoked".into(),
        ));
    }

    Ok(Json(serde_json::json!({ "revoked": true })))
}
