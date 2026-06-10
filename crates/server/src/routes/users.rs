use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::Result;
use crate::routes::auth::extract_user;
use crate::AppState;

#[derive(Deserialize)]
pub struct UserSearchQuery {
    pub q: String,
    pub limit: Option<i64>,
}

#[derive(Serialize)]
pub struct UserSearchResponse {
    pub id: String,
    pub name: String,
    pub email: Option<String>,
}

/// GET /api/users/search?q=...&limit=10
pub async fn search_users(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Query(query): Query<UserSearchQuery>,
) -> Result<Json<Vec<UserSearchResponse>>> {
    let _user = extract_user(&state.db, &headers).await?;
    let limit = query.limit.unwrap_or(10);
    let users = cowiki_db::users::search(&state.db, &query.q, limit).await?;
    Ok(Json(
        users
            .into_iter()
            .map(|u| UserSearchResponse {
                id: u.id.to_string(),
                name: u.name,
                email: u.email,
            })
            .collect(),
    ))
}
