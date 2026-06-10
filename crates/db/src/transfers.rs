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
pub async fn find_pending_for_user(
    pool: &PgPool,
    user_id: Uuid,
) -> sqlx::Result<Vec<OwnershipTransfer>> {
    sqlx::query_as::<_, OwnershipTransfer>(
        "SELECT * FROM ownership_transfers WHERE to_user_id = $1 AND status = 'pending' ORDER BY created_at DESC"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

/// Find a transfer by ID.
pub async fn find_by_id(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<OwnershipTransfer>> {
    sqlx::query_as::<_, OwnershipTransfer>("SELECT * FROM ownership_transfers WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// Accept a transfer: update both members' roles and mark transfer accepted.
/// Returns RowNotFound if the transfer was already accepted/rejected/cancelled.
pub async fn accept_transfer(pool: &PgPool, transfer_id: Uuid) -> sqlx::Result<OwnershipTransfer> {
    let transfer = find_by_id(pool, transfer_id)
        .await?
        .ok_or_else(|| sqlx::Error::RowNotFound)?;

    if transfer.status != "pending" {
        return Err(sqlx::Error::RowNotFound);
    }

    let mut tx = pool.begin().await?;

    // Atomically claim the transfer (status check inside transaction)
    let claimed = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM ownership_transfers WHERE id = $1 AND status = 'pending' FOR UPDATE",
    )
    .bind(transfer_id)
    .fetch_optional(&mut *tx)
    .await?;

    if claimed.is_none() {
        return Err(sqlx::Error::RowNotFound);
    }

    // Demote old owner
    sqlx::query("UPDATE workspace_members SET role = $3 WHERE workspace_id = $1 AND user_id = $2")
        .bind(transfer.workspace_id)
        .bind(transfer.from_user_id)
        .bind(&transfer.previous_owner_new_role)
        .execute(&mut *tx)
        .await?;

    // Promote new owner
    sqlx::query(
        "UPDATE workspace_members SET role = 'owner' WHERE workspace_id = $1 AND user_id = $2",
    )
    .bind(transfer.workspace_id)
    .bind(transfer.to_user_id)
    .execute(&mut *tx)
    .await?;

    // Update transfer status
    let result = sqlx::query_as::<_, OwnershipTransfer>(
        "UPDATE ownership_transfers SET status = 'accepted' WHERE id = $1 AND status = 'pending' RETURNING *"
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

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_pool() -> Option<PgPool> {
        let url = std::env::var("TEST_DATABASE_URL").ok()?;
        let pool = PgPool::connect(&url).await.ok()?;
        let sql1 = include_str!("migrations/001_init.sql").replace("__EMBEDDING_DIM__", "768");
        let _ = sqlx::raw_sql(&sql1).execute(&pool).await;
        for m in [
            include_str!("migrations/002_workspaces.sql"),
            include_str!("migrations/003_workspace_visibility.sql"),
            include_str!("migrations/004_role_update.sql"),
            include_str!("migrations/005_fts.sql"),
            include_str!("migrations/006_api_keys.sql"),
            include_str!("migrations/007_team_permissions.sql"),
            include_str!("migrations/008_submission_workspace.sql"),
            include_str!("migrations/009_role_management.sql"),
        ] {
            let _ = sqlx::raw_sql(m).execute(&pool).await;
        }
        Some(pool)
    }

    async fn make_user(pool: &PgPool, name: &str) -> Uuid {
        let id = Uuid::new_v4();
        let _ = sqlx::query(
            "INSERT INTO users (id, name, email, api_key) VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING",
        )
        .bind(id).bind(name).bind(format!("{name}@test.com")).bind(format!("key-{id}"))
        .execute(pool).await;
        id
    }

    async fn make_workspace(pool: &PgPool, owner_id: Uuid) -> (Uuid, String) {
        let slug = format!(
            "ws-{}",
            Uuid::new_v4().to_string().split('-').next().unwrap()
        );
        let ws: (Uuid,) = sqlx::query_as(
            "INSERT INTO workspaces (name, slug, visibility, created_by) \
             VALUES ('Test WS', $1, 'public', $2) RETURNING id",
        )
        .bind(&slug)
        .bind(owner_id)
        .fetch_one(pool)
        .await
        .unwrap();
        (ws.0, slug)
    }

    async fn add_member_to_ws(pool: &PgPool, ws_id: Uuid, user_id: Uuid, role: &str) {
        let _ = sqlx::query(
            "INSERT INTO workspace_members (workspace_id, user_id, role, invited_by, joined_via) \
             VALUES ($1, $2, $3, $4, 'direct') ON CONFLICT DO NOTHING",
        )
        .bind(ws_id)
        .bind(user_id)
        .bind(role)
        .bind(user_id)
        .execute(pool)
        .await;
    }

    // ── Transfer Lifecycle ─────────────────────────────────────────

    #[tokio::test]
    async fn test_transfer_lifecycle_pending_to_accepted() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let owner = make_user(&pool, "owner").await;
        let target = make_user(&pool, "target").await;
        let (ws_id, _) = make_workspace(&pool, owner).await;
        add_member_to_ws(&pool, ws_id, owner, "owner").await;
        add_member_to_ws(&pool, ws_id, target, "editor").await;

        // Create transfer
        let t = create_transfer(&pool, ws_id, owner, target, "manager")
            .await
            .unwrap();
        assert_eq!(t.status, "pending");
        assert_eq!(t.previous_owner_new_role, "manager");

        // Accept
        let accepted = accept_transfer(&pool, t.id).await.unwrap();
        assert_eq!(accepted.status, "accepted");

        // Verify roles swapped
        let owner_role: (String,) = sqlx::query_as(
            "SELECT role FROM workspace_members WHERE workspace_id = $1 AND user_id = $2",
        )
        .bind(ws_id)
        .bind(owner)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(owner_role.0, "manager", "old owner should be demoted");

        let new_owner_role: (String,) = sqlx::query_as(
            "SELECT role FROM workspace_members WHERE workspace_id = $1 AND user_id = $2",
        )
        .bind(ws_id)
        .bind(target)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(new_owner_role.0, "owner", "target should be promoted");

        // Cleanup
        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(ws_id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    async fn test_transfer_reject() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let owner = make_user(&pool, "owner2").await;
        let target = make_user(&pool, "target2").await;
        let (ws_id, _) = make_workspace(&pool, owner).await;
        add_member_to_ws(&pool, ws_id, owner, "owner").await;
        add_member_to_ws(&pool, ws_id, target, "editor").await;

        let t = create_transfer(&pool, ws_id, owner, target, "viewer")
            .await
            .unwrap();
        let rejected = reject_transfer(&pool, t.id).await.unwrap();
        assert_eq!(rejected.status, "rejected");

        // Roles should be unchanged
        let owner_role: (String,) = sqlx::query_as(
            "SELECT role FROM workspace_members WHERE workspace_id = $1 AND user_id = $2",
        )
        .bind(ws_id)
        .bind(owner)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(owner_role.0, "owner");

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(ws_id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    async fn test_transfer_cancel() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let owner = make_user(&pool, "owner3").await;
        let target = make_user(&pool, "target3").await;
        let (ws_id, _) = make_workspace(&pool, owner).await;
        add_member_to_ws(&pool, ws_id, owner, "owner").await;
        add_member_to_ws(&pool, ws_id, target, "editor").await;

        let t = create_transfer(&pool, ws_id, owner, target, "viewer")
            .await
            .unwrap();
        let cancelled = cancel_transfer(&pool, t.id).await.unwrap();
        assert_eq!(cancelled.status, "cancelled");

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(ws_id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    async fn test_transfer_accept_then_accept_again_fails() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let owner = make_user(&pool, "owner4").await;
        let target = make_user(&pool, "target4").await;
        let (ws_id, _) = make_workspace(&pool, owner).await;
        add_member_to_ws(&pool, ws_id, owner, "owner").await;
        add_member_to_ws(&pool, ws_id, target, "editor").await;

        let t = create_transfer(&pool, ws_id, owner, target, "manager")
            .await
            .unwrap();
        let first = accept_transfer(&pool, t.id).await;
        assert!(first.is_ok(), "first accept should succeed");

        // Second accept should fail (status no longer pending)
        let second = accept_transfer(&pool, t.id).await;
        assert!(second.is_err(), "second accept should fail");

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(ws_id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    async fn test_transfer_reject_then_accept_fails() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let owner = make_user(&pool, "owner5").await;
        let target = make_user(&pool, "target5").await;
        let (ws_id, _) = make_workspace(&pool, owner).await;
        add_member_to_ws(&pool, ws_id, owner, "owner").await;
        add_member_to_ws(&pool, ws_id, target, "editor").await;

        let t = create_transfer(&pool, ws_id, owner, target, "viewer")
            .await
            .unwrap();
        reject_transfer(&pool, t.id).await.unwrap();
        let result = accept_transfer(&pool, t.id).await;
        assert!(result.is_err(), "cannot accept a rejected transfer");

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(ws_id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    async fn test_transfer_nonexistent_returns_error() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let result = accept_transfer(&pool, Uuid::new_v4()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_find_pending_for_user_only_returns_pending() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping: TEST_DATABASE_URL not set");
                return;
            }
        };
        let owner = make_user(&pool, "owner6").await;
        let target = make_user(&pool, "target6").await;
        let (ws_id, _) = make_workspace(&pool, owner).await;
        add_member_to_ws(&pool, ws_id, owner, "owner").await;
        add_member_to_ws(&pool, ws_id, target, "editor").await;

        let t1 = create_transfer(&pool, ws_id, owner, target, "viewer")
            .await
            .unwrap();
        let pending = find_pending_for_user(&pool, target).await.unwrap();
        assert!(pending.iter().any(|t| t.id == t1.id));

        accept_transfer(&pool, t1.id).await.unwrap();
        let pending_after = find_pending_for_user(&pool, target).await.unwrap();
        assert!(
            !pending_after.iter().any(|t| t.id == t1.id),
            "accepted transfers should not appear"
        );

        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(ws_id)
            .execute(&pool)
            .await;
    }
}
