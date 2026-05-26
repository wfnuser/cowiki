use sqlx::PgPool;
use uuid::Uuid;
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ApiKey {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub key_prefix: String,
    pub last_used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

/// Returned once on creation — contains the raw key value.
#[derive(Debug, Clone, Serialize)]
pub struct ApiKeyCreated {
    pub id: Uuid,
    pub name: String,
    pub key_prefix: String,
    pub raw_key: String,
    pub created_at: DateTime<Utc>,
}

pub async fn create(pool: &PgPool, user_id: Uuid, name: &str) -> sqlx::Result<ApiKeyCreated> {
    let raw_key = generate_api_key();
    let key_prefix = key_prefix(&raw_key);
    let key_hash = sha256_hash(&raw_key);

    let row = sqlx::query_as::<_, ApiKey>(
        "INSERT INTO api_keys (user_id, name, key_prefix, key_hash) VALUES ($1, $2, $3, $4) RETURNING id, user_id, name, key_prefix, last_used_at, created_at, revoked_at"
    )
    .bind(user_id)
    .bind(name)
    .bind(&key_prefix)
    .bind(&key_hash)
    .fetch_one(pool)
    .await
    .map_err(|e| { tracing::error!("DB create API key failed: {e}"); e })?;

    Ok(ApiKeyCreated {
        id: row.id,
        name: row.name,
        key_prefix,
        raw_key,
        created_at: row.created_at,
    })
}

/// List active (non-revoked) keys for a user, newest first.
pub async fn list_for_user(pool: &PgPool, user_id: Uuid) -> sqlx::Result<Vec<ApiKey>> {
    sqlx::query_as::<_, ApiKey>(
        "SELECT id, user_id, name, key_prefix, last_used_at, created_at, revoked_at FROM api_keys WHERE user_id = $1 AND revoked_at IS NULL ORDER BY created_at DESC"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|e| { tracing::error!("DB list API keys failed: {e}"); e })
}

/// Revoke a key (soft-delete). Returns true if a key was revoked.
pub async fn revoke(pool: &PgPool, key_id: Uuid, user_id: Uuid) -> sqlx::Result<bool> {
    let result = sqlx::query(
        "UPDATE api_keys SET revoked_at = now() WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL"
    )
    .bind(key_id)
    .bind(user_id)
    .execute(pool)
    .await
    .map_err(|e| { tracing::error!("DB revoke API key failed: {e}"); e })?;

    Ok(result.rows_affected() > 0)
}

/// Look up an active key by its SHA-256 hash. Returns (ApiKey, user_id) or None.
/// Used by the auth fallback path — MUST filter revoked_at IS NULL.
pub async fn find_by_key_hash(pool: &PgPool, key_hash: &str) -> sqlx::Result<Option<(ApiKey, Uuid)>> {
    sqlx::query_as::<_, ApiKey>(
        "SELECT id, user_id, name, key_prefix, last_used_at, created_at, revoked_at FROM api_keys WHERE key_hash = $1 AND revoked_at IS NULL"
    )
    .bind(key_hash)
    .fetch_optional(pool)
    .await
    .map(|opt| opt.map(|k| {
        let uid = k.user_id;
        (k, uid)
    }))
    .map_err(|e| { tracing::error!("DB find_by_key_hash failed: {e}"); e })
}

/// Update last_used_at when a key successfully authenticates.
pub async fn touch_last_used(pool: &PgPool, key_hash: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE api_keys SET last_used_at = now() WHERE key_hash = $1 AND revoked_at IS NULL")
        .bind(key_hash)
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(|e| { tracing::error!("DB touch_last_used failed: {e}"); e })
}

// ── Helpers ──

pub fn mask_key_prefix(prefix: &str) -> String {
    // prefix is "cw_xxxxxxxx" (11 chars). Show "cw_****xxxx"
    if prefix.len() >= 7 {
        format!("{}****{}", &prefix[..3], &prefix[prefix.len()-4..])
    } else {
        format!("{}****", prefix)
    }
}

fn generate_api_key() -> String {
    format!("cw_{}", Uuid::new_v4().to_string().replace('-', ""))
}

fn key_prefix(raw_key: &str) -> String {
    // Store first 8 characters of the raw key
    raw_key.chars().take(8).collect()
}

fn sha256_hash(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}
