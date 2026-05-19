use sqlx::PgPool;
use uuid::Uuid;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Submission {
    pub id: Uuid,
    pub user_id: Uuid,
    pub status: String,
    pub summary: String,
    pub page_slugs: Vec<String>,
    pub source_branch: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub reviewed_by: Option<Uuid>,
    pub reviewed_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn create(
    pool: &PgPool,
    user_id: Uuid,
    summary: &str,
    page_slugs: &[String],
    source_branch: &str,
) -> sqlx::Result<Submission> {
    sqlx::query_as::<_, Submission>(
        "INSERT INTO submissions (user_id, summary, page_slugs, source_branch) VALUES ($1, $2, $3, $4) RETURNING *",
    )
    .bind(user_id)
    .bind(summary)
    .bind(page_slugs)
    .bind(source_branch)
    .fetch_one(pool)
    .await
}

pub async fn list_pending(pool: &PgPool) -> sqlx::Result<Vec<Submission>> {
    sqlx::query_as::<_, Submission>(
        "SELECT * FROM submissions WHERE status = 'pending' ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await
}

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<Submission>> {
    sqlx::query_as::<_, Submission>("SELECT * FROM submissions WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn update_status(
    pool: &PgPool,
    id: Uuid,
    status: &str,
    reviewer_id: Uuid,
) -> sqlx::Result<Submission> {
    sqlx::query_as::<_, Submission>(
        "UPDATE submissions SET status = $2, reviewed_by = $3, reviewed_at = now() WHERE id = $1 RETURNING *",
    )
    .bind(id)
    .bind(status)
    .bind(reviewer_id)
    .fetch_one(pool)
    .await
}
