use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

use crate::AppState;
use crate::auth::{AuthenticatedUser, api_key_hash, random_secret};
use crate::db;
use crate::error::{AppError, AppResult};
use crate::model::{MemberRole, SpaceInvitation, SpaceInvitationPreview};
use crate::spaces::{CloudSpace, space_response};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateInvitationRequest {
    role: MemberRole,
    expires_in_hours: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InvitationResponse {
    id: Uuid,
    space_id: Uuid,
    role: MemberRole,
    expires_at: String,
    accepted_count: i32,
    created_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreatedInvitationResponse {
    id: Uuid,
    space_id: Uuid,
    role: MemberRole,
    expires_at: String,
    accepted_count: i32,
    created_at: String,
    token: String,
    invite_url: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InvitationPreviewResponse {
    space_id: Uuid,
    space_name: String,
    space_slug: String,
    role: MemberRole,
    expires_at: String,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/invitations/{token}", get(preview_invitation))
        .route("/api/invitations/{token}/accept", post(accept_invitation))
        .route(
            "/api/spaces/{space_id}/invitations",
            get(list_invitations).post(create_invitation),
        )
        .route(
            "/api/spaces/{space_id}/invitations/{invitation_id}",
            delete(revoke_invitation),
        )
}

pub fn validate_invitation_input(role: MemberRole, expires_in_hours: i64) -> Result<(), String> {
    if role == MemberRole::Owner {
        return Err("invitation cannot grant the owner role".into());
    }
    if !(1..=720).contains(&expires_in_hours) {
        return Err("invitation expiry must be between 1 and 720 hours".into());
    }
    Ok(())
}

async fn preview_invitation(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> AppResult<Json<InvitationPreviewResponse>> {
    let token_hash = invitation_hash(&state, &token)?;
    let invitation = db::preview_space_invitation(&state.pool, &token_hash)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(preview_response(invitation)?))
}

async fn accept_invitation(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(token): Path<String>,
) -> AppResult<Json<CloudSpace>> {
    let token_hash = invitation_hash(&state, &token)?;
    let membership = db::accept_space_invitation(&state.pool, &token_hash, user.user.id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(space_response(&state, membership, user.user.id)?))
}

async fn list_invitations(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(space_id): Path<Uuid>,
) -> AppResult<Json<Vec<InvitationResponse>>> {
    require_manager(&state, space_id, user.user.id).await?;
    let invitations = db::list_space_invitations(&state.pool, space_id)
        .await?
        .into_iter()
        .map(invitation_response)
        .collect::<AppResult<Vec<_>>>()?;
    Ok(Json(invitations))
}

async fn create_invitation(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(space_id): Path<Uuid>,
    Json(input): Json<CreateInvitationRequest>,
) -> AppResult<(StatusCode, Json<CreatedInvitationResponse>)> {
    require_manager(&state, space_id, user.user.id).await?;
    validate_invitation_input(input.role, input.expires_in_hours).map_err(AppError::BadRequest)?;
    let invitation_id = Uuid::new_v4();
    let token = random_secret("cw_invite_");
    let token_hash = api_key_hash(&token, &state.config.token_pepper);
    let expires_at = OffsetDateTime::now_utc() + time::Duration::hours(input.expires_in_hours);
    let invitation = db::create_space_invitation(
        &state.pool,
        invitation_id,
        space_id,
        user.user.id,
        input.role,
        &token_hash,
        expires_at,
    )
    .await?;
    let invite_url = state
        .config
        .public_origin
        .join(&format!("/invite/{token}"))
        .map_err(|error| AppError::Internal(error.to_string()))?
        .to_string();
    Ok((
        StatusCode::CREATED,
        Json(CreatedInvitationResponse {
            id: invitation.id,
            space_id: invitation.space_id,
            role: invitation.role,
            expires_at: format_time(invitation.expires_at)?,
            accepted_count: invitation.accepted_count,
            created_at: format_time(invitation.created_at)?,
            token,
            invite_url,
        }),
    ))
}

async fn revoke_invitation(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path((space_id, invitation_id)): Path<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    require_manager(&state, space_id, user.user.id).await?;
    if db::revoke_space_invitation(&state.pool, space_id, invitation_id, user.user.id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound)
    }
}

async fn require_manager(state: &AppState, space_id: Uuid, user_id: Uuid) -> AppResult<()> {
    let role = db::member_role(&state.pool, space_id, user_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if role.can_merge() {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

fn invitation_hash(state: &AppState, token: &str) -> AppResult<[u8; 32]> {
    let token = token.trim();
    if !token.starts_with("cw_invite_") || token.len() > 128 {
        return Err(AppError::NotFound);
    }
    Ok(api_key_hash(token, &state.config.token_pepper))
}

fn invitation_response(invitation: SpaceInvitation) -> AppResult<InvitationResponse> {
    Ok(InvitationResponse {
        id: invitation.id,
        space_id: invitation.space_id,
        role: invitation.role,
        expires_at: format_time(invitation.expires_at)?,
        accepted_count: invitation.accepted_count,
        created_at: format_time(invitation.created_at)?,
    })
}

fn preview_response(invitation: SpaceInvitationPreview) -> AppResult<InvitationPreviewResponse> {
    Ok(InvitationPreviewResponse {
        space_id: invitation.space_id,
        space_name: invitation.space_name,
        space_slug: invitation.space_slug,
        role: invitation.role,
        expires_at: format_time(invitation.expires_at)?,
    })
}

fn format_time(value: OffsetDateTime) -> AppResult<String> {
    value
        .format(&Rfc3339)
        .map_err(|error| AppError::Internal(error.to_string()))
}
