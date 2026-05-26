use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::routes::auth::extract_user;
use crate::AppState;

#[derive(Deserialize)]
pub struct CreateWorkspaceRequest {
    pub name: String,
    pub slug: String,
    pub visibility: Option<String>, // "private" (default) or "public"
}

#[derive(Serialize)]
pub struct WorkspaceResponse {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub visibility: String,
    pub role: String,
}

fn ws_response(ws: &cowiki_db::workspaces::Workspace, role: &str) -> WorkspaceResponse {
    WorkspaceResponse {
        id: ws.id.to_string(),
        name: ws.name.clone(),
        slug: ws.slug.clone(),
        visibility: ws.visibility.clone(),
        role: role.to_string(),
    }
}

/// Create a new workspace
pub async fn create_workspace(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(input): Json<CreateWorkspaceRequest>,
) -> Result<Json<WorkspaceResponse>> {
    let user = extract_user(&state.db, &headers).await?;

    if !input.slug.chars().all(|c| c.is_alphanumeric() || c == '-') {
        return Err(AppError::BadRequest("slug must be alphanumeric with hyphens".into()));
    }

    let visibility = input.visibility.as_deref().unwrap_or("private");
    if visibility != "private" && visibility != "public" {
        return Err(AppError::BadRequest("visibility must be 'private' or 'public'".into()));
    }

    let ws = cowiki_db::workspaces::create(&state.db, &input.name, &input.slug, visibility, user.id)
        .await
        .map_err(|e| {
            if e.to_string().contains("duplicate") {
                AppError::BadRequest("workspace slug already taken".into())
            } else {
                AppError::Internal(e.to_string())
            }
        })?;

    Ok(Json(ws_response(&ws, "owner")))
}

/// List my workspaces (created + joined)
pub async fn list_workspaces(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Vec<WorkspaceResponse>>> {
    let user = extract_user(&state.db, &headers).await?;
    let workspaces = cowiki_db::workspaces::list_for_user(&state.db, user.id).await?;

    let mut result = Vec::new();
    for ws in workspaces {
        let role = cowiki_db::workspaces::get_member_role(&state.db, ws.id, user.id)
            .await?
            .unwrap_or_else(|| if ws.created_by == user.id { "owner".into() } else { "writer".into() });
        result.push(ws_response(&ws, &role));
    }
    Ok(Json(result))
}

/// List all public workspaces
pub async fn list_public_workspaces(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Vec<WorkspaceResponse>>> {
    let _user = extract_user(&state.db, &headers).await?;
    let workspaces = cowiki_db::workspaces::list_public(&state.db).await?;

    let result = workspaces.iter().map(|ws| ws_response(ws, "viewer")).collect();
    Ok(Json(result))
}

/// Join a public workspace
pub async fn join_workspace(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(workspace_slug): Path<String>,
) -> Result<Json<WorkspaceResponse>> {
    let user = extract_user(&state.db, &headers).await?;

    let ws = cowiki_db::workspaces::find_by_slug(&state.db, &workspace_slug)
        .await?
        .ok_or_else(|| AppError::NotFound("workspace not found".into()))?;

    if ws.visibility != "public" {
        return Err(AppError::BadRequest("workspace is private, you need an invitation".into()));
    }

    cowiki_db::workspaces::add_member(&state.db, ws.id, user.id, "writer", user.id).await?;

    // Create user branch in default repo AND workspace repo
    state.wiki_repo
        .ensure_user_branch(&user.id.to_string())
        .map_err(|e| AppError::Internal(e.to_string()))?;
    state.repo_manager
        .get(&workspace_slug)
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ensure_user_branch(&user.id.to_string())
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(ws_response(&ws, "writer")))
}

#[derive(Deserialize)]
pub struct InviteWithRoleRequest {
    pub email: String,
    pub role: Option<String>, // defaults to "writer"
}

#[derive(Serialize)]
pub struct InviteResponse {
    pub invitation_id: String,
    pub email: String,
    pub workspace: String,
}

/// Invite someone to a workspace by email with optional role.
/// Owner-only. Creates a pending invitation — the invited user must accept/reject.
pub async fn invite(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(workspace_slug): Path<String>,
    Json(input): Json<InviteWithRoleRequest>,
) -> Result<Json<InviteResponse>> {
    let user = extract_user(&state.db, &headers).await?;

    // Validate role
    let role = input.role.as_deref().unwrap_or("writer");
    if !cowiki_db::workspaces::Role::ALL.contains(&role) {
        return Err(AppError::BadRequest(
            format!("invalid role '{role}': must be one of: owner, writer, reader")
        ));
    }

    let ws = cowiki_db::workspaces::find_by_slug(&state.db, &workspace_slug)
        .await?
        .ok_or_else(|| AppError::NotFound("workspace not found".into()))?;

    // Require owner
    let current_role = cowiki_db::workspaces::get_member_role(&state.db, ws.id, user.id)
        .await?
        .unwrap_or_default();
    if current_role != "owner" {
        return Err(AppError::Forbidden("only the workspace owner can invite members".into()));
    }

    // Create invitation (no auto-add — invitee must accept)
    let invitation = cowiki_db::workspaces::create_invitation(
        &state.db, ws.id, &input.email, role, user.id,
    ).await?;

    // Audit log
    cowiki_db::audit::log(
        &state.db, ws.id, user.id,
        "invite_member", Some("invitation"), Some(invitation.id),
        Some(serde_json::json!({"email": input.email, "role": role})),
    ).await?;

    Ok(Json(InviteResponse {
        invitation_id: invitation.id.to_string(),
        email: input.email,
        workspace: ws.slug,
    }))
}

/// List members of a workspace
pub async fn list_members(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(workspace_slug): Path<String>,
) -> Result<Json<Vec<MemberResponse>>> {
    let user = extract_user(&state.db, &headers).await?;

    let ws = cowiki_db::workspaces::find_by_slug(&state.db, &workspace_slug)
        .await?
        .ok_or_else(|| AppError::NotFound("workspace not found".into()))?;

    if !cowiki_db::workspaces::is_member(&state.db, ws.id, user.id).await? {
        return Err(AppError::Forbidden("you are not a member of this workspace".into()));
    }

    let members = cowiki_db::workspaces::list_members(&state.db, ws.id).await?;

    let mut result = Vec::new();
    for m in members {
        if let Some(u) = cowiki_db::users::find_by_id(&state.db, m.user_id).await? {
            result.push(MemberResponse {
                id: u.id.to_string(),
                name: u.name,
                email: u.email,
                role: m.role,
            });
        }
    }
    Ok(Json(result))
}

#[derive(Serialize)]
pub struct MemberResponse {
    pub id: String,
    pub name: String,
    pub email: Option<String>,
    pub role: String,
}

#[derive(Deserialize)]
pub struct RenameRequest {
    pub name: String,
}

pub async fn rename_workspace(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(workspace_slug): Path<String>,
    Json(input): Json<RenameRequest>,
) -> Result<Json<WorkspaceResponse>> {
    let user = extract_user(&state.db, &headers).await?;
    let ws = cowiki_db::workspaces::find_by_slug(&state.db, &workspace_slug)
        .await?
        .ok_or_else(|| AppError::NotFound("workspace not found".into()))?;
    let role = cowiki_db::workspaces::get_member_role(&state.db, ws.id, user.id)
        .await?
        .unwrap_or_default();
    if role != "owner" {
        return Err(AppError::Forbidden("only the owner can rename".into()));
    }
    let updated = cowiki_db::workspaces::rename(&state.db, ws.id, &input.name).await?;

    // Audit log
    cowiki_db::audit::log(
        &state.db, ws.id, user.id,
        "rename_workspace", Some("workspace"), Some(ws.id),
        Some(serde_json::json!({"old_name": ws.name, "new_name": input.name})),
    ).await?;

    Ok(Json(ws_response(&updated, "owner")))
}

// ── Invitation accept/reject/pending ──

#[derive(Serialize)]
pub struct PendingInvitationResponse {
    pub id: String,
    pub workspace_id: String,
    pub workspace_name: String,
    pub workspace_slug: String,
    pub role: String,
    pub invited_by_name: String,
    pub created_at: String,
}

/// Accept a pending invitation. Adds the user as a member with the invited role.
pub async fn accept_invitation(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(invitation_id): Path<Uuid>,
) -> Result<Json<WorkspaceResponse>> {
    let user = extract_user(&state.db, &headers).await?;

    let invitation = cowiki_db::workspaces::find_invitation_by_id(&state.db, invitation_id)
        .await?
        .ok_or_else(|| AppError::NotFound("invitation not found".into()))?;

    if invitation.status != "pending" {
        return Err(AppError::BadRequest("invitation is no longer pending".into()));
    }

    // Verify the invitation is for this user (by email match)
    let current_user = cowiki_db::users::find_by_id(&state.db, user.id)
        .await?
        .ok_or_else(|| AppError::Unauthorized("user not found".into()))?;

    if current_user.email.as_deref() != Some(&invitation.email) {
        return Err(AppError::Forbidden("this invitation is for a different email address".into()));
    }

    // Add as member with the invited role
    cowiki_db::workspaces::add_member(
        &state.db, invitation.workspace_id, user.id,
        &invitation.role, invitation.invited_by,
    ).await?;

    // Accept the invitation
    cowiki_db::workspaces::accept_invitation(&state.db, invitation.id).await?;

    // Audit log
    cowiki_db::audit::log(
        &state.db, invitation.workspace_id, user.id,
        "accept_invitation", Some("invitation"), Some(invitation.id),
        Some(serde_json::json!({"role": invitation.role})),
    ).await?;

    // Create user branch in default repo AND workspace repo
    let ws = cowiki_db::workspaces::find_by_id(
        &state.db, invitation.workspace_id,
    ).await?
    .ok_or_else(|| AppError::NotFound("workspace not found".into()))?;
    
    state.wiki_repo
        .ensure_user_branch(&user.id.to_string())
        .map_err(|e| AppError::Internal(e.to_string()))?;
    state.repo_manager
        .get(&ws.slug)
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ensure_user_branch(&user.id.to_string())
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(ws_response(&ws, &invitation.role)))
}

/// Reject a pending invitation.
pub async fn reject_invitation(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(invitation_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>> {
    let user = extract_user(&state.db, &headers).await?;

    let invitation = cowiki_db::workspaces::find_invitation_by_id(&state.db, invitation_id)
        .await?
        .ok_or_else(|| AppError::NotFound("invitation not found".into()))?;

    if invitation.status != "pending" {
        return Err(AppError::BadRequest("invitation is no longer pending".into()));
    }

    // Verify email match
    let current_user = cowiki_db::users::find_by_id(&state.db, user.id)
        .await?
        .ok_or_else(|| AppError::Unauthorized("user not found".into()))?;

    if current_user.email.as_deref() != Some(&invitation.email) {
        return Err(AppError::Forbidden("this invitation is for a different email address".into()));
    }

    cowiki_db::workspaces::reject_invitation(&state.db, invitation.id).await?;

    // Audit log
    cowiki_db::audit::log(
        &state.db, invitation.workspace_id, user.id,
        "reject_invitation", Some("invitation"), Some(invitation.id),
        None,
    ).await?;

    Ok(Json(serde_json::json!({"status": "rejected"})))
}

/// List pending invitations for the current user.
pub async fn list_pending_invitations(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Vec<PendingInvitationResponse>>> {
    let user = extract_user(&state.db, &headers).await?;

    let invitations = cowiki_db::workspaces::find_pending_invitations_for_user(
        &state.db, user.id,
    ).await?;

    let mut result = Vec::new();
    for inv in invitations {
        let ws = cowiki_db::workspaces::find_by_id(&state.db, inv.workspace_id)
            .await?
            .ok_or_else(|| AppError::NotFound("workspace not found".into()))?;
        let inviter = cowiki_db::users::find_by_id(&state.db, inv.invited_by)
            .await?
            .map(|u| u.name)
            .unwrap_or_default();

        result.push(PendingInvitationResponse {
            id: inv.id.to_string(),
            workspace_id: ws.id.to_string(),
            workspace_name: ws.name.clone(),
            workspace_slug: ws.slug.clone(),
            role: inv.role.clone(),
            invited_by_name: inviter,
            created_at: inv.created_at.to_rfc3339(),
        });
    }
    Ok(Json(result))
}

// ── Member management (owner only) ──

#[derive(Deserialize)]
pub struct RemoveMemberRequest {
    pub user_id: String,
}

/// Remove a member from a workspace (owner only). Cannot remove the owner.
pub async fn remove_member(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(workspace_slug): Path<String>,
    Json(input): Json<RemoveMemberRequest>,
) -> Result<Json<serde_json::Value>> {
    let user = extract_user(&state.db, &headers).await?;

    let ws = cowiki_db::workspaces::find_by_slug(&state.db, &workspace_slug)
        .await?
        .ok_or_else(|| AppError::NotFound("workspace not found".into()))?;

    // Require owner
    let current_role = cowiki_db::workspaces::get_member_role(&state.db, ws.id, user.id)
        .await?
        .unwrap_or_default();
    if current_role != "owner" {
        return Err(AppError::Forbidden("only the owner can remove members".into()));
    }

    let target_id = Uuid::parse_str(&input.user_id)
        .map_err(|_| AppError::BadRequest("invalid user_id".into()))?;

    if target_id == user.id {
        return Err(AppError::BadRequest("cannot remove yourself as owner".into()));
    }

    let removed = cowiki_db::workspaces::remove_member(&state.db, ws.id, target_id).await?;
    if !removed {
        return Err(AppError::NotFound("member not found or is owner".into()));
    }

    // Audit log
    cowiki_db::audit::log(
        &state.db, ws.id, user.id,
        "remove_member", Some("user"), Some(target_id),
        None,
    ).await?;

    Ok(Json(serde_json::json!({"status": "removed"})))
}

#[derive(Deserialize)]
pub struct ChangeRoleRequest {
    pub user_id: String,
    pub role: String,
}

/// Change a member's role (owner only). Cannot change the owner's role.
pub async fn change_member_role(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(workspace_slug): Path<String>,
    Json(input): Json<ChangeRoleRequest>,
) -> Result<Json<MemberResponse>> {
    let user = extract_user(&state.db, &headers).await?;

    let ws = cowiki_db::workspaces::find_by_slug(&state.db, &workspace_slug)
        .await?
        .ok_or_else(|| AppError::NotFound("workspace not found".into()))?;

    // Require owner
    let current_role = cowiki_db::workspaces::get_member_role(&state.db, ws.id, user.id)
        .await?
        .unwrap_or_default();
    if current_role != "owner" {
        return Err(AppError::Forbidden("only the owner can change member roles".into()));
    }

    // Validate target role
    if !cowiki_db::workspaces::Role::ALL.contains(&input.role.as_str()) {
        return Err(AppError::BadRequest(
            format!("invalid role '{}': must be owner, writer, or reader", input.role)
        ));
    }

    let target_id = Uuid::parse_str(&input.user_id)
        .map_err(|_| AppError::BadRequest("invalid user_id".into()))?;

    let new_role = cowiki_db::workspaces::change_member_role(
        &state.db, ws.id, target_id, &input.role,
    ).await?
    .ok_or_else(|| AppError::NotFound("member not found or is owner".into()))?;

    // Audit log
    cowiki_db::audit::log(
        &state.db, ws.id, user.id,
        "change_member_role", Some("user"), Some(target_id),
        Some(serde_json::json!({"new_role": new_role})),
    ).await?;

    let member_user = cowiki_db::users::find_by_id(&state.db, target_id)
        .await?
        .ok_or_else(|| AppError::NotFound("user not found".into()))?;

    Ok(Json(MemberResponse {
        id: member_user.id.to_string(),
        name: member_user.name,
        email: member_user.email,
        role: new_role,
    }))
}

// ── Workspace deletion (owner only) ──

/// Delete a workspace (owner only). Cascade deletes members and content.
pub async fn delete_workspace(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(workspace_slug): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let user = extract_user(&state.db, &headers).await?;

    let ws = cowiki_db::workspaces::find_by_slug(&state.db, &workspace_slug)
        .await?
        .ok_or_else(|| AppError::NotFound("workspace not found".into()))?;

    // Require owner
    let current_role = cowiki_db::workspaces::get_member_role(&state.db, ws.id, user.id)
        .await?
        .unwrap_or_default();
    if current_role != "owner" {
        return Err(AppError::Forbidden("only the owner can delete a workspace".into()));
    }

    // Audit log (before delete so we capture it)
    cowiki_db::audit::log(
        &state.db, ws.id, user.id,
        "delete_workspace", Some("workspace"), Some(ws.id),
        Some(serde_json::json!({"name": ws.name, "slug": ws.slug})),
    ).await?;

    cowiki_db::workspaces::delete_workspace(&state.db, ws.id).await?;

    Ok(Json(serde_json::json!({"status": "deleted"})))
}
