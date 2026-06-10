use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Notification {
    pub id: Uuid,
    pub user_id: Uuid,
    pub kind: String,
    pub title: String,
    pub body: Option<String>,
    pub workspace_id: Option<Uuid>,
    pub link: Option<String>,
    pub read: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn create(
    pool: &PgPool,
    user_id: Uuid,
    kind: &str,
    title: &str,
    body: Option<&str>,
    workspace_id: Option<Uuid>,
    link: Option<&str>,
) -> sqlx::Result<Notification> {
    sqlx::query_as::<_, Notification>(
        "INSERT INTO notifications (user_id, kind, title, body, workspace_id, link) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING *",
    )
    .bind(user_id)
    .bind(kind)
    .bind(title)
    .bind(body)
    .bind(workspace_id)
    .bind(link)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        tracing::error!("DB create notification failed: {e}");
        e
    })
}

pub async fn list_for_user(
    pool: &PgPool,
    user_id: Uuid,
    limit: i64,
) -> sqlx::Result<Vec<Notification>> {
    sqlx::query_as::<_, Notification>(
        "SELECT * FROM notifications WHERE user_id = $1 ORDER BY read ASC, created_at DESC LIMIT $2",
    )
    .bind(user_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| {
        tracing::error!("DB list notifications failed: {e}");
        e
    })
}

pub async fn unread_count(pool: &PgPool, user_id: Uuid) -> sqlx::Result<i64> {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM notifications WHERE user_id = $1 AND read = FALSE",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        tracing::error!("DB unread count failed: {e}");
        e
    })
}

pub async fn mark_read(pool: &PgPool, notification_id: Uuid, user_id: Uuid) -> sqlx::Result<bool> {
    let rows = sqlx::query("UPDATE notifications SET read = TRUE WHERE id = $1 AND user_id = $2")
        .bind(notification_id)
        .bind(user_id)
        .execute(pool)
        .await
        .map_err(|e| {
            tracing::error!("DB mark read failed: {e}");
            e
        })?;
    Ok(rows.rows_affected() > 0)
}

pub async fn mark_all_read(pool: &PgPool, user_id: Uuid) -> sqlx::Result<u64> {
    let rows =
        sqlx::query("UPDATE notifications SET read = TRUE WHERE user_id = $1 AND read = FALSE")
            .bind(user_id)
            .execute(pool)
            .await
            .map_err(|e| {
                tracing::error!("DB mark all read failed: {e}");
                e
            })?;
    Ok(rows.rows_affected())
}
