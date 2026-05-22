use sqlx::PgPool;
use uuid::Uuid;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Workspace {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub visibility: String,
    pub created_by: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct WorkspaceMember {
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub role: String,
    pub joined_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Invitation {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub email: String,
    pub invited_by: Uuid,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn create(pool: &PgPool, name: &str, slug: &str, visibility: &str, created_by: Uuid) -> sqlx::Result<Workspace> {
    let ws = sqlx::query_as::<_, Workspace>(
        "INSERT INTO workspaces (name, slug, visibility, created_by) VALUES ($1, $2, $3, $4) RETURNING *"
    )
    .bind(name).bind(slug).bind(visibility).bind(created_by)
    .fetch_one(pool)
    .await?;

    // Add creator as owner
    sqlx::query(
        "INSERT INTO workspace_members (workspace_id, user_id, role) VALUES ($1, $2, 'owner')"
    )
    .bind(ws.id).bind(created_by)
    .execute(pool)
    .await?;

    Ok(ws)
}

pub async fn find_by_slug(pool: &PgPool, slug: &str) -> sqlx::Result<Option<Workspace>> {
    sqlx::query_as::<_, Workspace>("SELECT * FROM workspaces WHERE slug = $1")
        .bind(slug)
        .fetch_optional(pool)
        .await
}

pub async fn list_public(pool: &PgPool) -> sqlx::Result<Vec<Workspace>> {
    sqlx::query_as::<_, Workspace>(
        "SELECT * FROM workspaces WHERE visibility = 'public' ORDER BY created_at DESC"
    )
    .fetch_all(pool)
    .await
}

pub async fn list_for_user(pool: &PgPool, user_id: Uuid) -> sqlx::Result<Vec<Workspace>> {
    sqlx::query_as::<_, Workspace>(
        "SELECT w.* FROM workspaces w JOIN workspace_members wm ON w.id = wm.workspace_id WHERE wm.user_id = $1 ORDER BY w.created_at DESC"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

pub async fn is_member(pool: &PgPool, workspace_id: Uuid, user_id: Uuid) -> sqlx::Result<bool> {
    let row = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM workspace_members WHERE workspace_id = $1 AND user_id = $2"
    )
    .bind(workspace_id).bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok(row > 0)
}

pub async fn add_member(pool: &PgPool, workspace_id: Uuid, user_id: Uuid, role: &str, invited_by: Uuid) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO workspace_members (workspace_id, user_id, role, invited_by) VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING"
    )
    .bind(workspace_id).bind(user_id).bind(role).bind(invited_by)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_members(pool: &PgPool, workspace_id: Uuid) -> sqlx::Result<Vec<WorkspaceMember>> {
    sqlx::query_as::<_, WorkspaceMember>(
        "SELECT * FROM workspace_members WHERE workspace_id = $1"
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await
}

pub async fn create_invitation(pool: &PgPool, workspace_id: Uuid, email: &str, invited_by: Uuid) -> sqlx::Result<Invitation> {
    sqlx::query_as::<_, Invitation>(
        "INSERT INTO invitations (workspace_id, email, invited_by) VALUES ($1, $2, $3) RETURNING *"
    )
    .bind(workspace_id).bind(email).bind(invited_by)
    .fetch_one(pool)
    .await
}

pub async fn find_pending_invitations(pool: &PgPool, email: &str) -> sqlx::Result<Vec<Invitation>> {
    sqlx::query_as::<_, Invitation>(
        "SELECT * FROM invitations WHERE email = $1 AND status = 'pending'"
    )
    .bind(email)
    .fetch_all(pool)
    .await
}

pub async fn accept_invitation(pool: &PgPool, invitation_id: Uuid) -> sqlx::Result<Invitation> {
    sqlx::query_as::<_, Invitation>(
        "UPDATE invitations SET status = 'accepted' WHERE id = $1 RETURNING *"
    )
    .bind(invitation_id)
    .fetch_one(pool)
    .await
}
