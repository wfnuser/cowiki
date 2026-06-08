use std::sync::Arc;

use crate::error::{AppError, Result};
use crate::AppState;

/// Permission levels corresponding to minimum role required.
#[derive(Debug, Clone, Copy)]
pub enum Permission {
    ViewContent,        // Viewer+
    EditContent,        // Editor+
    ManageMembers,      // Manager+
    ManageWorkspace,    // Manager+
    DeleteWorkspace,    // Owner only
    TransferOwnership,  // Owner only
}

impl Permission {
    pub fn required_role(&self) -> cowiki_db::workspaces::Role {
        use cowiki_db::workspaces::Role;
        match self {
            Self::DeleteWorkspace | Self::TransferOwnership => Role::Owner,
            Self::ManageMembers | Self::ManageWorkspace => Role::Manager,
            Self::EditContent => Role::Editor,
            Self::ViewContent => Role::Viewer,
        }
    }
}

/// Resolved workspace context after permission check.
pub struct WorkspaceGuard {
    pub workspace: cowiki_db::workspaces::Workspace,
    pub user: cowiki_db::users::User,
    pub member_role: cowiki_db::workspaces::Role,
}

/// Resolve user + workspace + member role, verifying at least ViewContent permission.
pub async fn require_membership(
    state: &Arc<AppState>,
    headers: &axum::http::HeaderMap,
    slug: &str,
) -> Result<WorkspaceGuard> {
    let user = crate::routes::auth::extract_user(&state.db, headers).await?;

    let workspace = cowiki_db::workspaces::find_by_slug(&state.db, slug)
        .await?
        .ok_or_else(|| AppError::NotFound("workspace not found".into()))?;

    let role_str = cowiki_db::workspaces::get_member_role(&state.db, workspace.id, user.id)
        .await?
        .ok_or_else(|| AppError::Forbidden("not a member of this workspace".into()))?;

    let member_role: cowiki_db::workspaces::Role = role_str.parse()
        .map_err(|_| AppError::Internal("invalid role stored in database".into()))?;

    // Touch last_active_at (fire-and-forget)
    let db = state.db.clone();
    let ws_id = workspace.id;
    let uid = user.id;
    tokio::spawn(async move { let _ = cowiki_db::workspaces::touch_last_active(&db, ws_id, uid).await; });

    Ok(WorkspaceGuard { workspace, user, member_role })
}

/// Require a specific permission level.
pub fn require(guard: &WorkspaceGuard, permission: Permission) -> Result<()> {
    let required = permission.required_role();
    if guard.member_role >= required {
        Ok(())
    } else {
        Err(AppError::Forbidden(format!(
            "requires {:?} role or higher (current: {})",
            required, guard.member_role
        )))
    }
}

// ── Unit Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use cowiki_db::workspaces::Role;

    fn mock_workspace() -> cowiki_db::workspaces::Workspace {
        cowiki_db::workspaces::Workspace {
            id: uuid::Uuid::nil(),
            name: "test-ws".into(),
            slug: "test-ws".into(),
            visibility: "public".into(),
            created_by: uuid::Uuid::nil(),
            created_at: chrono::Utc::now(),
        }
    }

    fn mock_user() -> cowiki_db::users::User {
        cowiki_db::users::User {
            id: uuid::Uuid::nil(),
            name: "test-user".into(),
            email: None,
            api_key: "test-key".into(),
        }
    }

    fn mock_guard(role: Role) -> WorkspaceGuard {
        WorkspaceGuard {
            workspace: mock_workspace(),
            user: mock_user(),
            member_role: role,
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // Permission → required_role mapping
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_permission_required_role_mapping() {
        assert_eq!(Permission::ViewContent.required_role(), Role::Viewer);
        assert_eq!(Permission::EditContent.required_role(), Role::Editor);
        assert_eq!(Permission::ManageMembers.required_role(), Role::Manager);
        assert_eq!(Permission::ManageWorkspace.required_role(), Role::Manager);
        assert_eq!(Permission::DeleteWorkspace.required_role(), Role::Owner);
        assert_eq!(Permission::TransferOwnership.required_role(), Role::Owner);
    }

    // ═══════════════════════════════════════════════════════════════
    // require() with each role against each permission
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_require_owner_passes_all() {
        let guard = mock_guard(Role::Owner);
        assert!(require(&guard, Permission::ViewContent).is_ok());
        assert!(require(&guard, Permission::EditContent).is_ok());
        assert!(require(&guard, Permission::ManageMembers).is_ok());
        assert!(require(&guard, Permission::ManageWorkspace).is_ok());
        assert!(require(&guard, Permission::DeleteWorkspace).is_ok());
        assert!(require(&guard, Permission::TransferOwnership).is_ok());
    }

    #[test]
    fn test_require_manager_permissions() {
        let guard = mock_guard(Role::Manager);
        // Manager passes Manager-level and below
        assert!(require(&guard, Permission::ViewContent).is_ok());
        assert!(require(&guard, Permission::EditContent).is_ok());
        assert!(require(&guard, Permission::ManageMembers).is_ok());
        assert!(require(&guard, Permission::ManageWorkspace).is_ok());
        // Manager fails Owner-level
        assert!(require(&guard, Permission::DeleteWorkspace).is_err());
        assert!(require(&guard, Permission::TransferOwnership).is_err());
    }

    #[test]
    fn test_require_editor_permissions() {
        let guard = mock_guard(Role::Editor);
        // Editor passes Editor-level and below
        assert!(require(&guard, Permission::ViewContent).is_ok());
        assert!(require(&guard, Permission::EditContent).is_ok());
        // Editor fails Manager-level and above
        assert!(require(&guard, Permission::ManageMembers).is_err());
        assert!(require(&guard, Permission::ManageWorkspace).is_err());
        assert!(require(&guard, Permission::DeleteWorkspace).is_err());
        assert!(require(&guard, Permission::TransferOwnership).is_err());
    }

    #[test]
    fn test_require_viewer_permissions() {
        let guard = mock_guard(Role::Viewer);
        // Viewer passes only ViewContent
        assert!(require(&guard, Permission::ViewContent).is_ok());
        // Viewer fails everything else
        assert!(require(&guard, Permission::EditContent).is_err());
        assert!(require(&guard, Permission::ManageMembers).is_err());
        assert!(require(&guard, Permission::ManageWorkspace).is_err());
        assert!(require(&guard, Permission::DeleteWorkspace).is_err());
        assert!(require(&guard, Permission::TransferOwnership).is_err());
    }

    // ═══════════════════════════════════════════════════════════════
    // Error message content
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_require_error_message_contains_required_role() {
        let guard = mock_guard(Role::Viewer);
        let err = require(&guard, Permission::EditContent).unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("Editor"), "error should mention required role: {msg}");
    }

    #[test]
    fn test_require_error_message_contains_current_role() {
        let guard = mock_guard(Role::Viewer);
        let err = require(&guard, Permission::ManageMembers).unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("viewer") || msg.contains("Viewer"),
            "error should mention current role: {msg}");
    }

    // ═══════════════════════════════════════════════════════════════
    // Cross-role permission matrix (exhaustive)
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_permission_matrix_exhaustive() {
        let roles = [Role::Viewer, Role::Editor, Role::Manager, Role::Owner];
        let permissions = [
            (Permission::ViewContent, "ViewContent"),
            (Permission::EditContent, "EditContent"),
            (Permission::ManageMembers, "ManageMembers"),
            (Permission::ManageWorkspace, "ManageWorkspace"),
            (Permission::DeleteWorkspace, "DeleteWorkspace"),
            (Permission::TransferOwnership, "TransferOwnership"),
        ];

        for &role in &roles {
            let guard = mock_guard(role);
            for &(perm, name) in &permissions {
                let result = require(&guard, perm);
                let should_pass = role >= perm.required_role();
                assert!(
                    result.is_ok() == should_pass,
                    "{role:?} {name}: expected {expected} but got {actual}",
                    expected = if should_pass { "pass" } else { "fail" },
                    actual = if result.is_ok() { "pass" } else { "fail" },
                );
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // Permission enum exhaustiveness
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_all_permissions_have_role_mapping() {
        // Every Permission variant must map to a valid role
        let all = [
            Permission::ViewContent,
            Permission::EditContent,
            Permission::ManageMembers,
            Permission::ManageWorkspace,
            Permission::DeleteWorkspace,
            Permission::TransferOwnership,
        ];
        for perm in &all {
            let role = perm.required_role();
            assert!(Role::ALL.contains(&role),
                "{perm:?} maps to {role:?} which is in Role::ALL");
        }
    }
}
