use sqlx::PgPool;
use uuid::Uuid;

/// Insert an audit log entry.
/// Called by route handlers after management actions.
pub async fn log(
    pool: &PgPool,
    workspace_id: Uuid,
    actor_id: Uuid,
    action: &str,
    target_type: Option<&str>,
    target_id: Option<Uuid>,
    metadata: Option<serde_json::Value>,
) -> sqlx::Result<()> {
    let meta = metadata.unwrap_or(serde_json::Value::Object(Default::default()));
    sqlx::query(
        "INSERT INTO audit_log (workspace_id, actor_id, action, target_type, target_id, metadata)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(workspace_id)
    .bind(actor_id)
    .bind(action)
    .bind(target_type)
    .bind(target_id)
    .bind(meta)
    .execute(pool)
    .await?;
    Ok(())
}
