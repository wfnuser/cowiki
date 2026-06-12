use pgvector::Vector;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

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

#[allow(clippy::too_many_arguments)]
pub async fn upsert(
    pool: &PgPool,
    slug: &str,
    title: &str,
    summary: &str,
    branch: &str,
    content_hash: &str,
    embedding: Option<&[f32]>,
    user_id: Uuid,
    workspace_slug: &str,
) -> sqlx::Result<PageMeta> {
    let emb = embedding.map(|e| Vector::from(e.to_vec()));
    sqlx::query_as::<_, PageMeta>(
        r#"INSERT INTO pages (slug, title, summary, branch, content_hash, embedding, created_by, workspace_slug)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (slug, branch, workspace_slug) DO UPDATE SET
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
    .bind(workspace_slug)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        tracing::error!("DB upsert page {slug}: {e}");
        e
    })
}

pub async fn list_by_branch(pool: &PgPool, branch: &str) -> sqlx::Result<Vec<PageMeta>> {
    sqlx::query_as::<_, PageMeta>(
        "SELECT id, slug, title, summary, branch, content_hash, created_at, updated_at FROM pages WHERE branch = $1 ORDER BY updated_at DESC",
    )
    .bind(branch)
    .fetch_all(pool)
    .await
    .map_err(|e| { tracing::error!("DB list pages by branch {branch}: {e}"); e })
}

/// Find pages similar to `embedding` on `branch`. Pass `workspace_slug = Some(ws)`
/// to scope the search to one workspace (dedup/submit); `None` searches across all
/// workspaces (the global search route — see #44 for scoping that).
pub async fn find_similar(
    pool: &PgPool,
    embedding: &[f32],
    branch: &str,
    limit: i64,
    threshold: f64,
    workspace_slug: Option<&str>,
) -> sqlx::Result<Vec<(PageMeta, f64)>> {
    let emb = Vector::from(embedding.to_vec());

    // The inner query is the one shape pgvector can serve from the HNSW index
    // (`ORDER BY embedding <=> $1 LIMIT n`); the similarity threshold is applied
    // outside it. Filters (branch/workspace) post-filter the index scan, so the
    // inner LIMIT over-fetches a bit to compensate.
    let rows = sqlx::query_as::<
        _,
        (
            Uuid,
            String,
            String,
            String,
            String,
            String,
            chrono::DateTime<chrono::Utc>,
            chrono::DateTime<chrono::Utc>,
            f64,
        ),
    >(
        r#"SELECT id, slug, title, summary, branch, content_hash, created_at, updated_at, similarity
        FROM (
            SELECT id, slug, title, summary, branch, content_hash, created_at, updated_at,
                1 - (embedding <=> $1::vector) AS similarity
            FROM pages
            WHERE branch = $2 AND embedding IS NOT NULL
              AND ($5::text IS NULL OR workspace_slug = $5)
            ORDER BY embedding <=> $1::vector
            LIMIT GREATEST($4 * 4, 50)
        ) candidates
        WHERE similarity > $3
        ORDER BY similarity DESC
        LIMIT $4"#,
    )
    .bind(&emb)
    .bind(branch)
    .bind(threshold)
    .bind(limit)
    .bind(workspace_slug)
    .fetch_all(pool)
    .await
    .map_err(|e| {
        tracing::error!("DB error: {e}");
        e
    })?;

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

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_pool() -> Option<PgPool> {
        let url = std::env::var("TEST_DATABASE_URL").ok()?;
        let pool = PgPool::connect(&url).await.ok()?;
        let sql1 = include_str!("migrations/001_init.sql").replace("__EMBEDDING_DIM__", "768");
        let _ = sqlx::raw_sql(&sql1).execute(&pool).await;
        for m in [
            include_str!("migrations/002_workspaces.sql"),
            include_str!("migrations/003_workspace_visibility.sql"),
            include_str!("migrations/004_role_update.sql"),
            include_str!("migrations/005_fts.sql"),
            include_str!("migrations/006_api_keys.sql"),
            include_str!("migrations/007_team_permissions.sql"),
            include_str!("migrations/008_submission_workspace.sql"),
            include_str!("migrations/009_role_management.sql"),
            include_str!("migrations/009_review_comments.sql"),
            include_str!("migrations/010_notifications.sql"),
            include_str!("migrations/010_invitation_reject_status.sql"),
            include_str!("migrations/011_pages_workspace_scope.sql"),
        ] {
            let _ = sqlx::raw_sql(m).execute(&pool).await;
        }
        Some(pool)
    }

    async fn make_user(pool: &PgPool) -> Uuid {
        let id = Uuid::new_v4();
        let _ = sqlx::query(
            "INSERT INTO users (id, name, email, api_key) VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING",
        )
        .bind(id)
        .bind(format!("u-{id}"))
        .bind(format!("{id}@test.com"))
        .bind(format!("key-{id}"))
        .execute(pool)
        .await;
        id
    }

    /// Two workspaces sharing (slug, branch) must NOT overwrite each other, and a
    /// workspace-scoped find_similar must not leak the other workspace's page.
    #[tokio::test]
    async fn pages_are_scoped_per_workspace() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let u = make_user(&pool).await;
        let emb: Vec<f32> = vec![0.1; 768];

        let a = upsert(
            &pool,
            "retry",
            "A title",
            "",
            "main",
            "h1",
            Some(&emb),
            u,
            "team-a",
        )
        .await
        .unwrap();
        let b = upsert(
            &pool,
            "retry",
            "B title",
            "",
            "main",
            "h2",
            Some(&emb),
            u,
            "team-b",
        )
        .await
        .unwrap();
        assert_ne!(
            a.id, b.id,
            "same (slug, branch) in two workspaces overwrote each other"
        );

        let only_a = find_similar(&pool, &emb, "main", 10, 0.5, Some("team-a"))
            .await
            .unwrap();
        assert!(only_a
            .iter()
            .any(|(p, _)| p.slug == "retry" && p.title == "A title"));
        assert!(
            only_a.iter().all(|(p, _)| p.title != "B title"),
            "workspace-scoped search leaked another workspace's page"
        );

        let only_b = find_similar(&pool, &emb, "main", 10, 0.5, Some("team-b"))
            .await
            .unwrap();
        assert!(only_b.iter().any(|(p, _)| p.title == "B title"));
    }
}
