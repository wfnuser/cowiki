use std::sync::Arc;
use uuid::Uuid;

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
