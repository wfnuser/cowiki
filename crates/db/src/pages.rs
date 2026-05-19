use sqlx::PgPool;
use uuid::Uuid;
use serde::{Deserialize, Serialize};
use pgvector::Vector;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PageMeta {
    pub id: Uuid,
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub branch: String,
    pub content_hash: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

pub async fn upsert(
    pool: &PgPool,
    slug: &str,
    title: &str,
    summary: &str,
    branch: &str,
    content_hash: &str,
    embedding: Option<&[f32]>,
    user_id: Uuid,
) -> sqlx::Result<PageMeta> {
    let emb = embedding.map(|e| Vector::from(e.to_vec()));
    sqlx::query_as::<_, PageMeta>(
        r#"INSERT INTO pages (slug, title, summary, branch, content_hash, embedding, created_by)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (slug, branch) DO UPDATE SET
            title = EXCLUDED.title,
            summary = EXCLUDED.summary,
            content_hash = EXCLUDED.content_hash,
            embedding = COALESCE(EXCLUDED.embedding, pages.embedding),
            updated_at = now()
        RETURNING id, slug, title, summary, branch, content_hash, created_at, updated_at"#,
    )
    .bind(slug)
    .bind(title)
    .bind(summary)
    .bind(branch)
    .bind(content_hash)
    .bind(emb)
    .bind(user_id)
    .fetch_one(pool)
    .await
}

pub async fn list_by_branch(pool: &PgPool, branch: &str) -> sqlx::Result<Vec<PageMeta>> {
    sqlx::query_as::<_, PageMeta>(
        "SELECT id, slug, title, summary, branch, content_hash, created_at, updated_at FROM pages WHERE branch = $1 ORDER BY updated_at DESC",
    )
    .bind(branch)
    .fetch_all(pool)
    .await
}

pub async fn find_similar(
    pool: &PgPool,
    embedding: &[f32],
    branch: &str,
    limit: i64,
    threshold: f64,
) -> sqlx::Result<Vec<(PageMeta, f64)>> {
    let emb = Vector::from(embedding.to_vec());

    // Use a subquery to compute similarity then filter
    let rows = sqlx::query_as::<_, (Uuid, String, String, String, String, String, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>, f64)>(
        r#"SELECT id, slug, title, summary, branch, content_hash, created_at, updated_at,
            1 - (embedding <=> $1::vector) as similarity
        FROM pages
        WHERE branch = $2 AND embedding IS NOT NULL
          AND 1 - (embedding <=> $1::vector) > $3
        ORDER BY similarity DESC
        LIMIT $4"#,
    )
    .bind(&emb)
    .bind(branch)
    .bind(threshold)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            (
                PageMeta {
                    id: r.0,
                    slug: r.1,
                    title: r.2,
                    summary: r.3,
                    branch: r.4,
                    content_hash: r.5,
                    created_at: r.6,
                    updated_at: r.7,
                },
                r.8,
            )
        })
        .collect())
}
