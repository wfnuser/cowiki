//! Document comments anchored to source line ranges of a wiki page.
//!
//! The page markdown itself is never touched (clean diffs); each top-level
//! comment stores a line-range anchor plus a `content_hash` pointing at a
//! snapshot of the page as the author saw it. The client re-maps the anchor
//! onto whatever branch version it is rendering via a git-style line diff.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PageComment {
    pub id: Uuid,
    pub workspace_slug: String,
    pub page_slug: String,
    pub user_id: Uuid,
    /// Anchor snapshot hash; `None` for replies.
    pub content_hash: Option<String>,
    /// 1-based inclusive source line range; `None` for replies.
    pub start_line: Option<i32>,
    pub end_line: Option<i32>,
    pub body: String,
    pub parent_id: Option<Uuid>,
    pub resolved: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// A deduplicated snapshot of the page markdown an anchor was created against.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CommentSnapshot {
    pub content_hash: String,
    pub source: String,
}

#[allow(clippy::too_many_arguments)]
pub async fn create(
    pool: &PgPool,
    workspace_slug: &str,
    page_slug: &str,
    user_id: Uuid,
    content_hash: Option<&str>,
    start_line: Option<i32>,
    end_line: Option<i32>,
    body: &str,
    parent_id: Option<Uuid>,
) -> sqlx::Result<PageComment> {
    sqlx::query_as::<_, PageComment>(
        "INSERT INTO page_comments \
         (workspace_slug, page_slug, user_id, content_hash, start_line, end_line, body, parent_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING *",
    )
    .bind(workspace_slug)
    .bind(page_slug)
    .bind(user_id)
    .bind(content_hash)
    .bind(start_line)
    .bind(end_line)
    .bind(body)
    .bind(parent_id)
    .fetch_one(pool)
    .await
}

pub async fn list_for_page(
    pool: &PgPool,
    workspace_slug: &str,
    page_slug: &str,
) -> sqlx::Result<Vec<PageComment>> {
    sqlx::query_as::<_, PageComment>(
        "SELECT * FROM page_comments WHERE workspace_slug = $1 AND page_slug = $2 \
         ORDER BY created_at ASC",
    )
    .bind(workspace_slug)
    .bind(page_slug)
    .fetch_all(pool)
    .await
}

/// Fetch a single comment (used to authorize/locate before mutation).
pub async fn find(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<PageComment>> {
    sqlx::query_as::<_, PageComment>("SELECT * FROM page_comments WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn set_resolved(pool: &PgPool, id: Uuid, resolved: bool) -> sqlx::Result<PageComment> {
    sqlx::query_as::<_, PageComment>(
        "UPDATE page_comments SET resolved = $2, updated_at = now() WHERE id = $1 RETURNING *",
    )
    .bind(id)
    .bind(resolved)
    .fetch_one(pool)
    .await
}

pub async fn delete(pool: &PgPool, id: Uuid) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM page_comments WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Store the page snapshot an anchor was created against (idempotent by hash).
pub async fn upsert_snapshot(
    pool: &PgPool,
    workspace_slug: &str,
    page_slug: &str,
    content_hash: &str,
    source: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO page_comment_snapshots (workspace_slug, page_slug, content_hash, source) \
         VALUES ($1, $2, $3, $4) ON CONFLICT (workspace_slug, page_slug, content_hash) DO NOTHING",
    )
    .bind(workspace_slug)
    .bind(page_slug)
    .bind(content_hash)
    .bind(source)
    .execute(pool)
    .await?;
    Ok(())
}

/// All snapshots referenced by comments on a page, so the client can re-anchor.
pub async fn snapshots_for_page(
    pool: &PgPool,
    workspace_slug: &str,
    page_slug: &str,
) -> sqlx::Result<Vec<CommentSnapshot>> {
    sqlx::query_as::<_, CommentSnapshot>(
        "SELECT content_hash, source FROM page_comment_snapshots \
         WHERE workspace_slug = $1 AND page_slug = $2",
    )
    .bind(workspace_slug)
    .bind(page_slug)
    .fetch_all(pool)
    .await
}
