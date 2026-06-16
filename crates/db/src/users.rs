use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub name: String,
    pub email: Option<String>,
    pub api_key: String,
}

/// Look up a user by the **raw** bearer token. The primary key is stored as a
/// SHA-256 hex digest (#59) — the raw value is hashed here and never persisted.
pub async fn find_by_api_key(pool: &PgPool, raw_key: &str) -> sqlx::Result<Option<User>> {
    sqlx::query_as::<_, User>("SELECT id, name, email, api_key FROM users WHERE api_key = $1")
        .bind(hash_api_key(raw_key))
        .fetch_optional(pool)
        .await
}

pub async fn find_by_name(pool: &PgPool, name: &str) -> sqlx::Result<Option<User>> {
    sqlx::query_as::<_, User>("SELECT id, name, email, api_key FROM users WHERE name = $1")
        .bind(name)
        .fetch_optional(pool)
        .await
}

pub async fn find_by_email(pool: &PgPool, email: &str) -> sqlx::Result<Option<User>> {
    sqlx::query_as::<_, User>("SELECT id, name, email, api_key FROM users WHERE email = $1")
        .bind(email)
        .fetch_optional(pool)
        .await
}

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<User>> {
    sqlx::query_as::<_, User>("SELECT id, name, email, api_key FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// Create a user. Returns the user and the **raw** API key — the only moment the
/// raw value exists; the DB stores its SHA-256 digest.
pub async fn create(
    pool: &PgPool,
    name: &str,
    email: Option<&str>,
    password_hash: Option<&str>,
) -> sqlx::Result<(User, String)> {
    let raw_key = generate_api_key();
    let user = sqlx::query_as::<_, User>(
        "INSERT INTO users (name, email, api_key, password_hash) VALUES ($1, $2, $3, $4) RETURNING id, name, email, api_key"
    )
    .bind(name)
    .bind(email)
    .bind(hash_api_key(&raw_key))
    .bind(password_hash)
    .fetch_one(pool)
    .await?;
    Ok((user, raw_key))
}

/// Rotate the primary key. Returns the user and the **raw** new key (shown once).
pub async fn regenerate_api_key(pool: &PgPool, user_id: Uuid) -> sqlx::Result<(User, String)> {
    let raw_key = generate_api_key();
    let user = sqlx::query_as::<_, User>(
        "UPDATE users SET api_key = $2 WHERE id = $1 RETURNING id, name, email, api_key",
    )
    .bind(user_id)
    .bind(hash_api_key(&raw_key))
    .fetch_one(pool)
    .await
    .map_err(|e| {
        tracing::error!("DB regenerate API key failed: {e}");
        e
    })?;
    Ok((user, raw_key))
}

pub async fn update_email(pool: &PgPool, user_id: Uuid, email: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE users SET email = $2 WHERE id = $1")
        .bind(user_id)
        .bind(email)
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(|e| {
            tracing::error!("DB update_email failed: {e}");
            e
        })
}

pub async fn get_default(pool: &PgPool) -> sqlx::Result<User> {
    sqlx::query_as::<_, User>("SELECT id, name, email, api_key FROM users WHERE name = 'default'")
        .fetch_one(pool)
        .await
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct UserSearchResult {
    pub id: Uuid,
    pub name: String,
    pub email: Option<String>,
}

/// Search users by name or email prefix (for invite autocomplete).
pub async fn search(pool: &PgPool, query: &str, limit: i64) -> sqlx::Result<Vec<UserSearchResult>> {
    let pattern = format!("{query}%");
    sqlx::query_as::<_, UserSearchResult>(
        "SELECT id, name, email FROM users \
         WHERE name ILIKE $1 OR email ILIKE $1 \
         ORDER BY name LIMIT $2",
    )
    .bind(&pattern)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| {
        tracing::error!("DB user search failed: {e}");
        e
    })
}

fn generate_api_key() -> String {
    format!("cw_{}", Uuid::new_v4().to_string().replace('-', ""))
}

/// SHA-256 hex digest used for at-rest storage of the primary API key.
///
/// No salt: API keys are `cw_<uuid-v4>` — ~122 bits of uniform entropy — so
/// rainbow tables and brute force are infeasible regardless of salting (the same
/// rationale as the secondary `api_keys` table). If the key format ever changes
/// to something lower-entropy, switch to a salted KDF here.
pub fn hash_api_key(raw_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw_key.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::hash_api_key;

    #[test]
    fn hash_is_deterministic() {
        assert_eq!(hash_api_key("cw_abc"), hash_api_key("cw_abc"));
    }

    #[test]
    fn hash_differs_for_different_keys() {
        assert_ne!(hash_api_key("cw_abc"), hash_api_key("cw_xyz"));
    }

    #[test]
    fn hash_is_sha256_hex() {
        let h = hash_api_key("cw_abc");
        assert_eq!(h.len(), 64, "sha-256 hex digest is 64 chars");
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
        // Known vector: SHA-256("cw_abc")
        assert_eq!(
            hash_api_key("cw_abc"),
            "a9a662a565ab7b0cfdb7609f7ad91d6f569b9d58ab8d7b30f6584c0e3417439a"
        );
    }

    #[test]
    fn hash_does_not_contain_raw_key() {
        // The stored digest must not leak the plaintext.
        assert!(!hash_api_key("cw_secret123").contains("cw_secret123"));
    }
}
