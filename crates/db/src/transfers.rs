use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct OwnershipTransfer {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub from_user_id: Uuid,
    pub to_user_id: Uuid,
    pub previous_owner_new_role: String,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Initiate an ownership transfer. Creates a pending record.
pub async fn create_transfer(
    pool: &PgPool,
    workspace_id: Uuid,
    from_user_id: Uuid,
    to_user_id: Uuid,
    previous_owner_new_role: &str,
) -> sqlx::Result<OwnershipTransfer> {
    sqlx::query_as::<_, OwnershipTransfer>(
        "INSERT INTO ownership_transfers (workspace_id, from_user_id, to_user_id, previous_owner_new_role)
         VALUES ($1, $2, $3, $4) RETURNING *"
    )
    .bind(workspace_id)
    .bind(from_user_id)
    .bind(to_user_id)
    .bind(previous_owner_new_role)
    .fetch_one(pool)
    .await
}

/// Get pending transfers for a user (as recipient).
pub async fn find_pending_for_user(pool: &PgPool, user_id: Uuid) -> sqlx::Result<Vec<OwnershipTransfer>> {
    sqlx::query_as::<_, OwnershipTransfer>(
        "SELECT * FROM ownership_transfers WHERE to_user_id = $1 AND status = 'pending' ORDER BY created_at DESC"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

/// Find a transfer by ID.
pub async fn find_by_id(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<OwnershipTransfer>> {
    sqlx::query_as::<_, OwnershipTransfer>(
        "SELECT * FROM ownership_transfers WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

/// Accept a transfer: update both members' roles and mark transfer accepted.
pub async fn accept_transfer(pool: &PgPool, transfer_id: Uuid) -> sqlx::Result<OwnershipTransfer> {
    let transfer = find_by_id(pool, transfer_id).await?.ok_or_else(|| {
        sqlx::Error::RowNotFound
    })?;

    if transfer.status != "pending" {
        return Err(sqlx::Error::Protocol("transfer is no longer pending".into()));
    }

    let mut tx = pool.begin().await?;

    // Demote old owner
    sqlx::query(
        "UPDATE workspace_members SET role = $3 WHERE workspace_id = $1 AND user_id = $2"
    )
    .bind(transfer.workspace_id)
    .bind(transfer.from_user_id)
    .bind(&transfer.previous_owner_new_role)
    .execute(&mut *tx)
    .await?;

    // Promote new owner
    sqlx::query(
        "UPDATE workspace_members SET role = 'owner' WHERE workspace_id = $1 AND user_id = $2"
    )
    .bind(transfer.workspace_id)
    .bind(transfer.to_user_id)
    .execute(&mut *tx)
    .await?;

    // Update transfer status
    let result = sqlx::query_as::<_, OwnershipTransfer>(
        "UPDATE ownership_transfers SET status = 'accepted' WHERE id = $1 RETURNING *"
    )
    .bind(transfer_id)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(result)
}

/// Reject a transfer.
pub async fn reject_transfer(pool: &PgPool, transfer_id: Uuid) -> sqlx::Result<OwnershipTransfer> {
    sqlx::query_as::<_, OwnershipTransfer>(
        "UPDATE ownership_transfers SET status = 'rejected' WHERE id = $1 AND status = 'pending' RETURNING *"
    )
    .bind(transfer_id)
    .fetch_one(pool)
    .await
}

/// Cancel a transfer (by the initiator).
pub async fn cancel_transfer(pool: &PgPool, transfer_id: Uuid) -> sqlx::Result<OwnershipTransfer> {
    sqlx::query_as::<_, OwnershipTransfer>(
        "UPDATE ownership_transfers SET status = 'cancelled' WHERE id = $1 AND status = 'pending' RETURNING *"
    )
    .bind(transfer_id)
    .fetch_one(pool)
    .await
}
