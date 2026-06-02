use sqlx::PgPool;
use uuid::Uuid;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Workspace member role with GitHub-style three-tier permissions.
/// Extensible: add new variants + update ALL + update DB CHECK constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Owner,
    Writer,
    Reader,
}

impl Role {
    /// All valid role strings (for validation against DB CHECK constraint).
    pub const ALL: &[&str] = &["owner", "writer", "reader"];

    /// Check if this role has management privileges (invite/remove/change_role/delete).
    pub fn can_manage(&self) -> bool {
        matches!(self, Role::Owner)
    }

    /// Check if this role can edit content.
    pub fn can_write(&self) -> bool {
        matches!(self, Role::Owner | Role::Writer)
    }
}

impl FromStr for Role {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "owner" => Ok(Role::Owner),
            "writer" => Ok(Role::Writer),
            "reader" => Ok(Role::Reader),
            _ => Err(format!("invalid role: {s}")),
        }
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Role::Owner => write!(f, "owner"),
            Role::Writer => write!(f, "writer"),
            Role::Reader => write!(f, "reader"),
        }
    }
}

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
    pub role: String,
    pub invited_by: Uuid,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn create(pool: &PgPool, name: &str, slug: &str, visibility: &str, created_by: Uuid) -> sqlx::Result<Workspace> {
    let mut tx = pool.begin().await.map_err(|e| { tracing::error!("DB begin tx error: {e}"); e })?;

    let ws = sqlx::query_as::<_, Workspace>(
        "INSERT INTO workspaces (name, slug, visibility, created_by) VALUES ($1, $2, $3, $4) RETURNING *"
    )
    .bind(name).bind(slug).bind(visibility).bind(created_by)
    .fetch_one(&mut *tx)
    .await.map_err(|e| { tracing::error!("DB error: {e}"); e })?;

    // Add creator as owner
    sqlx::query(
        "INSERT INTO workspace_members (workspace_id, user_id, role) VALUES ($1, $2, 'owner')"
    )
    .bind(ws.id).bind(created_by)
    .execute(&mut *tx)
    .await.map_err(|e| { tracing::error!("DB error: {e}"); e })?;

    tx.commit().await.map_err(|e| { tracing::error!("DB commit tx error: {e}"); e })?;

    Ok(ws)
}

pub async fn find_by_slug(pool: &PgPool, slug: &str) -> sqlx::Result<Option<Workspace>> {
    sqlx::query_as::<_, Workspace>("SELECT * FROM workspaces WHERE slug = $1")
        .bind(slug)
        .fetch_optional(pool)
        .await
        .map_err(|e| { tracing::error!("DB find workspace by slug failed: {e}"); e })
}

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<Workspace>> {
    sqlx::query_as::<_, Workspace>("SELECT * FROM workspaces WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| { tracing::error!("DB find workspace by id failed: {e}"); e })
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
    .map_err(|e| { tracing::error!("DB list workspaces for user failed: {e}"); e })
}

pub async fn is_member(pool: &PgPool, workspace_id: Uuid, user_id: Uuid) -> sqlx::Result<bool> {
    let row = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM workspace_members WHERE workspace_id = $1 AND user_id = $2"
    )
    .bind(workspace_id).bind(user_id)
    .fetch_one(pool)
    .await.map_err(|e| { tracing::error!("DB error: {e}"); e })?;
    Ok(row > 0)
}

pub async fn add_member(pool: &PgPool, workspace_id: Uuid, user_id: Uuid, role: &str, invited_by: Uuid) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO workspace_members (workspace_id, user_id, role, invited_by) VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING"
    )
    .bind(workspace_id).bind(user_id).bind(role).bind(invited_by)
    .execute(pool)
    .await.map_err(|e| { tracing::error!("DB error: {e}"); e })?;
    Ok(())
}

pub async fn list_members(pool: &PgPool, workspace_id: Uuid) -> sqlx::Result<Vec<WorkspaceMember>> {
    sqlx::query_as::<_, WorkspaceMember>(
        "SELECT * FROM workspace_members WHERE workspace_id = $1"
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await
    .map_err(|e| { tracing::error!("DB list members failed: {e}"); e })
}

pub async fn create_invitation(pool: &PgPool, workspace_id: Uuid, email: &str, role: &str, invited_by: Uuid) -> sqlx::Result<Invitation> {
    sqlx::query_as::<_, Invitation>(
        "INSERT INTO invitations (workspace_id, email, role, invited_by) VALUES ($1, $2, $3, $4) RETURNING *"
    )
    .bind(workspace_id).bind(email).bind(role).bind(invited_by)
    .fetch_one(pool)
    .await
    .map_err(|e| { tracing::error!("DB create invitation failed: {e}"); e })
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

pub async fn rename(pool: &PgPool, id: Uuid, new_name: &str) -> sqlx::Result<Workspace> {
    sqlx::query_as::<_, Workspace>(
        "UPDATE workspaces SET name = $2 WHERE id = $1 RETURNING *"
    )
    .bind(id).bind(new_name)
    .fetch_one(pool)
    .await
}

/// Get the role of a user in a workspace. Returns None if not a member.
pub async fn get_member_role(pool: &PgPool, workspace_id: Uuid, user_id: Uuid) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar::<_, String>(
        "SELECT role FROM workspace_members WHERE workspace_id = $1 AND user_id = $2"
    )
    .bind(workspace_id).bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| { tracing::error!("DB get_member_role failed: {e}"); e })
}

/// Remove a member from a workspace. Cannot remove owners. Returns true if deleted.
pub async fn remove_member(pool: &PgPool, workspace_id: Uuid, user_id: Uuid) -> sqlx::Result<bool> {
    let rows = sqlx::query(
        "DELETE FROM workspace_members WHERE workspace_id = $1 AND user_id = $2 AND role != 'owner'"
    )
    .bind(workspace_id).bind(user_id)
    .execute(pool)
    .await
    .map_err(|e| { tracing::error!("DB remove_member failed: {e}"); e })?;
    Ok(rows.rows_affected() > 0)
}

/// Change a member's role. Cannot change owner's role. Returns the new role string.
pub async fn change_member_role(
    pool: &PgPool, workspace_id: Uuid, user_id: Uuid, new_role: &str
) -> sqlx::Result<Option<String>> {
    let row = sqlx::query_scalar::<_, String>(
        "UPDATE workspace_members SET role = $3
         WHERE workspace_id = $1 AND user_id = $2 AND role != 'owner'
         RETURNING role"
    )
    .bind(workspace_id).bind(user_id).bind(new_role)
    .fetch_optional(pool)
    .await
    .map_err(|e| { tracing::error!("DB change_member_role failed: {e}"); e })?;
    Ok(row)
}

/// Find all pending invitations for a user.
/// Tries email match first, falls back to NULL-email handling.
pub async fn find_pending_invitations_for_user(
    pool: &PgPool, user_id: Uuid
) -> sqlx::Result<Vec<Invitation>> {
    // First get the user's email
    let email: Option<String> = sqlx::query_scalar(
        "SELECT email FROM users WHERE id = $1"
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| { tracing::error!("DB get user email failed: {e}"); e })?
    .flatten();

    match email {
        Some(ref e) if !e.is_empty() => {
            sqlx::query_as::<_, Invitation>(
                "SELECT i.* FROM invitations i WHERE i.email = $1 AND i.status = 'pending'"
            )
            .bind(e)
            .fetch_all(pool)
            .await
            .map_err(|e| { tracing::error!("DB find_pending_invitations_for_user failed: {e}"); e })
        }
        _ => Ok(vec![]) // No email → no email-based invitations
    }
}

/// Find an invitation by its ID.
pub async fn find_invitation_by_id(pool: &PgPool, invitation_id: Uuid) -> sqlx::Result<Option<Invitation>> {
    sqlx::query_as::<_, Invitation>("SELECT * FROM invitations WHERE id = $1")
        .bind(invitation_id)
        .fetch_optional(pool)
        .await
}

/// Reject an invitation (set status to 'rejected').
pub async fn reject_invitation(pool: &PgPool, invitation_id: Uuid) -> sqlx::Result<Invitation> {
    sqlx::query_as::<_, Invitation>(
        "UPDATE invitations SET status = 'rejected' WHERE id = $1 RETURNING *"
    )
    .bind(invitation_id)
    .fetch_one(pool)
    .await
}

/// Delete a workspace. Cascade handled by DB ON DELETE CASCADE.
pub async fn delete_workspace(pool: &PgPool, workspace_id: Uuid) -> sqlx::Result<bool> {
    let rows = sqlx::query("DELETE FROM workspaces WHERE id = $1")
        .bind(workspace_id)
        .execute(pool)
        .await
        .map_err(|e| { tracing::error!("DB delete_workspace failed: {e}"); e })?;
    Ok(rows.rows_affected() > 0)
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ═══════════════════════════════════════════════════════════════
    // Role Enum Unit Tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_role_from_str_valid() {
        assert_eq!("owner".parse::<Role>().unwrap(), Role::Owner);
        assert_eq!("writer".parse::<Role>().unwrap(), Role::Writer);
        assert_eq!("reader".parse::<Role>().unwrap(), Role::Reader);
        // case-insensitive
        assert_eq!("OWNER".parse::<Role>().unwrap(), Role::Owner);
        assert_eq!("Writer".parse::<Role>().unwrap(), Role::Writer);
        assert_eq!("Reader".parse::<Role>().unwrap(), Role::Reader);
    }

    #[test]
    fn test_role_from_str_invalid() {
        assert!("admin".parse::<Role>().is_err());
        assert!("member".parse::<Role>().is_err());
        assert!("".parse::<Role>().is_err());
        assert!("superadmin".parse::<Role>().is_err());
    }

    #[test]
    fn test_role_display() {
        assert_eq!(Role::Owner.to_string(), "owner");
        assert_eq!(Role::Writer.to_string(), "writer");
        assert_eq!(Role::Reader.to_string(), "reader");
    }

    #[test]
    fn test_role_can_manage() {
        assert!(Role::Owner.can_manage());
        assert!(!Role::Writer.can_manage());
        assert!(!Role::Reader.can_manage());
    }

    #[test]
    fn test_role_can_write() {
        assert!(Role::Owner.can_write());
        assert!(Role::Writer.can_write());
        assert!(!Role::Reader.can_write());
    }

    #[test]
    fn test_role_all_contains_all_variants() {
        assert_eq!(Role::ALL.len(), 3);
        assert!(Role::ALL.contains(&"owner"));
        assert!(Role::ALL.contains(&"writer"));
        assert!(Role::ALL.contains(&"reader"));
    }

    #[test]
    fn test_role_all_is_sorted_like_check_constraint() {
        // Must match the DB CHECK constraint order for validation
        assert_eq!(Role::ALL, &["owner", "writer", "reader"]);
    }

    #[test]
    fn test_role_roundtrip_parse_then_display() {
        for role_str in ["owner", "writer", "reader"] {
            let role: Role = role_str.parse().unwrap();
            assert_eq!(role.to_string(), role_str);
        }
    }

    #[test]
    fn test_role_copy_and_eq() {
        let a = Role::Owner;
        let b = a; // Copy
        assert_eq!(a, b);
        assert_ne!(Role::Owner, Role::Writer);
        assert_ne!(Role::Writer, Role::Reader);
        assert_ne!(Role::Reader, Role::Owner);
    }

    #[test]
    fn test_role_serde_roundtrip() {
        let json = serde_json::to_string(&Role::Owner).unwrap();
        assert_eq!(json, "\"owner\"");
        let role: Role = serde_json::from_str(&json).unwrap();
        assert_eq!(role, Role::Owner);

        let json = serde_json::to_string(&Role::Reader).unwrap();
        assert_eq!(json, "\"reader\"");
        let role: Role = serde_json::from_str(&json).unwrap();
        assert_eq!(role, Role::Reader);
    }

    // ═══════════════════════════════════════════════════════════════
    // Permission Matrix Logic Tests (no DB needed)
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_permission_matrix_owner_full_access() {
        assert!(Role::Owner.can_manage());
        assert!(Role::Owner.can_write());
    }

    #[test]
    fn test_permission_matrix_writer_limited() {
        assert!(Role::Writer.can_write());
        assert!(!Role::Writer.can_manage());
    }

    #[test]
    fn test_permission_matrix_reader_readonly() {
        assert!(!Role::Reader.can_write());
        assert!(!Role::Reader.can_manage());
    }

    #[test]
    fn test_role_validation_against_all() {
        // Simulates the invite endpoint's role validation
        let valid_roles = ["owner", "writer", "reader"];
        assert!(valid_roles.contains(&"owner"));
        assert!(valid_roles.contains(&"writer"));
        assert!(valid_roles.contains(&"reader"));
        assert!(!valid_roles.contains(&"admin"));
        assert!(!valid_roles.contains(&"member"));
        assert!(!valid_roles.contains(&""));
    }

    // ═══════════════════════════════════════════════════════════════
    // Database Integration Tests (require TEST_DATABASE_URL)
    // ═══════════════════════════════════════════════════════════════

    /// Create an in-memory-style test helper. Uses `TEST_DATABASE_URL` env var.
    /// Skips all DB tests if not set.
    async fn test_pool() -> Option<PgPool> {
        let url = std::env::var("TEST_DATABASE_URL").ok()?;
        let pool = PgPool::connect(&url).await.ok()?;
        // Run all migrations
        let sql1 = include_str!("migrations/001_init.sql").replace("__EMBEDDING_DIM__", "768");
        let _ = sqlx::raw_sql(&sql1).execute(&pool).await;
        let _ = sqlx::raw_sql(include_str!("migrations/002_workspaces.sql")).execute(&pool).await;
        let _ = sqlx::raw_sql(include_str!("migrations/003_workspace_visibility.sql")).execute(&pool).await;
        let _ = sqlx::raw_sql(include_str!("migrations/004_role_update.sql")).execute(&pool).await;
        let _ = sqlx::raw_sql(include_str!("migrations/005_fts.sql")).execute(&pool).await;
        let _ = sqlx::raw_sql(include_str!("migrations/006_api_keys.sql")).execute(&pool).await;
        let _ = sqlx::raw_sql(include_str!("migrations/007_team_permissions.sql")).execute(&pool).await;
        Some(pool)
    }

    /// Create test users and return (user_a_id, user_b_id, user_c_id)
    async fn create_test_users(pool: &PgPool) -> (Uuid, Uuid, Uuid) {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();
        let _ = sqlx::query(
            "INSERT INTO users (id, name, email) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING"
        )
        .bind(a).bind("Alice").bind("alice@test.com")
        .execute(pool).await;
        let _ = sqlx::query(
            "INSERT INTO users (id, name, email) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING"
        )
        .bind(b).bind("Bob").bind("bob@test.com")
        .execute(pool).await;
        let _ = sqlx::query(
            "INSERT INTO users (id, name, email) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING"
        )
        .bind(c).bind("Carol").bind("carol@test.com")
        .execute(pool).await;
        (a, b, c)
    }

    // ── Workspace + Member Tests ──────────────────────────────────

    #[tokio::test]
    async fn test_create_workspace_adds_creator_as_owner() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => { eprintln!("Skipping: TEST_DATABASE_URL not set"); return; }
        };
        let (user_a, _, _) = create_test_users(&pool).await;
        let slug = format!("test-ws-{}", Uuid::new_v4().to_string().split('-').next().unwrap());

        let ws = create(&pool, "Test WS", &slug, "public", user_a).await.unwrap();
        assert_eq!(ws.name, "Test WS");
        assert_eq!(ws.slug, slug);
        assert_eq!(ws.visibility, "public");
        assert_eq!(ws.created_by, user_a);

        let role = get_member_role(&pool, ws.id, user_a).await.unwrap();
        assert_eq!(role.as_deref(), Some("owner"));

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1").bind(ws.id).execute(&pool).await;
    }

    #[tokio::test]
    async fn test_get_member_role_returns_none_for_non_member() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => { eprintln!("Skipping: TEST_DATABASE_URL not set"); return; }
        };
        let (user_a, user_b, _) = create_test_users(&pool).await;
        let slug = format!("test-ws-{}", Uuid::new_v4().to_string().split('-').next().unwrap());

        let ws = create(&pool, "Test WS", &slug, "public", user_a).await.unwrap();
        let role = get_member_role(&pool, ws.id, user_b).await.unwrap();
        assert!(role.is_none());

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1").bind(ws.id).execute(&pool).await;
    }

    #[tokio::test]
    async fn test_add_member_with_role() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => { eprintln!("Skipping: TEST_DATABASE_URL not set"); return; }
        };
        let (user_a, user_b, _) = create_test_users(&pool).await;
        let slug = format!("test-ws-{}", Uuid::new_v4().to_string().split('-').next().unwrap());

        let ws = create(&pool, "Test WS", &slug, "public", user_a).await.unwrap();
        add_member(&pool, ws.id, user_b, "reader", user_a).await.unwrap();
        let role = get_member_role(&pool, ws.id, user_b).await.unwrap();
        assert_eq!(role.as_deref(), Some("reader"));

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1").bind(ws.id).execute(&pool).await;
    }

    #[tokio::test]
    async fn test_add_member_idempotent_no_override() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => { eprintln!("Skipping: TEST_DATABASE_URL not set"); return; }
        };
        let (user_a, user_b, _) = create_test_users(&pool).await;
        let slug = format!("test-ws-{}", Uuid::new_v4().to_string().split('-').next().unwrap());

        let ws = create(&pool, "Test WS", &slug, "public", user_a).await.unwrap();
        add_member(&pool, ws.id, user_b, "reader", user_a).await.unwrap();
        add_member(&pool, ws.id, user_b, "writer", user_a).await.unwrap();
        let role = get_member_role(&pool, ws.id, user_b).await.unwrap();
        assert_eq!(role.as_deref(), Some("reader"), "ON CONFLICT DO NOTHING preserves original role");

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1").bind(ws.id).execute(&pool).await;
    }

    // ── Remove Member Tests ───────────────────────────────────────

    #[tokio::test]
    async fn test_remove_member_success() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => { eprintln!("Skipping: TEST_DATABASE_URL not set"); return; }
        };
        let (user_a, user_b, _) = create_test_users(&pool).await;
        let slug = format!("test-ws-{}", Uuid::new_v4().to_string().split('-').next().unwrap());

        let ws = create(&pool, "Test WS", &slug, "public", user_a).await.unwrap();
        add_member(&pool, ws.id, user_b, "writer", user_a).await.unwrap();

        let removed = remove_member(&pool, ws.id, user_b).await.unwrap();
        assert!(removed);
        let role = get_member_role(&pool, ws.id, user_b).await.unwrap();
        assert!(role.is_none());

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1").bind(ws.id).execute(&pool).await;
    }

    #[tokio::test]
    async fn test_remove_member_cannot_remove_owner() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => { eprintln!("Skipping: TEST_DATABASE_URL not set"); return; }
        };
        let (user_a, _, _) = create_test_users(&pool).await;
        let slug = format!("test-ws-{}", Uuid::new_v4().to_string().split('-').next().unwrap());

        let ws = create(&pool, "Test WS", &slug, "public", user_a).await.unwrap();
        let removed = remove_member(&pool, ws.id, user_a).await.unwrap();
        assert!(!removed, "owner should not be removable");
        let role = get_member_role(&pool, ws.id, user_a).await.unwrap();
        assert_eq!(role.as_deref(), Some("owner"));

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1").bind(ws.id).execute(&pool).await;
    }

    #[tokio::test]
    async fn test_remove_member_nonexistent_returns_false() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => { eprintln!("Skipping: TEST_DATABASE_URL not set"); return; }
        };
        let (user_a, _, _) = create_test_users(&pool).await;
        let slug = format!("test-ws-{}", Uuid::new_v4().to_string().split('-').next().unwrap());

        let ws = create(&pool, "Test WS", &slug, "public", user_a).await.unwrap();
        let removed = remove_member(&pool, ws.id, Uuid::new_v4()).await.unwrap();
        assert!(!removed);

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1").bind(ws.id).execute(&pool).await;
    }

    // ── Change Role Tests ─────────────────────────────────────────

    #[tokio::test]
    async fn test_change_member_role_success() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => { eprintln!("Skipping: TEST_DATABASE_URL not set"); return; }
        };
        let (user_a, user_b, _) = create_test_users(&pool).await;
        let slug = format!("test-ws-{}", Uuid::new_v4().to_string().split('-').next().unwrap());

        let ws = create(&pool, "Test WS", &slug, "public", user_a).await.unwrap();
        add_member(&pool, ws.id, user_b, "reader", user_a).await.unwrap();

        let new_role = change_member_role(&pool, ws.id, user_b, "writer").await.unwrap();
        assert_eq!(new_role.as_deref(), Some("writer"));
        let role = get_member_role(&pool, ws.id, user_b).await.unwrap();
        assert_eq!(role.as_deref(), Some("writer"));

        let new_role = change_member_role(&pool, ws.id, user_b, "reader").await.unwrap();
        assert_eq!(new_role.as_deref(), Some("reader"));

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1").bind(ws.id).execute(&pool).await;
    }

    #[tokio::test]
    async fn test_change_member_role_cannot_change_owner() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => { eprintln!("Skipping: TEST_DATABASE_URL not set"); return; }
        };
        let (user_a, _, _) = create_test_users(&pool).await;
        let slug = format!("test-ws-{}", Uuid::new_v4().to_string().split('-').next().unwrap());

        let ws = create(&pool, "Test WS", &slug, "public", user_a).await.unwrap();
        let result = change_member_role(&pool, ws.id, user_a, "writer").await.unwrap();
        assert!(result.is_none(), "owner's role should not be changeable");
        let role = get_member_role(&pool, ws.id, user_a).await.unwrap();
        assert_eq!(role.as_deref(), Some("owner"));

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1").bind(ws.id).execute(&pool).await;
    }

    // ── Invitation Tests ──────────────────────────────────────────

    #[tokio::test]
    async fn test_create_invitation_with_role() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => { eprintln!("Skipping: TEST_DATABASE_URL not set"); return; }
        };
        let (user_a, _, _) = create_test_users(&pool).await;
        let slug = format!("test-ws-{}", Uuid::new_v4().to_string().split('-').next().unwrap());

        let ws = create(&pool, "Test WS", &slug, "public", user_a).await.unwrap();

        let inv = create_invitation(&pool, ws.id, "newuser@test.com", "reader", user_a).await.unwrap();
        assert_eq!(inv.email, "newuser@test.com");
        assert_eq!(inv.role, "reader");
        assert_eq!(inv.status, "pending");
        assert_eq!(inv.invited_by, user_a);

        let inv2 = create_invitation(&pool, ws.id, "newuser2@test.com", "writer", user_a).await.unwrap();
        assert_eq!(inv2.role, "writer");

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1").bind(ws.id).execute(&pool).await;
    }

    #[tokio::test]
    async fn test_accept_invitation_changes_status() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => { eprintln!("Skipping: TEST_DATABASE_URL not set"); return; }
        };
        let (user_a, _, _) = create_test_users(&pool).await;
        let slug = format!("test-ws-{}", Uuid::new_v4().to_string().split('-').next().unwrap());

        let ws = create(&pool, "Test WS", &slug, "public", user_a).await.unwrap();
        let inv = create_invitation(&pool, ws.id, "newuser@test.com", "writer", user_a).await.unwrap();

        assert_eq!(inv.status, "pending");
        let accepted = accept_invitation(&pool, inv.id).await.unwrap();
        assert_eq!(accepted.status, "accepted");

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1").bind(ws.id).execute(&pool).await;
    }

    #[tokio::test]
    async fn test_reject_invitation_changes_status() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => { eprintln!("Skipping: TEST_DATABASE_URL not set"); return; }
        };
        let (user_a, _, _) = create_test_users(&pool).await;
        let slug = format!("test-ws-{}", Uuid::new_v4().to_string().split('-').next().unwrap());

        let ws = create(&pool, "Test WS", &slug, "public", user_a).await.unwrap();
        let inv = create_invitation(&pool, ws.id, "newuser@test.com", "reader", user_a).await.unwrap();

        let rejected = reject_invitation(&pool, inv.id).await.unwrap();
        assert_eq!(rejected.status, "rejected");

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1").bind(ws.id).execute(&pool).await;
    }

    #[tokio::test]
    async fn test_find_invitation_by_id() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => { eprintln!("Skipping: TEST_DATABASE_URL not set"); return; }
        };
        let (user_a, _, _) = create_test_users(&pool).await;
        let slug = format!("test-ws-{}", Uuid::new_v4().to_string().split('-').next().unwrap());

        let ws = create(&pool, "Test WS", &slug, "public", user_a).await.unwrap();
        let inv = create_invitation(&pool, ws.id, "findme@test.com", "writer", user_a).await.unwrap();

        let found = find_invitation_by_id(&pool, inv.id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().email, "findme@test.com");

        let not_found = find_invitation_by_id(&pool, Uuid::new_v4()).await.unwrap();
        assert!(not_found.is_none());

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1").bind(ws.id).execute(&pool).await;
    }

    // ── Delete Workspace Test ─────────────────────────────────────

    #[tokio::test]
    async fn test_delete_workspace_success() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => { eprintln!("Skipping: TEST_DATABASE_URL not set"); return; }
        };
        let (user_a, _, _) = create_test_users(&pool).await;
        let slug = format!("test-ws-{}", Uuid::new_v4().to_string().split('-').next().unwrap());

        let ws = create(&pool, "Test WS", &slug, "public", user_a).await.unwrap();
        let deleted = delete_workspace(&pool, ws.id).await.unwrap();
        assert!(deleted);
        let found = find_by_slug(&pool, &slug).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_delete_workspace_nonexistent_returns_false() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => { eprintln!("Skipping: TEST_DATABASE_URL not set"); return; }
        };
        let deleted = delete_workspace(&pool, Uuid::new_v4()).await.unwrap();
        assert!(!deleted);
    }

    // ── Audit Log Tests ───────────────────────────────────────────

    #[tokio::test]
    async fn test_audit_log_insert() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => { eprintln!("Skipping: TEST_DATABASE_URL not set"); return; }
        };
        let (user_a, _, _) = create_test_users(&pool).await;
        let slug = format!("test-ws-{}", Uuid::new_v4().to_string().split('-').next().unwrap());

        let ws = create(&pool, "Test WS", &slug, "public", user_a).await.unwrap();

        crate::audit::log(
            &pool, ws.id, user_a,
            "invite_member", Some("invitation"), Some(Uuid::new_v4()),
            Some(serde_json::json!({"email": "test@test.com", "role": "writer"})),
        ).await.unwrap();

        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM audit_log WHERE workspace_id = $1 AND actor_id = $2"
        )
        .bind(ws.id).bind(user_a)
        .fetch_one(&pool).await.unwrap();
        assert!(count.0 >= 1, "audit log entry should exist");

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1").bind(ws.id).execute(&pool).await;
    }

    #[tokio::test]
    async fn test_audit_log_multiple_actions() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => { eprintln!("Skipping: TEST_DATABASE_URL not set"); return; }
        };
        let (user_a, _, _) = create_test_users(&pool).await;
        let slug = format!("test-ws-{}", Uuid::new_v4().to_string().split('-').next().unwrap());

        let ws = create(&pool, "Test WS", &slug, "public", user_a).await.unwrap();

        let actions = ["invite_member", "remove_member", "change_member_role", "delete_workspace"];
        for action in &actions {
            crate::audit::log(
                &pool, ws.id, user_a, action, Some("user"), Some(Uuid::new_v4()), None,
            ).await.unwrap();
        }

        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM audit_log WHERE workspace_id = $1"
        )
        .bind(ws.id)
        .fetch_one(&pool).await.unwrap();
        assert!(count.0 >= 4, "all audit log entries should exist");

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1").bind(ws.id).execute(&pool).await;
    }

    // ── Edge Cases ────────────────────────────────────────────────

    #[tokio::test]
    async fn test_is_member_returns_false_for_non_member() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => { eprintln!("Skipping: TEST_DATABASE_URL not set"); return; }
        };
        let (user_a, user_b, _) = create_test_users(&pool).await;
        let slug = format!("test-ws-{}", Uuid::new_v4().to_string().split('-').next().unwrap());

        let ws = create(&pool, "Test WS", &slug, "public", user_a).await.unwrap();
        assert!(is_member(&pool, ws.id, user_a).await.unwrap());
        assert!(!is_member(&pool, ws.id, user_b).await.unwrap());

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1").bind(ws.id).execute(&pool).await;
    }

    #[tokio::test]
    async fn test_list_members_includes_all_roles() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => { eprintln!("Skipping: TEST_DATABASE_URL not set"); return; }
        };
        let (user_a, user_b, user_c) = create_test_users(&pool).await;
        let slug = format!("test-ws-{}", Uuid::new_v4().to_string().split('-').next().unwrap());

        let ws = create(&pool, "Test WS", &slug, "public", user_a).await.unwrap();
        add_member(&pool, ws.id, user_b, "writer", user_a).await.unwrap();
        add_member(&pool, ws.id, user_c, "reader", user_a).await.unwrap();

        let members = list_members(&pool, ws.id).await.unwrap();
        assert_eq!(members.len(), 3);
        let roles: Vec<&str> = members.iter().map(|m| m.role.as_str()).collect();
        assert!(roles.contains(&"owner"));
        assert!(roles.contains(&"writer"));
        assert!(roles.contains(&"reader"));

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1").bind(ws.id).execute(&pool).await;
    }

    #[tokio::test]
    async fn test_find_pending_invitations_for_user() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => { eprintln!("Skipping: TEST_DATABASE_URL not set"); return; }
        };
        let (user_a, user_b, _) = create_test_users(&pool).await;
        let slug = format!("test-ws-{}", Uuid::new_v4().to_string().split('-').next().unwrap());

        let ws = create(&pool, "Test WS", &slug, "public", user_a).await.unwrap();
        create_invitation(&pool, ws.id, "bob@test.com", "writer", user_a).await.unwrap();

        let pending = find_pending_invitations_for_user(&pool, user_b).await.unwrap();
        assert!(!pending.is_empty(), "user_b should see the pending invitation");
        assert_eq!(pending[0].email, "bob@test.com");
        assert_eq!(pending[0].status, "pending");

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1").bind(ws.id).execute(&pool).await;
    }

    #[tokio::test]
    async fn test_create_workspace_private_visibility() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => { eprintln!("Skipping: TEST_DATABASE_URL not set"); return; }
        };
        let (user_a, _, _) = create_test_users(&pool).await;
        let slug = format!("test-ws-{}", Uuid::new_v4().to_string().split('-').next().unwrap());

        let ws = create(&pool, "Private WS", &slug, "private", user_a).await.unwrap();
        assert_eq!(ws.visibility, "private");
        assert_eq!(ws.name, "Private WS");

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1").bind(ws.id).execute(&pool).await;
    }

    #[tokio::test]
    async fn test_rename_workspace() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => { eprintln!("Skipping: TEST_DATABASE_URL not set"); return; }
        };
        let (user_a, _, _) = create_test_users(&pool).await;
        let slug = format!("test-ws-{}", Uuid::new_v4().to_string().split('-').next().unwrap());

        let ws = create(&pool, "Original Name", &slug, "public", user_a).await.unwrap();
        let updated = rename(&pool, ws.id, "Renamed WS").await.unwrap();
        assert_eq!(updated.name, "Renamed WS");
        assert_eq!(updated.slug, slug);

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1").bind(ws.id).execute(&pool).await;
    }
}
