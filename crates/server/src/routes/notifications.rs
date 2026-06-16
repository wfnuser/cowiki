use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::error::Result;
use crate::routes::auth::extract_user;
use crate::AppState;

#[derive(Serialize)]
pub struct NotificationResponse {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub body: Option<String>,
    pub workspace_id: Option<String>,
    /// Workspace name/slug (joined) for display and the per-space filter; null for
    /// cross-space notifications with no associated workspace.
    pub workspace_name: Option<String>,
    pub workspace_slug: Option<String>,
    pub link: Option<String>,
    pub read: bool,
    pub created_at: String,
}

fn notif_response(n: &cowiki_db::notifications::Notification) -> NotificationResponse {
    NotificationResponse {
        id: n.id.to_string(),
        kind: n.kind.clone(),
        title: n.title.clone(),
        body: n.body.clone(),
        workspace_id: n.workspace_id.map(|id| id.to_string()),
        workspace_name: n.workspace_name.clone(),
        workspace_slug: n.workspace_slug.clone(),
        link: n.link.clone(),
        read: n.read,
        created_at: n.created_at.to_rfc3339(),
    }
}

#[derive(Deserialize)]
pub struct NotificationQuery {
    limit: Option<i64>,
}

/// GET /api/notifications
pub async fn list_notifications(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Query(query): Query<NotificationQuery>,
) -> Result<Json<Vec<NotificationResponse>>> {
    let user = extract_user(&state.db, &headers).await?;
    let limit = query.limit.unwrap_or(50);
    let notifs = cowiki_db::notifications::list_for_user(&state.db, user.id, limit).await?;
    Ok(Json(notifs.iter().map(notif_response).collect()))
}

/// GET /api/notifications/unread-count
pub async fn unread_count(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>> {
    let user = extract_user(&state.db, &headers).await?;
    let count = cowiki_db::notifications::unread_count(&state.db, user.id).await?;
    Ok(Json(serde_json::json!({"count": count})))
}

/// POST /api/notifications/{id}/read
pub async fn mark_read(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(notification_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>> {
    let user = extract_user(&state.db, &headers).await?;
    cowiki_db::notifications::mark_read(&state.db, notification_id, user.id).await?;
    Ok(Json(serde_json::json!({"ok": true})))
}

/// POST /api/notifications/{id}/unread
pub async fn mark_unread(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(notification_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>> {
    let user = extract_user(&state.db, &headers).await?;
    cowiki_db::notifications::mark_unread(&state.db, notification_id, user.id).await?;
    Ok(Json(serde_json::json!({"ok": true})))
}

/// POST /api/notifications/read-all
pub async fn mark_all_read(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>> {
    let user = extract_user(&state.db, &headers).await?;
    let count = cowiki_db::notifications::mark_all_read(&state.db, user.id).await?;
    Ok(Json(serde_json::json!({"cleared": count})))
}
