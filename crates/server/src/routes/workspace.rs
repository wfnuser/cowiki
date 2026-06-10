use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::routes::auth::extract_user;
use crate::routes::guard::{self, Permission};
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

    if input.name.is_empty() || input.name.len() > 100 {
        return Err(AppError::BadRequest(
            "name must be between 1 and 100 characters".into(),
        ));
    }
    if input.slug.is_empty() || input.slug.len() > 50 {
        return Err(AppError::BadRequest(
            "slug must be between 1 and 50 characters".into(),
        ));
    }
    if !input
        .slug
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(AppError::BadRequest("slug must match [a-z0-9-]+".into()));
    }

    let visibility = input.visibility.as_deref().unwrap_or("private");
    if visibility != "private" && visibility != "public" {
        return Err(AppError::BadRequest(
            "visibility must be 'private' or 'public'".into(),
        ));
    }

    let ws =
        cowiki_db::workspaces::create(&state.db, &input.name, &input.slug, visibility, user.id)
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
            .unwrap_or_else(|| {
                if ws.created_by == user.id {
                    "owner".into()
                } else {
                    "viewer".into()
                }
            });
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

    let result = workspaces
        .iter()
        .map(|ws| ws_response(ws, "viewer"))
        .collect();
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
        return Err(AppError::BadRequest(
            "workspace is private, you need an invitation".into(),
        ));
    }

    cowiki_db::workspaces::add_member_public_join(&state.db, ws.id, user.id, "editor", user.id)
        .await?;

    // Create user branch in the workspace repo
    state
        .repo_manager
        .get(&workspace_slug)
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ensure_user_branch(&user.id.to_string())
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(ws_response(&ws, "editor")))
}

#[derive(Deserialize)]
pub struct InviteWithRoleRequest {
    pub user: String,         // id (UUID), email, or username
    pub role: Option<String>, // defaults to "viewer"
}

#[derive(Deserialize)]
pub struct BatchInviteRequest {
    pub invitations: Vec<InviteWithRoleRequest>,
}

#[derive(Serialize)]
pub struct InviteResult {
    pub user: String,
    pub user_id: Option<String>,
    pub status: String, // "sent" | "failed"
    pub invitation_id: Option<String>,
    pub reason: Option<String>,
}

#[derive(Serialize)]
pub struct BatchInviteResponse {
    pub sent: usize,
    pub failed: usize,
    pub results: Vec<InviteResult>,
}

/// Invite users to a workspace (Manager+). Supports batch via `BatchInviteRequest`.
/// Creates pending invitations — invitees must accept/reject.
pub async fn invite(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(workspace_slug): Path<String>,
    Json(input): Json<BatchInviteRequest>,
) -> Result<Json<BatchInviteResponse>> {
    let guard = guard::require_membership(&state, &headers, &workspace_slug).await?;
    guard::require(&guard, Permission::ManageMembers)?;

    let mut results = Vec::new();
    let mut sent = 0;
    let mut failed = 0;

    for item in &input.invitations {
        // Validate role
        let role = item.role.as_deref().unwrap_or("viewer");
        if role.parse::<cowiki_db::workspaces::Role>().is_err() {
            failed += 1;
            results.push(InviteResult {
                user: item.user.clone(),
                user_id: None,
                status: "failed".into(),
                invitation_id: None,
                reason: Some(format!("invalid role: {role}")),
            });
            continue;
        }

        // Resolve user identifier
        let invited_user =
            match cowiki_db::workspaces::resolve_user_identifier(&state.db, &item.user).await {
                Ok(Some(id)) => id,
                Ok(None) => {
                    failed += 1;
                    results.push(InviteResult {
                        user: item.user.clone(),
                        user_id: None,
                        status: "failed".into(),
                        invitation_id: None,
                        reason: Some("user not found".into()),
                    });
                    continue;
                }
                Err(_) => {
                    failed += 1;
                    results.push(InviteResult {
                        user: item.user.clone(),
                        user_id: None,
                        status: "failed".into(),
                        invitation_id: None,
                        reason: Some("lookup error".into()),
                    });
                    continue;
                }
            };

        // Cannot invite self
        if invited_user == guard.user.id {
            failed += 1;
            results.push(InviteResult {
                user: item.user.clone(),
                user_id: Some(invited_user.to_string()),
                status: "failed".into(),
                invitation_id: None,
                reason: Some("cannot invite yourself".into()),
            });
            continue;
        }

        // Cannot invite existing member
        if cowiki_db::workspaces::is_member(&state.db, guard.workspace.id, invited_user)
            .await
            .unwrap_or(false)
        {
            failed += 1;
            results.push(InviteResult {
                user: item.user.clone(),
                user_id: Some(invited_user.to_string()),
                status: "failed".into(),
                invitation_id: None,
                reason: Some("already a member".into()),
            });
            continue;
        }

        // Create invitation
        match cowiki_db::workspaces::create_invitation(
            &state.db,
            guard.workspace.id,
            &item.user,
            role,
            guard.user.id,
            invited_user,
        )
        .await
        {
            Ok(invitation) => {
                // Fire-and-forget audit log + notification
                let db = state.db.clone();
                let ws_id = guard.workspace.id;
                let actor_id = guard.user.id;
                let inv_id = invitation.id;
                let user_display = item.user.clone();
                let role_display = role.to_string();
                let ws_name = guard.workspace.name.clone();
                let inviter_name = guard.user.name.clone();
                let inv_id2 = inv_id;
                tokio::spawn(async move {
                    let _ = cowiki_db::audit::log(
                        &db,
                        ws_id,
                        actor_id,
                        "invite_member",
                        Some("invitation"),
                        Some(inv_id),
                        Some(serde_json::json!({"user": user_display, "role": role_display})),
                    )
                    .await;
                    // Notify invitee
                    let _ = cowiki_db::notifications::create(
                        &db,
                        invited_user,
                        "invitation",
                        &format!("Invited to {}", ws_name),
                        Some(&format!(
                            "{} invited you to join {} as {}",
                            inviter_name, ws_name, role_display
                        )),
                        Some(ws_id),
                        Some(&format!("/invitations/{}", inv_id2)),
                    )
                    .await;
                });
                sent += 1;
                results.push(InviteResult {
                    user: item.user.clone(),
                    user_id: Some(invited_user.to_string()),
                    status: "sent".into(),
                    invitation_id: Some(invitation.id.to_string()),
                    reason: None,
                });
            }
            Err(_) => {
                failed += 1;
                results.push(InviteResult {
                    user: item.user.clone(),
                    user_id: Some(invited_user.to_string()),
                    status: "failed".into(),
                    invitation_id: None,
                    reason: Some("database error".into()),
                });
            }
        }
    }

    Ok(Json(BatchInviteResponse {
        sent,
        failed,
        results,
    }))
}

/// List members of a workspace
pub async fn list_members(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(workspace_slug): Path<String>,
) -> Result<Json<Vec<MemberResponse>>> {
    let guard = guard::require_membership(&state, &headers, &workspace_slug).await?;
    guard::require(&guard, Permission::ViewContent)?;

    let members = cowiki_db::workspaces::list_members(&state.db, guard.workspace.id).await?;

    let mut result = Vec::new();
    for m in members {
        if let Some(u) = cowiki_db::users::find_by_id(&state.db, m.user_id).await? {
            result.push(MemberResponse {
                id: u.id.to_string(),
                name: u.name,
                email: u.email,
                role: m.role,
                last_active_at: m.last_active_at.map(|t| t.to_rfc3339()),
                joined_via: m.joined_via,
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
    pub last_active_at: Option<String>,
    pub joined_via: String,
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
    let guard = guard::require_membership(&state, &headers, &workspace_slug).await?;
    guard::require(&guard, Permission::ManageWorkspace)?;
    let ws = &guard.workspace;
    let updated = cowiki_db::workspaces::rename(&state.db, ws.id, &input.name).await?;

    // Audit log
    cowiki_db::audit::log(
        &state.db,
        ws.id,
        guard.user.id,
        "rename_workspace",
        Some("workspace"),
        Some(ws.id),
        Some(serde_json::json!({"old_name": ws.name, "new_name": input.name})),
    )
    .await?;

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
        return Err(AppError::BadRequest(
            "invitation is no longer pending".into(),
        ));
    }

    // Verify invitation is for this user by invited_user_id
    match invitation.invited_user_id {
        Some(invited_id) if invited_id == user.id => {}
        _ => {
            return Err(AppError::Forbidden(
                "this invitation is for a different user".into(),
            ))
        }
    }

    // Add as member with the invited role
    cowiki_db::workspaces::add_member(
        &state.db,
        invitation.workspace_id,
        user.id,
        &invitation.role,
        invitation.invited_by,
    )
    .await?;

    // Accept the invitation
    cowiki_db::workspaces::accept_invitation(&state.db, invitation.id).await?;

    // Audit log
    cowiki_db::audit::log(
        &state.db,
        invitation.workspace_id,
        user.id,
        "accept_invitation",
        Some("invitation"),
        Some(invitation.id),
        Some(serde_json::json!({"role": invitation.role})),
    )
    .await?;

    // Create user branch in the workspace repo
    let ws = cowiki_db::workspaces::find_by_id(&state.db, invitation.workspace_id)
        .await?
        .ok_or_else(|| AppError::NotFound("workspace not found".into()))?;

    state
        .repo_manager
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
        return Err(AppError::BadRequest(
            "invitation is no longer pending".into(),
        ));
    }

    // Verify invitation is for this user by invited_user_id
    match invitation.invited_user_id {
        Some(invited_id) if invited_id == user.id => {}
        _ => {
            return Err(AppError::Forbidden(
                "this invitation is for a different user".into(),
            ))
        }
    }

    cowiki_db::workspaces::reject_invitation(&state.db, invitation.id).await?;

    // Audit log
    cowiki_db::audit::log(
        &state.db,
        invitation.workspace_id,
        user.id,
        "reject_invitation",
        Some("invitation"),
        Some(invitation.id),
        None,
    )
    .await?;

    Ok(Json(serde_json::json!({"status": "rejected"})))
}

/// List pending invitations for the current user.
pub async fn list_pending_invitations(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Vec<PendingInvitationResponse>>> {
    let user = extract_user(&state.db, &headers).await?;

    let invitations =
        cowiki_db::workspaces::find_pending_invitations_for_user(&state.db, user.id).await?;

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

// ── Invitation Management ──

#[derive(Serialize)]
pub struct InvitationDetailResponse {
    pub id: String,
    pub email: String,
    pub role: String,
    pub status: String,
    pub invited_user_id: Option<String>,
    pub message: Option<String>,
    pub expires_at: Option<String>,
    pub resent_count: i32,
    pub created_at: String,
}

/// List all invitations for a workspace (Manager+).
pub async fn list_invitations(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(workspace_slug): Path<String>,
) -> Result<Json<Vec<InvitationDetailResponse>>> {
    let guard = guard::require_membership(&state, &headers, &workspace_slug).await?;
    guard::require(&guard, Permission::ManageMembers)?;

    let invitations =
        cowiki_db::workspaces::find_invitations_by_workspace(&state.db, guard.workspace.id).await?;
    let result: Vec<InvitationDetailResponse> = invitations
        .iter()
        .map(|inv| InvitationDetailResponse {
            id: inv.id.to_string(),
            email: inv.email.clone(),
            role: inv.role.clone(),
            status: inv.status.clone(),
            invited_user_id: inv.invited_user_id.map(|id| id.to_string()),
            message: inv.message.clone(),
            expires_at: inv.expires_at.map(|t| t.to_rfc3339()),
            resent_count: inv.resent_count,
            created_at: inv.created_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(result))
}

/// Resend a pending invitation (Manager+).
pub async fn resend_invitation(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path((workspace_slug, invitation_id)): Path<(String, Uuid)>,
) -> Result<Json<InvitationDetailResponse>> {
    let guard = guard::require_membership(&state, &headers, &workspace_slug).await?;
    guard::require(&guard, Permission::ManageMembers)?;

    let inv = cowiki_db::workspaces::resend_invitation(&state.db, invitation_id)
        .await
        .map_err(|_| AppError::NotFound("invitation not found or not pending".into()))?;

    Ok(Json(InvitationDetailResponse {
        id: inv.id.to_string(),
        email: inv.email.clone(),
        role: inv.role.clone(),
        status: inv.status.clone(),
        invited_user_id: inv.invited_user_id.map(|id| id.to_string()),
        message: inv.message.clone(),
        expires_at: inv.expires_at.map(|t| t.to_rfc3339()),
        resent_count: inv.resent_count,
        created_at: inv.created_at.to_rfc3339(),
    }))
}

/// Revoke (expire) a pending invitation (Manager+).
pub async fn revoke_invitation(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path((workspace_slug, invitation_id)): Path<(String, Uuid)>,
) -> Result<Json<serde_json::Value>> {
    let guard = guard::require_membership(&state, &headers, &workspace_slug).await?;
    guard::require(&guard, Permission::ManageMembers)?;

    cowiki_db::workspaces::revoke_invitation(&state.db, invitation_id)
        .await
        .map_err(|_| AppError::NotFound("invitation not found or not pending".into()))?;

    Ok(Json(serde_json::json!({"status": "revoked"})))
}

// ── Member management ──

/// Remove a member from a workspace (Manager+).
pub async fn remove_member(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(workspace_slug): Path<String>,
    Json(input): Json<RemoveMemberRequest>,
) -> Result<Json<serde_json::Value>> {
    let guard = guard::require_membership(&state, &headers, &workspace_slug).await?;
    guard::require(&guard, Permission::ManageMembers)?;

    let target_id = Uuid::parse_str(&input.user_id)
        .map_err(|_| AppError::BadRequest("invalid user_id".into()))?;

    // Cannot manage Owner
    let target_role =
        cowiki_db::workspaces::get_member_role(&state.db, guard.workspace.id, target_id)
            .await?
            .unwrap_or_default();
    let target_role: cowiki_db::workspaces::Role = target_role
        .parse()
        .map_err(|_| AppError::BadRequest("invalid role".into()))?;
    if !guard.member_role.can_manage_role(target_role) {
        return Err(AppError::Forbidden(
            "cannot manage a member with equal or higher role".into(),
        ));
    }

    let removed =
        cowiki_db::workspaces::remove_member(&state.db, guard.workspace.id, target_id).await?;
    if !removed {
        return Err(AppError::NotFound("member not found".into()));
    }

    // Audit log
    cowiki_db::audit::log(
        &state.db,
        guard.workspace.id,
        guard.user.id,
        "remove_member",
        Some("user"),
        Some(target_id),
        None,
    )
    .await?;

    Ok(Json(serde_json::json!({"status": "removed"})))
}

#[derive(Deserialize)]
pub struct ChangeRoleRequest {
    pub user_id: String,
    pub role: String,
}

/// Change a member's role (Manager+).
pub async fn change_member_role(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(workspace_slug): Path<String>,
    Json(input): Json<ChangeRoleRequest>,
) -> Result<Json<MemberResponse>> {
    let guard = guard::require_membership(&state, &headers, &workspace_slug).await?;
    guard::require(&guard, Permission::ManageMembers)?;

    // Validate target role
    let new_role: cowiki_db::workspaces::Role = input.role.parse().map_err(|_| {
        AppError::BadRequest(format!(
            "invalid role '{}': must be owner, manager, editor, or viewer",
            input.role
        ))
    })?;

    // Cannot assign Owner role unless you are Owner
    if new_role == cowiki_db::workspaces::Role::Owner && !guard.member_role.can_transfer_ownership()
    {
        return Err(AppError::Forbidden(
            "only the owner can assign the owner role".into(),
        ));
    }

    let target_id = Uuid::parse_str(&input.user_id)
        .map_err(|_| AppError::BadRequest("invalid user_id".into()))?;

    // Cannot manage someone with equal or higher role
    let target_role_str =
        cowiki_db::workspaces::get_member_role(&state.db, guard.workspace.id, target_id)
            .await?
            .unwrap_or_default();
    let target_role: cowiki_db::workspaces::Role = target_role_str
        .parse()
        .map_err(|_| AppError::BadRequest("invalid role".into()))?;
    if !guard.member_role.can_manage_role(target_role) {
        return Err(AppError::Forbidden(
            "cannot manage a member with equal or higher role".into(),
        ));
    }

    let updated_role = cowiki_db::workspaces::change_member_role(
        &state.db,
        guard.workspace.id,
        target_id,
        &input.role,
    )
    .await?
    .ok_or_else(|| AppError::NotFound("member not found".into()))?;

    // Audit log
    cowiki_db::audit::log(
        &state.db,
        guard.workspace.id,
        guard.user.id,
        "change_member_role",
        Some("user"),
        Some(target_id),
        Some(serde_json::json!({"new_role": updated_role})),
    )
    .await?;

    let member_user = cowiki_db::users::find_by_id(&state.db, target_id)
        .await?
        .ok_or_else(|| AppError::NotFound("user not found".into()))?;

    // Fetch the member record for last_active_at + joined_via
    let member = cowiki_db::workspaces::list_members(&state.db, guard.workspace.id)
        .await?
        .into_iter()
        .find(|m| m.user_id == target_id);

    Ok(Json(MemberResponse {
        id: member_user.id.to_string(),
        name: member_user.name,
        email: member_user.email,
        role: updated_role.to_string(),
        last_active_at: member
            .as_ref()
            .and_then(|m| m.last_active_at.map(|t| t.to_rfc3339())),
        joined_via: member
            .as_ref()
            .map_or_else(|| "direct".to_string(), |m| m.joined_via.clone()),
    }))
}

// ── Workspace deletion (owner only) ──

/// Delete a workspace (Owner only).
pub async fn delete_workspace(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(workspace_slug): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let guard = guard::require_membership(&state, &headers, &workspace_slug).await?;
    guard::require(&guard, Permission::DeleteWorkspace)?;

    // Audit log (before delete so we capture it)
    cowiki_db::audit::log(
        &state.db,
        guard.workspace.id,
        guard.user.id,
        "delete_workspace",
        Some("workspace"),
        Some(guard.workspace.id),
        Some(serde_json::json!({"name": guard.workspace.name, "slug": guard.workspace.slug})),
    )
    .await?;

    cowiki_db::workspaces::delete_workspace(&state.db, guard.workspace.id).await?;

    Ok(Json(serde_json::json!({"status": "deleted"})))
}
