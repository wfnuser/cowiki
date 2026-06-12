use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ReviewComment {
    pub id: Uuid,
    pub submission_id: Uuid,
    pub user_id: Uuid,
    pub file_path: String,
    pub line_number: Option<i32>,
    pub body: String,
    pub parent_id: Option<Uuid>,
    pub resolved: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

pub async fn create(
    pool: &PgPool,
    submission_id: Uuid,
    user_id: Uuid,
    file_path: &str,
    line_number: Option<i32>,
    body: &str,
    parent_id: Option<Uuid>,
) -> sqlx::Result<ReviewComment> {
    sqlx::query_as::<_, ReviewComment>(
        "INSERT INTO review_comments (submission_id, user_id, file_path, line_number, body, parent_id) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING *",
    )
    .bind(submission_id)
    .bind(user_id)
    .bind(file_path)
    .bind(line_number)
    .bind(body)
    .bind(parent_id)
    .fetch_one(pool)
    .await
}

pub async fn list_for_submission(
    pool: &PgPool,
    submission_id: Uuid,
) -> sqlx::Result<Vec<ReviewComment>> {
    sqlx::query_as::<_, ReviewComment>(
        "SELECT * FROM review_comments WHERE submission_id = $1 ORDER BY created_at ASC",
    )
    .bind(submission_id)
    .fetch_all(pool)
    .await
}

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<ReviewComment>> {
    sqlx::query_as::<_, ReviewComment>("SELECT * FROM review_comments WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn resolve(pool: &PgPool, id: Uuid) -> sqlx::Result<ReviewComment> {
    sqlx::query_as::<_, ReviewComment>(
        "UPDATE review_comments SET resolved = TRUE, updated_at = now() WHERE id = $1 RETURNING *",
    )
    .bind(id)
    .fetch_one(pool)
    .await
}

pub async fn unresolve(pool: &PgPool, id: Uuid) -> sqlx::Result<ReviewComment> {
    sqlx::query_as::<_, ReviewComment>(
        "UPDATE review_comments SET resolved = FALSE, updated_at = now() WHERE id = $1 RETURNING *",
    )
    .bind(id)
    .fetch_one(pool)
    .await
}

pub async fn delete(pool: &PgPool, id: Uuid) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM review_comments WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
