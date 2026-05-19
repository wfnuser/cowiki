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

pub async fn get_default(pool: &PgPool) -> sqlx::Result<User> {
    sqlx::query_as::<_, User>("SELECT id, name, email, api_key FROM users WHERE name = 'default'")
        .fetch_one(pool)
        .await
}
