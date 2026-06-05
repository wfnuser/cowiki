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
    pub workspace_slug: String,
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
    workspace_slug: &str,
) -> sqlx::Result<Submission> {
    sqlx::query_as::<_, Submission>(
        "INSERT INTO submissions (user_id, summary, page_slugs, source_branch, workspace_slug) \
         VALUES ($1, $2, $3, $4, $5) RETURNING *",
    )
    .bind(user_id)
    .bind(summary)
    .bind(page_slugs)
    .bind(source_branch)
    .bind(workspace_slug)
    .fetch_one(pool)
    .await
}

pub async fn list_pending_for_workspace(
    pool: &PgPool,
    workspace_slug: &str,
) -> sqlx::Result<Vec<Submission>> {
    sqlx::query_as::<_, Submission>(
        "SELECT * FROM submissions WHERE status = 'pending' AND workspace_slug = $1 \
         ORDER BY created_at DESC",
    )
    .bind(workspace_slug)
    .fetch_all(pool)
    .await
    .map_err(|e| { tracing::error!("DB list submissions for workspace failed: {e}"); e })
}

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<Submission>> {
    sqlx::query_as::<_, Submission>("SELECT * FROM submissions WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| { tracing::error!("DB find submission by id failed: {e}"); e })
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

    #[tokio::test]
    async fn submission_records_and_filters_by_workspace() {
        let Some(pool) = test_pool().await else { return };
        let user = make_user(&pool).await;

        let s = create(&pool, user, "summary", &["page-a".into()], "user/abc", "team-alpha")
            .await
            .unwrap();
        assert_eq!(s.workspace_slug, "team-alpha");

        let in_alpha = list_pending_for_workspace(&pool, "team-alpha").await.unwrap();
        assert!(in_alpha.iter().any(|x| x.id == s.id));

        let in_beta = list_pending_for_workspace(&pool, "team-beta").await.unwrap();
        assert!(!in_beta.iter().any(|x| x.id == s.id));
    }
}
