use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::{AppError, Result};
use crate::routes::auth::extract_user;
use crate::AppState;

#[derive(Deserialize)]
pub struct CreateWorkspaceRequest {
    pub name: String,
    pub slug: String,
}

#[derive(Serialize)]
pub struct WorkspaceResponse {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub role: String,
}

/// Create a new workspace
pub async fn create_workspace(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(input): Json<CreateWorkspaceRequest>,
) -> Result<Json<WorkspaceResponse>> {
    let user = extract_user(&state.db, &headers).await?;

    // Validate slug (alphanumeric + hyphens only)
    if !input.slug.chars().all(|c| c.is_alphanumeric() || c == '-') {
        return Err(AppError::BadRequest("slug must be alphanumeric with hyphens".into()));
    }

    let ws = cowiki_db::workspaces::create(&state.db, &input.name, &input.slug, user.id)
        .await
        .map_err(|e| {
            if e.to_string().contains("duplicate") {
                AppError::BadRequest("workspace slug already taken".into())
            } else {
                AppError::Internal(e.to_string())
            }
        })?;

    Ok(Json(WorkspaceResponse {
        id: ws.id.to_string(),
        name: ws.name,
        slug: ws.slug,
        role: "owner".into(),
    }))
}

/// List workspaces the current user belongs to
pub async fn list_workspaces(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Vec<WorkspaceResponse>>> {
    let user = extract_user(&state.db, &headers).await?;
    let workspaces = cowiki_db::workspaces::list_for_user(&state.db, user.id).await?;

    let mut result = Vec::new();
    for ws in workspaces {
        let role = if ws.created_by == user.id { "owner" } else { "member" };
        result.push(WorkspaceResponse {
            id: ws.id.to_string(),
            name: ws.name,
            slug: ws.slug,
            role: role.into(),
        });
    }
    Ok(Json(result))
}

#[derive(Deserialize)]
pub struct InviteRequest {
    pub email: String,
}

#[derive(Serialize)]
pub struct InviteResponse {
    pub invitation_id: String,
    pub email: String,
    pub workspace: String,
}

/// Invite someone to a workspace by email
pub async fn invite(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(workspace_slug): Path<String>,
    Json(input): Json<InviteRequest>,
) -> Result<Json<InviteResponse>> {
    let user = extract_user(&state.db, &headers).await?;

    let ws = cowiki_db::workspaces::find_by_slug(&state.db, &workspace_slug)
        .await?
        .ok_or_else(|| AppError::NotFound("workspace not found".into()))?;

    // Check caller is a member
    if !cowiki_db::workspaces::is_member(&state.db, ws.id, user.id).await? {
        return Err(AppError::BadRequest("you are not a member of this workspace".into()));
    }

    let invitation = cowiki_db::workspaces::create_invitation(&state.db, ws.id, &input.email, user.id).await?;

    // If the invited user already exists, auto-add them
    if let Some(invited_user) = cowiki_db::users::find_by_email(&state.db, &input.email).await? {
        cowiki_db::workspaces::add_member(&state.db, ws.id, invited_user.id, "member", user.id).await?;
        cowiki_db::workspaces::accept_invitation(&state.db, invitation.id).await?;

        // Create their branch
        state.wiki_repo
            .ensure_user_branch(&invited_user.id.to_string())
            .map_err(|e| AppError::Internal(e.to_string()))?;
    }

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
        return Err(AppError::BadRequest("you are not a member".into()));
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
