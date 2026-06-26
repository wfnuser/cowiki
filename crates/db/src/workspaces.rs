use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::str::FromStr;
use uuid::Uuid;

/// Workspace member role with four-tier permissions.
/// Numeric order: Viewer(1) < Editor(2) < Manager(3) < Owner(4)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Viewer = 1,
    Editor = 2,
    Manager = 3,
    Owner = 4,
}

impl Role {
    pub const ALL: &[Role] = &[Role::Viewer, Role::Editor, Role::Manager, Role::Owner];

    pub fn can_manage(&self) -> bool {
        *self >= Role::Manager
    }
    pub fn can_edit(&self) -> bool {
        *self >= Role::Editor
    }
    pub fn can_view(&self) -> bool {
        *self >= Role::Viewer
    }
    pub fn can_delete_workspace(&self) -> bool {
        *self == Role::Owner
    }
    pub fn can_transfer_ownership(&self) -> bool {
        *self == Role::Owner
    }

    /// Higher role manages lower role (strictly greater).
    pub fn can_manage_role(&self, target: Role) -> bool {
        *self > target
    }
}

impl FromStr for Role {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "owner" => Ok(Role::Owner),
            "manager" => Ok(Role::Manager),
            "editor" => Ok(Role::Editor),
            "viewer" => Ok(Role::Viewer),
            _ => Err(format!("invalid role: {s}")),
        }
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Role::Owner => "owner",
                Role::Manager => "manager",
                Role::Editor => "editor",
                Role::Viewer => "viewer",
            }
        )
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
    pub invited_by: Option<Uuid>,
    pub joined_via: String,
    pub joined_at: chrono::DateTime<chrono::Utc>,
    pub last_active_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Invitation {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub email: String,
    pub role: String,
    pub invited_by: Uuid,
    pub invited_user_id: Option<Uuid>,
    pub status: String,
    pub message: Option<String>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub resent_count: i32,
    pub last_resent_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn create(
    pool: &PgPool,
    name: &str,
    slug: &str,
    visibility: &str,
    created_by: Uuid,
) -> sqlx::Result<Workspace> {
    let mut tx = pool.begin().await.map_err(|e| {
        tracing::error!("DB begin tx error: {e}");
        e
    })?;

    let ws = sqlx::query_as::<_, Workspace>(
        "INSERT INTO workspaces (name, slug, visibility, created_by) VALUES ($1, $2, $3, $4) RETURNING *"
    )
    .bind(name).bind(slug).bind(visibility).bind(created_by)
    .fetch_one(&mut *tx)
    .await.map_err(|e| { tracing::error!("DB error: {e}"); e })?;

    // Add creator as owner
    sqlx::query(
        "INSERT INTO workspace_members (workspace_id, user_id, role) VALUES ($1, $2, 'owner')",
    )
    .bind(ws.id)
    .bind(created_by)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!("DB error: {e}");
        e
    })?;

    tx.commit().await.map_err(|e| {
        tracing::error!("DB commit tx error: {e}");
        e
    })?;

    Ok(ws)
}

pub async fn find_by_slug(pool: &PgPool, slug: &str) -> sqlx::Result<Option<Workspace>> {
    sqlx::query_as::<_, Workspace>("SELECT * FROM workspaces WHERE slug = $1")
        .bind(slug)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            tracing::error!("DB find workspace by slug failed: {e}");
            e
        })
}

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<Workspace>> {
    sqlx::query_as::<_, Workspace>("SELECT * FROM workspaces WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            tracing::error!("DB find workspace by id failed: {e}");
            e
        })
}

pub async fn list_public(pool: &PgPool) -> sqlx::Result<Vec<Workspace>> {
    sqlx::query_as::<_, Workspace>(
        "SELECT * FROM workspaces WHERE visibility = 'public' ORDER BY created_at DESC",
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
        "SELECT COUNT(*) FROM workspace_members WHERE workspace_id = $1 AND user_id = $2",
    )
    .bind(workspace_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        tracing::error!("DB error: {e}");
        e
    })?;
    Ok(row > 0)
}

pub async fn add_member(
    pool: &PgPool,
    workspace_id: Uuid,
    user_id: Uuid,
    role: &str,
    invited_by: Uuid,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO workspace_members (workspace_id, user_id, role, invited_by, joined_via) VALUES ($1, $2, $3, $4, 'invitation') ON CONFLICT DO NOTHING"
    )
    .bind(workspace_id).bind(user_id).bind(role).bind(invited_by)
    .execute(pool)
    .await.map_err(|e| { tracing::error!("DB error: {e}"); e })?;
    Ok(())
}

pub async fn add_member_direct(
    pool: &PgPool,
    workspace_id: Uuid,
    user_id: Uuid,
    role: &str,
    invited_by: Uuid,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO workspace_members (workspace_id, user_id, role, invited_by, joined_via) VALUES ($1, $2, $3, $4, 'direct') ON CONFLICT DO NOTHING"
    )
    .bind(workspace_id).bind(user_id).bind(role).bind(invited_by)
    .execute(pool)
    .await.map_err(|e| { tracing::error!("DB error: {e}"); e })?;
    Ok(())
}

pub async fn add_member_public_join(
    pool: &PgPool,
    workspace_id: Uuid,
    user_id: Uuid,
    role: &str,
    invited_by: Uuid,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO workspace_members (workspace_id, user_id, role, invited_by, joined_via) VALUES ($1, $2, $3, $4, 'public_join') ON CONFLICT DO NOTHING"
    )
    .bind(workspace_id).bind(user_id).bind(role).bind(invited_by)
    .execute(pool)
    .await.map_err(|e| { tracing::error!("DB error: {e}"); e })?;
    Ok(())
}

pub async fn list_members(pool: &PgPool, workspace_id: Uuid) -> sqlx::Result<Vec<WorkspaceMember>> {
    sqlx::query_as::<_, WorkspaceMember>("SELECT * FROM workspace_members WHERE workspace_id = $1")
        .bind(workspace_id)
        .fetch_all(pool)
        .await
        .map_err(|e| {
            tracing::error!("DB list members failed: {e}");
            e
        })
}

pub async fn create_invitation(
    pool: &PgPool,
    workspace_id: Uuid,
    email: &str,
    role: &str,
    invited_by: Uuid,
    invited_user_id: Uuid,
) -> sqlx::Result<Invitation> {
    sqlx::query_as::<_, Invitation>(
        "INSERT INTO invitations (workspace_id, email, role, invited_by, invited_user_id) VALUES ($1, $2, $3, $4, $5) RETURNING *"
    )
    .bind(workspace_id).bind(email).bind(role).bind(invited_by).bind(invited_user_id)
    .fetch_one(pool)
    .await
    .map_err(|e| { tracing::error!("DB create invitation failed: {e}"); e })
}

pub async fn find_pending_invitations(pool: &PgPool, email: &str) -> sqlx::Result<Vec<Invitation>> {
    sqlx::query_as::<_, Invitation>(
        "SELECT * FROM invitations WHERE email = $1 AND status = 'pending'",
    )
    .bind(email)
    .fetch_all(pool)
    .await
}

pub async fn accept_invitation(pool: &PgPool, invitation_id: Uuid) -> sqlx::Result<Invitation> {
    sqlx::query_as::<_, Invitation>(
        "UPDATE invitations SET status = 'accepted' WHERE id = $1 RETURNING *",
    )
    .bind(invitation_id)
    .fetch_one(pool)
    .await
}

pub async fn rename(pool: &PgPool, id: Uuid, new_name: &str) -> sqlx::Result<Workspace> {
    sqlx::query_as::<_, Workspace>("UPDATE workspaces SET name = $2 WHERE id = $1 RETURNING *")
        .bind(id)
        .bind(new_name)
        .fetch_one(pool)
        .await
}

/// Get the role of a user in a workspace. Returns None if not a member.
pub async fn get_member_role(
    pool: &PgPool,
    workspace_id: Uuid,
    user_id: Uuid,
) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar::<_, String>(
        "SELECT role FROM workspace_members WHERE workspace_id = $1 AND user_id = $2",
    )
    .bind(workspace_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        tracing::error!("DB get_member_role failed: {e}");
        e
    })
}

/// Remove a member from a workspace. Caller must check permissions first.
pub async fn remove_member(pool: &PgPool, workspace_id: Uuid, user_id: Uuid) -> sqlx::Result<bool> {
    let rows =
        sqlx::query("DELETE FROM workspace_members WHERE workspace_id = $1 AND user_id = $2")
            .bind(workspace_id)
            .bind(user_id)
            .execute(pool)
            .await
            .map_err(|e| {
                tracing::error!("DB remove_member failed: {e}");
                e
            })?;
    Ok(rows.rows_affected() > 0)
}

/// Change a member's role. Caller must check permissions first.
pub async fn change_member_role(
    pool: &PgPool,
    workspace_id: Uuid,
    user_id: Uuid,
    new_role: &str,
) -> sqlx::Result<Option<String>> {
    let row = sqlx::query_scalar::<_, String>(
        "UPDATE workspace_members SET role = $3
         WHERE workspace_id = $1 AND user_id = $2
         RETURNING role",
    )
    .bind(workspace_id)
    .bind(user_id)
    .bind(new_role)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        tracing::error!("DB change_member_role failed: {e}");
        e
    })?;
    Ok(row)
}

/// Find all pending invitations for a user by invited_user_id.
pub async fn find_pending_invitations_for_user(
    pool: &PgPool,
    user_id: Uuid,
) -> sqlx::Result<Vec<Invitation>> {
    sqlx::query_as::<_, Invitation>(
        "SELECT i.* FROM invitations i WHERE i.invited_user_id = $1 AND i.status = 'pending'",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|e| {
        tracing::error!("DB find_pending_invitations_for_user failed: {e}");
        e
    })
}

/// Find an invitation by its ID.
pub async fn find_invitation_by_id(
    pool: &PgPool,
    invitation_id: Uuid,
) -> sqlx::Result<Option<Invitation>> {
    sqlx::query_as::<_, Invitation>("SELECT * FROM invitations WHERE id = $1")
        .bind(invitation_id)
        .fetch_optional(pool)
        .await
}

/// Reject an invitation (set status to 'rejected').
pub async fn reject_invitation(pool: &PgPool, invitation_id: Uuid) -> sqlx::Result<Invitation> {
    sqlx::query_as::<_, Invitation>(
        "UPDATE invitations SET status = 'rejected' WHERE id = $1 RETURNING *",
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
        .map_err(|e| {
            tracing::error!("DB delete_workspace failed: {e}");
            e
        })?;
    Ok(rows.rows_affected() > 0)
}

// ── New Helper Functions ───────────────────────────────────────────

pub async fn resolve_user_identifier(
    pool: &PgPool,
    identifier: &str,
) -> sqlx::Result<Option<Uuid>> {
    if let Ok(id) = Uuid::parse_str(identifier) {
        let row = sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?;
        if row.is_some() {
            return Ok(row);
        }
    }
    let row = sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE email = $1")
        .bind(identifier)
        .fetch_optional(pool)
        .await?;
    if row.is_some() {
        return Ok(row);
    }
    sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE name = $1")
        .bind(identifier)
        .fetch_optional(pool)
        .await
}

pub async fn find_invitations_by_workspace(
    pool: &PgPool,
    workspace_id: Uuid,
) -> sqlx::Result<Vec<Invitation>> {
    sqlx::query_as::<_, Invitation>(
        "SELECT * FROM invitations WHERE workspace_id = $1 ORDER BY created_at DESC",
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await
    .map_err(|e| {
        tracing::error!("DB find_invitations_by_workspace failed: {e}");
        e
    })
}

pub async fn resend_invitation(pool: &PgPool, invitation_id: Uuid) -> sqlx::Result<Invitation> {
    sqlx::query_as::<_, Invitation>(
        "UPDATE invitations SET resent_count = resent_count + 1, last_resent_at = now(), expires_at = now() + INTERVAL '7 days' WHERE id = $1 AND status = 'pending' RETURNING *"
    )
    .bind(invitation_id).fetch_one(pool).await
    .map_err(|e| { tracing::error!("DB resend_invitation failed: {e}"); e })
}

pub async fn revoke_invitation(pool: &PgPool, invitation_id: Uuid) -> sqlx::Result<Invitation> {
    sqlx::query_as::<_, Invitation>(
        "UPDATE invitations SET status = 'expired' WHERE id = $1 AND status = 'pending' RETURNING *"
    )
    .bind(invitation_id).fetch_one(pool).await
    .map_err(|e| { tracing::error!("DB revoke_invitation failed: {e}"); e })
}

pub async fn expire_stale_invitations(pool: &PgPool) -> sqlx::Result<u64> {
    let rows = sqlx::query(
        "UPDATE invitations SET status = 'expired' WHERE status = 'pending' AND expires_at < now()",
    )
    .execute(pool)
    .await
    .map_err(|e| {
        tracing::error!("DB expire_stale_invitations failed: {e}");
        e
    })?;
    Ok(rows.rows_affected())
}

pub async fn touch_last_active(
    pool: &PgPool,
    workspace_id: Uuid,
    user_id: Uuid,
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE workspace_members SET last_active_at = now() WHERE workspace_id = $1 AND user_id = $2"
    )
    .bind(workspace_id).bind(user_id).execute(pool).await
    .map_err(|e| { tracing::error!("DB touch_last_active failed: {e}"); e })?;
    Ok(())
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
        assert_eq!("manager".parse::<Role>().unwrap(), Role::Manager);
        assert_eq!("editor".parse::<Role>().unwrap(), Role::Editor);
        assert_eq!("viewer".parse::<Role>().unwrap(), Role::Viewer);
        // case-insensitive
        assert_eq!("OWNER".parse::<Role>().unwrap(), Role::Owner);
        assert_eq!("Manager".parse::<Role>().unwrap(), Role::Manager);
        assert_eq!("Editor".parse::<Role>().unwrap(), Role::Editor);
        assert_eq!("VIEWER".parse::<Role>().unwrap(), Role::Viewer);
    }

    #[test]
    fn test_role_from_str_invalid() {
        assert!("admin".parse::<Role>().is_err());
        assert!("member".parse::<Role>().is_err());
        assert!("writer".parse::<Role>().is_err());
        assert!("reader".parse::<Role>().is_err());
        assert!("".parse::<Role>().is_err());
    }

    #[test]
    fn test_role_display() {
        assert_eq!(Role::Owner.to_string(), "owner");
        assert_eq!(Role::Manager.to_string(), "manager");
        assert_eq!(Role::Editor.to_string(), "editor");
        assert_eq!(Role::Viewer.to_string(), "viewer");
    }

    #[test]
    fn test_role_can_manage() {
        assert!(Role::Owner.can_manage());
        assert!(!Role::Editor.can_manage());
        assert!(!Role::Viewer.can_manage());
    }

    #[test]
    fn test_role_can_edit() {
        assert!(Role::Owner.can_edit());
        assert!(Role::Editor.can_edit());
        assert!(!Role::Viewer.can_edit());
    }

    #[test]
    fn test_role_all_contains_all_variants() {
        assert_eq!(Role::ALL.len(), 4);
        assert!(Role::ALL.contains(&Role::Owner));
        assert!(Role::ALL.contains(&Role::Manager));
        assert!(Role::ALL.contains(&Role::Editor));
        assert!(Role::ALL.contains(&Role::Viewer));
    }

    #[test]
    fn test_role_roundtrip_parse_then_display() {
        for role_str in ["owner", "manager", "editor", "viewer"] {
            let role: Role = role_str.parse().unwrap();
            assert_eq!(role.to_string(), role_str);
        }
    }

    #[test]
    fn test_role_copy_and_eq() {
        let a = Role::Owner;
        let b = a; // Copy
        assert_eq!(a, b);
        assert_ne!(Role::Owner, Role::Editor);
        assert_ne!(Role::Editor, Role::Viewer);
        assert_ne!(Role::Viewer, Role::Owner);
    }

    #[test]
    fn test_role_serde_roundtrip() {
        let json = serde_json::to_string(&Role::Owner).unwrap();
        assert_eq!(json, "\"owner\"");
        let role: Role = serde_json::from_str(&json).unwrap();
        assert_eq!(role, Role::Owner);

        let json = serde_json::to_string(&Role::Viewer).unwrap();
        assert_eq!(json, "\"viewer\"");
        let role: Role = serde_json::from_str(&json).unwrap();
        assert_eq!(role, Role::Viewer);
    }

    // ═══════════════════════════════════════════════════════════════
    // Permission Matrix Logic Tests (no DB needed)
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_permission_matrix_owner_full_access() {
        assert!(Role::Owner.can_manage());
        assert!(Role::Owner.can_edit());
    }

    #[test]
    fn test_permission_matrix_writer_limited() {
        assert!(Role::Editor.can_edit());
        assert!(!Role::Editor.can_manage());
    }

    #[test]
    fn test_permission_matrix_reader_readonly() {
        assert!(!Role::Viewer.can_edit());
        assert!(!Role::Viewer.can_manage());
    }

    #[test]
    fn test_role_validation_against_all() {
        // Simulates the invite endpoint's role validation
        let valid_roles = ["owner", "editor", "viewer"];
        assert!(valid_roles.contains(&"owner"));
        assert!(valid_roles.contains(&"editor"));
        assert!(valid_roles.contains(&"viewer"));
        assert!(!valid_roles.contains(&"admin"));
        assert!(!valid_roles.contains(&"member"));
        assert!(!valid_roles.contains(&""));
    }

    // ═══════════════════════════════════════════════════════════════
    // Role Hierarchy & Ordering Tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_role_ordering() {
        // Viewer(1) < Editor(2) < Manager(3) < Owner(4)
        assert!(Role::Viewer < Role::Editor);
        assert!(Role::Editor < Role::Manager);
        assert!(Role::Manager < Role::Owner);
        assert!(Role::Viewer < Role::Owner);
        assert!(!(Role::Owner < Role::Viewer));
        // >= is used for permission checks
        assert!(Role::Owner >= Role::Owner);
        assert!(Role::Owner >= Role::Manager);
        assert!(Role::Manager >= Role::Editor);
        assert!(Role::Editor >= Role::Viewer);
        assert!(!(Role::Viewer >= Role::Editor));
    }

    #[test]
    fn test_role_numeric_discriminants() {
        assert_eq!(Role::Viewer as u8, 1);
        assert_eq!(Role::Editor as u8, 2);
        assert_eq!(Role::Manager as u8, 3);
        assert_eq!(Role::Owner as u8, 4);
    }

    #[test]
    fn test_role_can_view() {
        assert!(Role::Owner.can_view());
        assert!(Role::Manager.can_view());
        assert!(Role::Editor.can_view());
        assert!(Role::Viewer.can_view());
    }

    #[test]
    fn test_role_can_edit_all_roles() {
        assert!(Role::Owner.can_edit());
        assert!(Role::Manager.can_edit());
        assert!(Role::Editor.can_edit());
        assert!(!Role::Viewer.can_edit());
    }

    #[test]
    fn test_role_can_manage_all_roles() {
        assert!(Role::Owner.can_manage());
        assert!(Role::Manager.can_manage());
        assert!(!Role::Editor.can_manage());
        assert!(!Role::Viewer.can_manage());
    }

    #[test]
    fn test_role_can_delete_workspace() {
        assert!(Role::Owner.can_delete_workspace());
        assert!(!Role::Manager.can_delete_workspace());
        assert!(!Role::Editor.can_delete_workspace());
        assert!(!Role::Viewer.can_delete_workspace());
    }

    #[test]
    fn test_role_can_transfer_ownership() {
        assert!(Role::Owner.can_transfer_ownership());
        assert!(!Role::Manager.can_transfer_ownership());
        assert!(!Role::Editor.can_transfer_ownership());
        assert!(!Role::Viewer.can_transfer_ownership());
    }

    #[test]
    fn test_role_can_manage_role_matrix() {
        // can_manage_role uses strict greater-than: self > target
        let all = [Role::Viewer, Role::Editor, Role::Manager, Role::Owner];

        // Owner can manage everyone below
        assert!(Role::Owner.can_manage_role(Role::Manager));
        assert!(Role::Owner.can_manage_role(Role::Editor));
        assert!(Role::Owner.can_manage_role(Role::Viewer));
        assert!(!Role::Owner.can_manage_role(Role::Owner)); // not strictly greater

        // Manager can manage Editor and Viewer
        assert!(Role::Manager.can_manage_role(Role::Editor));
        assert!(Role::Manager.can_manage_role(Role::Viewer));
        assert!(!Role::Manager.can_manage_role(Role::Manager));
        assert!(!Role::Manager.can_manage_role(Role::Owner));

        // Editor can manage Viewer only
        assert!(Role::Editor.can_manage_role(Role::Viewer));
        assert!(!Role::Editor.can_manage_role(Role::Editor));
        assert!(!Role::Editor.can_manage_role(Role::Manager));
        assert!(!Role::Editor.can_manage_role(Role::Owner));

        // Viewer cannot manage anyone
        for role in &all {
            assert!(!Role::Viewer.can_manage_role(*role));
        }

        // Verify symmetry: if A can manage B, then B cannot manage A
        for &a in &all {
            for &b in &all {
                if a != b {
                    assert!(
                        a.can_manage_role(b) != b.can_manage_role(a) || !a.can_manage_role(b),
                        "asymmetric: {:?} vs {:?}",
                        a,
                        b
                    );
                }
            }
        }
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
        let _ = sqlx::raw_sql(include_str!("migrations/002_workspaces.sql"))
            .execute(&pool)
            .await;
        let _ = sqlx::raw_sql(include_str!("migrations/003_workspace_visibility.sql"))
            .execute(&pool)
            .await;
        let _ = sqlx::raw_sql(include_str!("migrations/004_role_update.sql"))
            .execute(&pool)
            .await;
        let _ = sqlx::raw_sql(include_str!("migrations/005_fts.sql"))
            .execute(&pool)
            .await;
        let _ = sqlx::raw_sql(include_str!("migrations/006_api_keys.sql"))
            .execute(&pool)
            .await;
        let _ = sqlx::raw_sql(include_str!("migrations/007_team_permissions.sql"))
            .execute(&pool)
            .await;
        let _ = sqlx::raw_sql(include_str!("migrations/008_submission_workspace.sql"))
            .execute(&pool)
            .await;
        let _ = sqlx::raw_sql(include_str!("migrations/009_role_management.sql"))
            .execute(&pool)
            .await;
        let _ = sqlx::raw_sql(include_str!("migrations/009_review_comments.sql"))
            .execute(&pool)
            .await;
        let _ = sqlx::raw_sql(include_str!("migrations/010_invitation_reject_status.sql"))
            .execute(&pool)
            .await;
        let _ = sqlx::raw_sql(include_str!("migrations/010_notifications.sql"))
            .execute(&pool)
            .await;
        let _ = sqlx::raw_sql(include_str!("migrations/011_pages_workspace_scope.sql"))
            .execute(&pool)
            .await;
        let _ = sqlx::raw_sql(include_str!("migrations/012_hash_primary_api_keys.sql"))
            .execute(&pool)
            .await;
        Some(pool)
    }

    /// Create test users and return (user_a_id, user_b_id, user_c_id).
    /// Uses stable names/emails (some tests match invitations by email) but
    /// supplies the required NOT NULL api_key and returns the existing row's id
    /// on conflict, so the helper is safe to call repeatedly against a shared DB.
    async fn create_test_users(pool: &PgPool) -> (Uuid, Uuid, Uuid) {
        async fn upsert(pool: &PgPool, name: &str, email: &str) -> Uuid {
            let id = Uuid::new_v4();
            sqlx::query_scalar::<_, Uuid>(
                "INSERT INTO users (id, name, email, api_key) VALUES ($1, $2, $3, $4) \
                 ON CONFLICT (name) DO UPDATE SET email = EXCLUDED.email RETURNING id",
            )
            .bind(id)
            .bind(name)
            .bind(email)
            .bind(format!("key-{id}"))
            .fetch_one(pool)
            .await
            .expect("seed test user")
        }
        let a = upsert(pool, "Alice", "alice@test.com").await;
        let b = upsert(pool, "Bob", "bob@test.com").await;
        let c = upsert(pool, "Carol", "carol@test.com").await;
        (a, b, c)
    }

    // ── Workspace + Member Tests ──────────────────────────────────

    #[tokio::test]
    async fn test_create_workspace_adds_creator_as_owner() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let (user_a, user_b, _) = create_test_users(&pool).await;
        let slug = format!(
            "test-ws-{}",
            Uuid::new_v4().to_string().split('-').next().unwrap()
        );

        let ws = create(&pool, "Test WS", &slug, "public", user_a)
            .await
            .unwrap();
        assert_eq!(ws.name, "Test WS");
        assert_eq!(ws.slug, slug);
        assert_eq!(ws.visibility, "public");
        assert_eq!(ws.created_by, user_a);

        let role = get_member_role(&pool, ws.id, user_a).await.unwrap();
        assert_eq!(role.as_deref(), Some("owner"));

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(ws.id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    async fn test_get_member_role_returns_none_for_non_member() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let (user_a, user_b, _) = create_test_users(&pool).await;
        let slug = format!(
            "test-ws-{}",
            Uuid::new_v4().to_string().split('-').next().unwrap()
        );

        let ws = create(&pool, "Test WS", &slug, "public", user_a)
            .await
            .unwrap();
        let role = get_member_role(&pool, ws.id, user_b).await.unwrap();
        assert!(role.is_none());

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(ws.id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    async fn test_add_member_with_role() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let (user_a, user_b, _) = create_test_users(&pool).await;
        let slug = format!(
            "test-ws-{}",
            Uuid::new_v4().to_string().split('-').next().unwrap()
        );

        let ws = create(&pool, "Test WS", &slug, "public", user_a)
            .await
            .unwrap();
        add_member(&pool, ws.id, user_b, "viewer", user_a)
            .await
            .unwrap();
        let role = get_member_role(&pool, ws.id, user_b).await.unwrap();
        assert_eq!(role.as_deref(), Some("viewer"));

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(ws.id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    async fn test_add_member_idempotent_no_override() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let (user_a, user_b, _) = create_test_users(&pool).await;
        let slug = format!(
            "test-ws-{}",
            Uuid::new_v4().to_string().split('-').next().unwrap()
        );

        let ws = create(&pool, "Test WS", &slug, "public", user_a)
            .await
            .unwrap();
        add_member(&pool, ws.id, user_b, "viewer", user_a)
            .await
            .unwrap();
        add_member(&pool, ws.id, user_b, "editor", user_a)
            .await
            .unwrap();
        let role = get_member_role(&pool, ws.id, user_b).await.unwrap();
        assert_eq!(
            role.as_deref(),
            Some("viewer"),
            "ON CONFLICT DO NOTHING preserves original role"
        );

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(ws.id)
            .execute(&pool)
            .await;
    }

    // ── Remove Member Tests ───────────────────────────────────────

    #[tokio::test]
    async fn test_remove_member_success() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let (user_a, user_b, _) = create_test_users(&pool).await;
        let slug = format!(
            "test-ws-{}",
            Uuid::new_v4().to_string().split('-').next().unwrap()
        );

        let ws = create(&pool, "Test WS", &slug, "public", user_a)
            .await
            .unwrap();
        add_member(&pool, ws.id, user_b, "editor", user_a)
            .await
            .unwrap();

        let removed = remove_member(&pool, ws.id, user_b).await.unwrap();
        assert!(removed);
        let role = get_member_role(&pool, ws.id, user_b).await.unwrap();
        assert!(role.is_none());

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(ws.id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    async fn test_remove_member_allows_owner_removal() {
        // Owner removal is now enforced at the route/application layer (PermissionGuard).
        // The DB function intentionally allows it — verification happens before calling.
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let (user_a, user_b, _) = create_test_users(&pool).await;
        let slug = format!(
            "test-ws-{}",
            Uuid::new_v4().to_string().split('-').next().unwrap()
        );

        let ws = create(&pool, "Test WS", &slug, "public", user_a)
            .await
            .unwrap();
        let removed = remove_member(&pool, ws.id, user_a).await.unwrap();
        assert!(removed, "DB allows owner removal; guard is at app layer");

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(ws.id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    async fn test_remove_member_nonexistent_returns_false() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let (user_a, user_b, _) = create_test_users(&pool).await;
        let slug = format!(
            "test-ws-{}",
            Uuid::new_v4().to_string().split('-').next().unwrap()
        );

        let ws = create(&pool, "Test WS", &slug, "public", user_a)
            .await
            .unwrap();
        let removed = remove_member(&pool, ws.id, Uuid::new_v4()).await.unwrap();
        assert!(!removed);

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(ws.id)
            .execute(&pool)
            .await;
    }

    // ── Change Role Tests ─────────────────────────────────────────

    #[tokio::test]
    async fn test_change_member_role_success() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let (user_a, user_b, _) = create_test_users(&pool).await;
        let slug = format!(
            "test-ws-{}",
            Uuid::new_v4().to_string().split('-').next().unwrap()
        );

        let ws = create(&pool, "Test WS", &slug, "public", user_a)
            .await
            .unwrap();
        add_member(&pool, ws.id, user_b, "viewer", user_a)
            .await
            .unwrap();

        let new_role = change_member_role(&pool, ws.id, user_b, "editor")
            .await
            .unwrap();
        assert_eq!(new_role.as_deref(), Some("editor"));
        let role = get_member_role(&pool, ws.id, user_b).await.unwrap();
        assert_eq!(role.as_deref(), Some("editor"));

        let new_role = change_member_role(&pool, ws.id, user_b, "viewer")
            .await
            .unwrap();
        assert_eq!(new_role.as_deref(), Some("viewer"));

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(ws.id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    async fn test_change_member_role_allows_owner_change() {
        // Owner role change is now enforced at the route/application layer (PermissionGuard).
        // The DB function intentionally allows it — verification happens before calling.
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let (user_a, user_b, _) = create_test_users(&pool).await;
        let slug = format!(
            "test-ws-{}",
            Uuid::new_v4().to_string().split('-').next().unwrap()
        );

        let ws = create(&pool, "Test WS", &slug, "public", user_a)
            .await
            .unwrap();
        let result = change_member_role(&pool, ws.id, user_a, "editor")
            .await
            .unwrap();
        assert!(
            result.is_some(),
            "DB allows owner role change; guard is at app layer"
        );
        let role = get_member_role(&pool, ws.id, user_a).await.unwrap();
        assert_eq!(role.as_deref(), Some("editor"));

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(ws.id)
            .execute(&pool)
            .await;
    }

    // ── Invitation Tests ──────────────────────────────────────────

    #[tokio::test]
    async fn test_create_invitation_with_role() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let (user_a, user_b, _) = create_test_users(&pool).await;
        let slug = format!(
            "test-ws-{}",
            Uuid::new_v4().to_string().split('-').next().unwrap()
        );

        let ws = create(&pool, "Test WS", &slug, "public", user_a)
            .await
            .unwrap();

        let inv = create_invitation(&pool, ws.id, "newuser@test.com", "viewer", user_a, user_b)
            .await
            .unwrap();
        assert_eq!(inv.email, "newuser@test.com");
        assert_eq!(inv.role, "viewer");
        assert_eq!(inv.status, "pending");
        assert_eq!(inv.invited_by, user_a);

        let inv2 = create_invitation(&pool, ws.id, "newuser2@test.com", "editor", user_a, user_b)
            .await
            .unwrap();
        assert_eq!(inv2.role, "editor");

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(ws.id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    async fn test_accept_invitation_changes_status() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let (user_a, user_b, _) = create_test_users(&pool).await;
        let slug = format!(
            "test-ws-{}",
            Uuid::new_v4().to_string().split('-').next().unwrap()
        );

        let ws = create(&pool, "Test WS", &slug, "public", user_a)
            .await
            .unwrap();
        let inv = create_invitation(&pool, ws.id, "newuser@test.com", "editor", user_a, user_b)
            .await
            .unwrap();

        assert_eq!(inv.status, "pending");
        let accepted = accept_invitation(&pool, inv.id).await.unwrap();
        assert_eq!(accepted.status, "accepted");

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(ws.id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    async fn test_reject_invitation_changes_status() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let (user_a, user_b, _) = create_test_users(&pool).await;
        let slug = format!(
            "test-ws-{}",
            Uuid::new_v4().to_string().split('-').next().unwrap()
        );

        let ws = create(&pool, "Test WS", &slug, "public", user_a)
            .await
            .unwrap();
        let inv = create_invitation(&pool, ws.id, "newuser@test.com", "viewer", user_a, user_b)
            .await
            .unwrap();

        let rejected = reject_invitation(&pool, inv.id).await.unwrap();
        assert_eq!(rejected.status, "rejected");

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(ws.id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    async fn test_find_invitation_by_id() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let (user_a, user_b, _) = create_test_users(&pool).await;
        let slug = format!(
            "test-ws-{}",
            Uuid::new_v4().to_string().split('-').next().unwrap()
        );

        let ws = create(&pool, "Test WS", &slug, "public", user_a)
            .await
            .unwrap();
        let inv = create_invitation(&pool, ws.id, "findme@test.com", "editor", user_a, user_b)
            .await
            .unwrap();

        let found = find_invitation_by_id(&pool, inv.id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().email, "findme@test.com");

        let not_found = find_invitation_by_id(&pool, Uuid::new_v4()).await.unwrap();
        assert!(not_found.is_none());

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(ws.id)
            .execute(&pool)
            .await;
    }

    // ── Delete Workspace Test ─────────────────────────────────────

    #[tokio::test]
    async fn test_delete_workspace_success() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let (user_a, user_b, _) = create_test_users(&pool).await;
        let slug = format!(
            "test-ws-{}",
            Uuid::new_v4().to_string().split('-').next().unwrap()
        );

        let ws = create(&pool, "Test WS", &slug, "public", user_a)
            .await
            .unwrap();
        let deleted = delete_workspace(&pool, ws.id).await.unwrap();
        assert!(deleted);
        let found = find_by_slug(&pool, &slug).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_delete_workspace_nonexistent_returns_false() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let deleted = delete_workspace(&pool, Uuid::new_v4()).await.unwrap();
        assert!(!deleted);
    }

    // ── Audit Log Tests ───────────────────────────────────────────

    #[tokio::test]
    async fn test_audit_log_insert() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let (user_a, user_b, _) = create_test_users(&pool).await;
        let slug = format!(
            "test-ws-{}",
            Uuid::new_v4().to_string().split('-').next().unwrap()
        );

        let ws = create(&pool, "Test WS", &slug, "public", user_a)
            .await
            .unwrap();

        crate::audit::log(
            &pool,
            ws.id,
            user_a,
            "invite_member",
            Some("invitation"),
            Some(Uuid::new_v4()),
            Some(serde_json::json!({"email": "test@test.com", "role": "editor"})),
        )
        .await
        .unwrap();

        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM audit_log WHERE workspace_id = $1 AND actor_id = $2",
        )
        .bind(ws.id)
        .bind(user_a)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(count.0 >= 1, "audit log entry should exist");

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(ws.id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    async fn test_audit_log_multiple_actions() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let (user_a, user_b, _) = create_test_users(&pool).await;
        let slug = format!(
            "test-ws-{}",
            Uuid::new_v4().to_string().split('-').next().unwrap()
        );

        let ws = create(&pool, "Test WS", &slug, "public", user_a)
            .await
            .unwrap();

        let actions = [
            "invite_member",
            "remove_member",
            "change_member_role",
            "delete_workspace",
        ];
        for action in &actions {
            crate::audit::log(
                &pool,
                ws.id,
                user_a,
                action,
                Some("user"),
                Some(Uuid::new_v4()),
                None,
            )
            .await
            .unwrap();
        }

        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM audit_log WHERE workspace_id = $1")
                .bind(ws.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(count.0 >= 4, "all audit log entries should exist");

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(ws.id)
            .execute(&pool)
            .await;
    }

    // ── Edge Cases ────────────────────────────────────────────────

    #[tokio::test]
    async fn test_is_member_returns_false_for_non_member() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let (user_a, user_b, _) = create_test_users(&pool).await;
        let slug = format!(
            "test-ws-{}",
            Uuid::new_v4().to_string().split('-').next().unwrap()
        );

        let ws = create(&pool, "Test WS", &slug, "public", user_a)
            .await
            .unwrap();
        assert!(is_member(&pool, ws.id, user_a).await.unwrap());
        assert!(!is_member(&pool, ws.id, user_b).await.unwrap());

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(ws.id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    async fn test_list_members_includes_all_roles() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let (user_a, user_b, user_c) = create_test_users(&pool).await;
        let slug = format!(
            "test-ws-{}",
            Uuid::new_v4().to_string().split('-').next().unwrap()
        );

        let ws = create(&pool, "Test WS", &slug, "public", user_a)
            .await
            .unwrap();
        add_member(&pool, ws.id, user_b, "editor", user_a)
            .await
            .unwrap();
        add_member(&pool, ws.id, user_c, "viewer", user_a)
            .await
            .unwrap();

        let members = list_members(&pool, ws.id).await.unwrap();
        assert_eq!(members.len(), 3);
        let roles: Vec<&str> = members.iter().map(|m| m.role.as_str()).collect();
        assert!(roles.contains(&"owner"));
        assert!(roles.contains(&"editor"));
        assert!(roles.contains(&"viewer"));

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(ws.id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    async fn test_find_pending_invitations_for_user() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let (user_a, user_b, _) = create_test_users(&pool).await;
        let slug = format!(
            "test-ws-{}",
            Uuid::new_v4().to_string().split('-').next().unwrap()
        );

        let ws = create(&pool, "Test WS", &slug, "public", user_a)
            .await
            .unwrap();
        create_invitation(&pool, ws.id, "bob@test.com", "editor", user_a, user_b)
            .await
            .unwrap();

        let pending = find_pending_invitations_for_user(&pool, user_b)
            .await
            .unwrap();
        assert!(
            !pending.is_empty(),
            "user_b should see the pending invitation"
        );
        assert_eq!(pending[0].email, "bob@test.com");
        assert_eq!(pending[0].status, "pending");

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(ws.id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    async fn test_create_workspace_private_visibility() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let (user_a, user_b, _) = create_test_users(&pool).await;
        let slug = format!(
            "test-ws-{}",
            Uuid::new_v4().to_string().split('-').next().unwrap()
        );

        let ws = create(&pool, "Private WS", &slug, "private", user_a)
            .await
            .unwrap();
        assert_eq!(ws.visibility, "private");
        assert_eq!(ws.name, "Private WS");

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(ws.id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    async fn test_rename_workspace() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let (user_a, user_b, _) = create_test_users(&pool).await;
        let slug = format!(
            "test-ws-{}",
            Uuid::new_v4().to_string().split('-').next().unwrap()
        );

        let ws = create(&pool, "Original Name", &slug, "public", user_a)
            .await
            .unwrap();
        let updated = rename(&pool, ws.id, "Renamed WS").await.unwrap();
        assert_eq!(updated.name, "Renamed WS");
        assert_eq!(updated.slug, slug);

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(ws.id)
            .execute(&pool)
            .await;
    }

    // ═══════════════════════════════════════════════════════════════
    // joined_via Tests
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_add_member_joined_via_invitation() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let (user_a, user_b, _) = create_test_users(&pool).await;
        let slug = format!(
            "test-ws-{}",
            Uuid::new_v4().to_string().split('-').next().unwrap()
        );
        let ws = create(&pool, "Test WS", &slug, "public", user_a)
            .await
            .unwrap();

        add_member(&pool, ws.id, user_b, "editor", user_a)
            .await
            .unwrap();
        let row: (String,) = sqlx::query_as(
            "SELECT joined_via FROM workspace_members WHERE workspace_id = $1 AND user_id = $2",
        )
        .bind(ws.id)
        .bind(user_b)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.0, "invitation");

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(ws.id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    async fn test_add_member_direct_joined_via_direct() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let (user_a, user_b, _) = create_test_users(&pool).await;
        let slug = format!(
            "test-ws-{}",
            Uuid::new_v4().to_string().split('-').next().unwrap()
        );
        let ws = create(&pool, "Test WS", &slug, "public", user_a)
            .await
            .unwrap();

        add_member_direct(&pool, ws.id, user_b, "editor", user_a)
            .await
            .unwrap();
        let row: (String,) = sqlx::query_as(
            "SELECT joined_via FROM workspace_members WHERE workspace_id = $1 AND user_id = $2",
        )
        .bind(ws.id)
        .bind(user_b)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.0, "direct");

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(ws.id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    async fn test_add_member_public_join_joined_via_public_join() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let (user_a, user_b, _) = create_test_users(&pool).await;
        let slug = format!(
            "test-ws-{}",
            Uuid::new_v4().to_string().split('-').next().unwrap()
        );
        let ws = create(&pool, "Test WS", &slug, "public", user_a)
            .await
            .unwrap();

        add_member_public_join(&pool, ws.id, user_b, "viewer", user_a)
            .await
            .unwrap();
        let row: (String,) = sqlx::query_as(
            "SELECT joined_via FROM workspace_members WHERE workspace_id = $1 AND user_id = $2",
        )
        .bind(ws.id)
        .bind(user_b)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.0, "public_join");

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(ws.id)
            .execute(&pool)
            .await;
    }

    // ═══════════════════════════════════════════════════════════════
    // resolve_user_identifier Tests
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_resolve_user_identifier_by_uuid() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let (user_a, _, _) = create_test_users(&pool).await;

        let resolved = resolve_user_identifier(&pool, &user_a.to_string())
            .await
            .unwrap();
        assert_eq!(resolved, Some(user_a));
    }

    #[tokio::test]
    async fn test_resolve_user_identifier_by_email() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let (user_a, _, _) = create_test_users(&pool).await;

        let resolved = resolve_user_identifier(&pool, "alice@test.com")
            .await
            .unwrap();
        assert_eq!(resolved, Some(user_a));
    }

    #[tokio::test]
    async fn test_resolve_user_identifier_by_name() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let (user_a, _, _) = create_test_users(&pool).await;

        let resolved = resolve_user_identifier(&pool, "Alice").await.unwrap();
        assert_eq!(resolved, Some(user_a));
    }

    #[tokio::test]
    async fn test_resolve_user_identifier_not_found() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let _ = create_test_users(&pool).await;

        let resolved = resolve_user_identifier(&pool, "nonexistent@test.com")
            .await
            .unwrap();
        assert!(resolved.is_none());
    }

    // ═══════════════════════════════════════════════════════════════
    // Invitation Lifecycle Tests
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_resend_invitation_updates_count_and_expiry() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let (user_a, user_b, _) = create_test_users(&pool).await;
        let slug = format!(
            "test-ws-{}",
            Uuid::new_v4().to_string().split('-').next().unwrap()
        );
        let ws = create(&pool, "Test WS", &slug, "public", user_a)
            .await
            .unwrap();

        let inv = create_invitation(&pool, ws.id, "bob@test.com", "editor", user_a, user_b)
            .await
            .unwrap();
        assert_eq!(inv.resent_count, 0);

        let resent = resend_invitation(&pool, inv.id).await.unwrap();
        assert_eq!(resent.resent_count, 1);
        assert!(resent.last_resent_at.is_some());
        assert!(resent.expires_at.is_some());

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(ws.id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    async fn test_revoke_invitation_sets_expired() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let (user_a, user_b, _) = create_test_users(&pool).await;
        let slug = format!(
            "test-ws-{}",
            Uuid::new_v4().to_string().split('-').next().unwrap()
        );
        let ws = create(&pool, "Test WS", &slug, "public", user_a)
            .await
            .unwrap();

        let inv = create_invitation(&pool, ws.id, "bob@test.com", "viewer", user_a, user_b)
            .await
            .unwrap();
        let revoked = revoke_invitation(&pool, inv.id).await.unwrap();
        assert_eq!(revoked.status, "expired");

        // Cannot revoke again (status no longer pending)
        let result = revoke_invitation(&pool, inv.id).await;
        assert!(result.is_err());

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(ws.id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    async fn test_expire_stale_invitations() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let (user_a, user_b, _) = create_test_users(&pool).await;
        let slug = format!(
            "test-ws-{}",
            Uuid::new_v4().to_string().split('-').next().unwrap()
        );
        let ws = create(&pool, "Test WS", &slug, "public", user_a)
            .await
            .unwrap();

        // Create an invitation with a past expiry
        sqlx::query(
            "INSERT INTO invitations (workspace_id, email, role, invited_by, invited_user_id, status, expires_at) \
             VALUES ($1, $2, $3, $4, $5, 'pending', now() - INTERVAL '8 days')"
        )
        .bind(ws.id).bind("expired@test.com").bind("viewer").bind(user_a).bind(user_b)
        .execute(&pool).await.unwrap();

        let count = expire_stale_invitations(&pool).await.unwrap();
        assert!(
            count >= 1,
            "should have expired at least one stale invitation"
        );

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(ws.id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    async fn test_find_invitations_by_workspace() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let (user_a, user_b, _) = create_test_users(&pool).await;
        let slug = format!(
            "test-ws-{}",
            Uuid::new_v4().to_string().split('-').next().unwrap()
        );
        let ws = create(&pool, "Test WS", &slug, "public", user_a)
            .await
            .unwrap();

        create_invitation(&pool, ws.id, "a@test.com", "viewer", user_a, user_b)
            .await
            .unwrap();
        create_invitation(&pool, ws.id, "b@test.com", "editor", user_a, user_b)
            .await
            .unwrap();

        let invs = find_invitations_by_workspace(&pool, ws.id).await.unwrap();
        assert_eq!(invs.len(), 2);

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(ws.id)
            .execute(&pool)
            .await;
    }

    // ═══════════════════════════════════════════════════════════════
    // Role Check Constraint Tests (migration 009 only)
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_invalid_role_rejected_by_check_constraint() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let (user_a, user_b, _) = create_test_users(&pool).await;
        let slug = format!(
            "test-ws-{}",
            Uuid::new_v4().to_string().split('-').next().unwrap()
        );
        let ws = create(&pool, "Test WS", &slug, "public", user_a)
            .await
            .unwrap();

        // "writer" is no longer a valid role after migration 009
        let result = sqlx::query(
            "INSERT INTO workspace_members (workspace_id, user_id, role, invited_by, joined_via) \
             VALUES ($1, $2, 'writer', $3, 'invitation') ON CONFLICT DO NOTHING",
        )
        .bind(ws.id)
        .bind(user_b)
        .bind(user_a)
        .execute(&pool)
        .await;
        assert!(
            result.is_err(),
            "role 'writer' should be rejected by CHECK constraint"
        );

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(ws.id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    async fn test_all_new_roles_accepted_by_check_constraint() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let (user_a, user_b, _) = create_test_users(&pool).await;
        let slug = format!(
            "test-ws-{}",
            Uuid::new_v4().to_string().split('-').next().unwrap()
        );
        let ws = create(&pool, "Test WS", &slug, "public", user_a)
            .await
            .unwrap();

        // All 4 valid roles should be accepted
        for role in ["owner", "manager", "editor", "viewer"] {
            let result = sqlx::query(
                "INSERT INTO workspace_members (workspace_id, user_id, role, invited_by, joined_via) \
                 VALUES ($1, $2, $3, $4, 'invitation') ON CONFLICT (workspace_id, user_id) DO UPDATE SET role = $3"
            )
            .bind(ws.id).bind(user_b).bind(role).bind(user_a)
            .execute(&pool).await;
            assert!(
                result.is_ok(),
                "role '{role}' should be accepted: {result:?}"
            );
        }

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(ws.id)
            .execute(&pool)
            .await;
    }
}
