use sqlx::PgPool;
use uuid::Uuid;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub name: String,
    pub email: Option<String>,
    pub api_key: String,
}

pub async fn find_by_api_key(pool: &PgPool, api_key: &str) -> sqlx::Result<Option<User>> {
    sqlx::query_as::<_, User>("SELECT id, name, email, api_key FROM users WHERE api_key = $1")
        .bind(api_key)
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

pub async fn create(pool: &PgPool, name: &str, email: Option<&str>, password_hash: Option<&str>) -> sqlx::Result<User> {
    let api_key = generate_api_key();
    sqlx::query_as::<_, User>(
        "INSERT INTO users (name, email, api_key, password_hash) VALUES ($1, $2, $3, $4) RETURNING id, name, email, api_key"
    )
    .bind(name)
    .bind(email)
    .bind(&api_key)
    .bind(password_hash)
    .fetch_one(pool)
    .await
}

pub async fn regenerate_api_key(pool: &PgPool, user_id: Uuid) -> sqlx::Result<User> {
    let api_key = generate_api_key();
    sqlx::query_as::<_, User>(
        "UPDATE users SET api_key = $2 WHERE id = $1 RETURNING id, name, email, api_key"
    )
    .bind(user_id)
    .bind(&api_key)
    .fetch_one(pool)
    .await
    .map_err(|e| { tracing::error!("DB regenerate API key failed: {e}"); e })
}

pub async fn get_default(pool: &PgPool) -> sqlx::Result<User> {
    sqlx::query_as::<_, User>("SELECT id, name, email, api_key FROM users WHERE name = 'default'")
        .fetch_one(pool)
        .await
}

fn generate_api_key() -> String {
    format!("cw_{}", Uuid::new_v4().to_string().replace('-', ""))
}
