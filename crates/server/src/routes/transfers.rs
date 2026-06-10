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
pub struct TransferRequest {
    pub new_owner_user_id: Uuid,
    pub previous_owner_role: Option<String>, // defaults to "manager"
}

#[derive(Serialize)]
pub struct TransferResponse {
    pub id: String,
    pub status: String,
    pub from_user_id: String,
    pub to_user_id: String,
    pub previous_owner_new_role: String,
    pub created_at: String,
}

fn transfer_response(t: &cowiki_db::transfers::OwnershipTransfer) -> TransferResponse {
    TransferResponse {
        id: t.id.to_string(),
        status: t.status.clone(),
        from_user_id: t.from_user_id.to_string(),
        to_user_id: t.to_user_id.to_string(),
        previous_owner_new_role: t.previous_owner_new_role.clone(),
        created_at: t.created_at.to_rfc3339(),
    }
}

/// Initiate an ownership transfer (Owner only).
pub async fn initiate_transfer(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(workspace_slug): Path<String>,
    Json(input): Json<TransferRequest>,
) -> Result<Json<TransferResponse>> {
    let guard = guard::require_membership(&state, &headers, &workspace_slug).await?;
    guard::require(&guard, Permission::TransferOwnership)?;

    let new_role = input.previous_owner_role.as_deref().unwrap_or("manager");
    if !["manager", "editor", "viewer"].contains(&new_role) {
        return Err(AppError::BadRequest(
            "previous_owner_role must be manager, editor, or viewer".into(),
        ));
    }

    // Cannot transfer to self
    if input.new_owner_user_id == guard.user.id {
        return Err(AppError::BadRequest(
            "cannot transfer ownership to yourself".into(),
        ));
    }

    // Target must be a member
    if !cowiki_db::workspaces::is_member(&state.db, guard.workspace.id, input.new_owner_user_id)
        .await?
    {
        return Err(AppError::BadRequest(
            "target user is not a member of this workspace".into(),
        ));
    }

    let transfer = cowiki_db::transfers::create_transfer(
        &state.db,
        guard.workspace.id,
        guard.user.id,
        input.new_owner_user_id,
        new_role,
    )
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    // Audit log
    cowiki_db::audit::log(
        &state.db,
        guard.workspace.id,
        guard.user.id,
        "initiate_transfer",
        Some("ownership_transfer"),
        Some(transfer.id),
        Some(serde_json::json!({
            "new_owner": input.new_owner_user_id.to_string(),
            "previous_owner_new_role": new_role,
        })),
    )
    .await?;

    // Notify the recipient
    let ws_name = guard.workspace.name.clone();
    let from_name = guard.user.name.clone();
    let ws_id = guard.workspace.id;
    let transfer_id = transfer.id;
    let recipient_id = input.new_owner_user_id;
    tokio::spawn(async move {
        let _ = cowiki_db::notifications::create(
            &state.db,
            recipient_id,
            "ownership_transfer",
            &format!("Ownership transfer from {}", from_name),
            Some(&format!(
                "{} wants to transfer ownership of {} to you. Your new role would be owner.",
                from_name, ws_name
            )),
            Some(ws_id),
            Some(&format!("/transfers/{}", transfer_id)),
        )
        .await;
    });

    Ok(Json(transfer_response(&transfer)))
}

/// List pending ownership transfers for the current user.
pub async fn list_pending_transfers(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Vec<TransferResponse>>> {
    let user = extract_user(&state.db, &headers).await?;
    let transfers = cowiki_db::transfers::find_pending_for_user(&state.db, user.id).await?;
    Ok(Json(transfers.iter().map(transfer_response).collect()))
}

/// Accept a pending ownership transfer.
pub async fn accept_transfer(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(transfer_id): Path<Uuid>,
) -> Result<Json<TransferResponse>> {
    let user = extract_user(&state.db, &headers).await?;

    let transfer = cowiki_db::transfers::find_by_id(&state.db, transfer_id)
        .await?
        .ok_or_else(|| AppError::NotFound("transfer not found".into()))?;

    if transfer.to_user_id != user.id {
        return Err(AppError::Forbidden(
            "this transfer is for a different user".into(),
        ));
    }

    let result = cowiki_db::transfers::accept_transfer(&state.db, transfer_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // Audit log
    cowiki_db::audit::log(
        &state.db,
        transfer.workspace_id,
        user.id,
        "accept_transfer",
        Some("ownership_transfer"),
        Some(transfer_id),
        Some(serde_json::json!({"previous_owner_new_role": transfer.previous_owner_new_role})),
    )
    .await?;

    // Notify the old owner
    let old_owner_id = transfer.from_user_id;
    let ws_id = transfer.workspace_id;
    let db = state.db.clone();
    let acceptor_name = user.name.clone();
    tokio::spawn(async move {
        let _ = cowiki_db::notifications::create(
            &db,
            old_owner_id,
            "transfer_accepted",
            &format!("Ownership transferred to {}", acceptor_name),
            Some(&format!(
                "{} accepted ownership of the workspace. Your new role is {}.",
                acceptor_name, transfer.previous_owner_new_role
            )),
            Some(ws_id),
            None,
        )
        .await;
    });

    Ok(Json(transfer_response(&result)))
}

/// Reject a pending ownership transfer.
pub async fn reject_transfer(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(transfer_id): Path<Uuid>,
) -> Result<Json<TransferResponse>> {
    let user = extract_user(&state.db, &headers).await?;

    let transfer = cowiki_db::transfers::find_by_id(&state.db, transfer_id)
        .await?
        .ok_or_else(|| AppError::NotFound("transfer not found".into()))?;

    if transfer.to_user_id != user.id {
        return Err(AppError::Forbidden(
            "this transfer is for a different user".into(),
        ));
    }

    let result = cowiki_db::transfers::reject_transfer(&state.db, transfer_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(transfer_response(&result)))
}

/// Cancel a pending ownership transfer (by the initiator).
pub async fn cancel_transfer(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(transfer_id): Path<Uuid>,
) -> Result<Json<TransferResponse>> {
    let user = extract_user(&state.db, &headers).await?;

    let transfer = cowiki_db::transfers::find_by_id(&state.db, transfer_id)
        .await?
        .ok_or_else(|| AppError::NotFound("transfer not found".into()))?;

    if transfer.from_user_id != user.id {
        return Err(AppError::Forbidden(
            "only the initiator can cancel the transfer".into(),
        ));
    }

    let result = cowiki_db::transfers::cancel_transfer(&state.db, transfer_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(transfer_response(&result)))
}
