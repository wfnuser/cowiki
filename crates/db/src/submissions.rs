use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

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
    /// Display name of the submitting user (joined in read queries; absent on writes).
    #[sqlx(default)]
    pub author_name: Option<String>,
    /// Display name of the reviewer, when reviewed (joined in read queries).
    #[sqlx(default)]
    pub reviewer_name: Option<String>,
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
        "SELECT s.*, u.name AS author_name, r.name AS reviewer_name FROM submissions s \
         JOIN users u ON u.id = s.user_id \
         LEFT JOIN users r ON r.id = s.reviewed_by \
         WHERE s.status IN ('pending', 'approved', 'rejected', 'merged') AND s.workspace_slug = $1 \
         ORDER BY s.created_at DESC",
    )
    .bind(workspace_slug)
    .fetch_all(pool)
    .await
    .map_err(|e| {
        tracing::error!("DB list submissions for workspace failed: {e}");
        e
    })
}

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<Submission>> {
    sqlx::query_as::<_, Submission>(
        "SELECT s.*, u.name AS author_name, r.name AS reviewer_name FROM submissions s \
         JOIN users u ON u.id = s.user_id \
         LEFT JOIN users r ON r.id = s.reviewed_by \
         WHERE s.id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        tracing::error!("DB find submission by id failed: {e}");
        e
    })
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

/// Replace a submission's summary (used by the background AI-summary task; submit stores a
/// diff-based placeholder immediately so it never blocks on the LLM).
pub async fn update_summary(pool: &PgPool, id: Uuid, summary: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE submissions SET summary = $2 WHERE id = $1")
        .bind(id)
        .bind(summary)
        .execute(pool)
        .await
        .map(|_| ())
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

    #[tokio::test]
    async fn submission_records_and_filters_by_workspace() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let user = make_user(&pool).await;

        let s = create(
            &pool,
            user,
            "summary",
            &["page-a".into()],
            "user/abc",
            "team-alpha",
        )
        .await
        .unwrap();
        assert_eq!(s.workspace_slug, "team-alpha");

        let in_alpha = list_pending_for_workspace(&pool, "team-alpha")
            .await
            .unwrap();
        assert!(in_alpha.iter().any(|x| x.id == s.id));

        let in_beta = list_pending_for_workspace(&pool, "team-beta")
            .await
            .unwrap();
        assert!(!in_beta.iter().any(|x| x.id == s.id));
    }

    #[tokio::test]
    async fn review_list_includes_active_review_states() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let user = make_user(&pool).await;

        let approved = create(
            &pool,
            user,
            "approved",
            &["page-a".into()],
            "user/a",
            "team-alpha",
        )
        .await
        .unwrap();
        let merged = create(
            &pool,
            user,
            "merged",
            &["page-b".into()],
            "user/b",
            "team-alpha",
        )
        .await
        .unwrap();
        let rejected = create(
            &pool,
            user,
            "rejected",
            &["page-c".into()],
            "user/c",
            "team-alpha",
        )
        .await
        .unwrap();

        update_status(&pool, approved.id, "approved", user)
            .await
            .unwrap();
        update_status(&pool, merged.id, "merged", user)
            .await
            .unwrap();
        update_status(&pool, rejected.id, "rejected", user)
            .await
            .unwrap();

        let in_alpha = list_pending_for_workspace(&pool, "team-alpha")
            .await
            .unwrap();
        assert!(in_alpha.iter().any(|x| x.id == approved.id));
        assert!(in_alpha.iter().any(|x| x.id == merged.id));
        assert!(in_alpha.iter().any(|x| x.id == rejected.id));
    }
}
