use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{ConnectOptions, PgPool};
use std::str::FromStr;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use crate::auth::{api_key_hash, random_secret};
use crate::model::MemberRole;
use crate::model::User;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SpaceMembership {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub role: MemberRole,
}

#[derive(Debug, Clone)]
pub struct IssuedApiKey {
    pub api_key: String,
    pub user: User,
}

pub async fn connect_and_migrate(database_url: &str) -> Result<PgPool, sqlx::Error> {
    let options = PgConnectOptions::from_str(database_url)?.disable_statement_logging();
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .connect_with(options)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

pub async fn create_oauth_state(
    pool: &PgPool,
    state_hash: &[u8],
    desktop_callback: Option<&str>,
    expires_at: OffsetDateTime,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM oauth_states WHERE expires_at <= now() OR consumed_at IS NOT NULL")
        .execute(pool)
        .await?;
    sqlx::query(
        "INSERT INTO oauth_states (state_hash, desktop_callback, expires_at)
         VALUES ($1, $2, $3)",
    )
    .bind(state_hash)
    .bind(desktop_callback)
    .bind(expires_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn consume_oauth_state(
    pool: &PgPool,
    state_hash: &[u8],
) -> Result<Option<Option<String>>, sqlx::Error> {
    sqlx::query_scalar::<_, Option<String>>(
        "DELETE FROM oauth_states
         WHERE state_hash = $1 AND expires_at > now() AND consumed_at IS NULL
         RETURNING desktop_callback",
    )
    .bind(state_hash)
    .fetch_optional(pool)
    .await
}

pub async fn upsert_github_user(
    pool: &PgPool,
    github_id: i64,
    handle: &str,
    display_name: &str,
    avatar_url: Option<&str>,
) -> Result<User, sqlx::Error> {
    let id = Uuid::new_v4();
    sqlx::query_as::<_, User>(
        "INSERT INTO users (id, github_id, handle, display_name, avatar_url)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (github_id) DO UPDATE SET
             handle = EXCLUDED.handle,
             display_name = EXCLUDED.display_name,
             avatar_url = EXCLUDED.avatar_url,
             updated_at = now()
         RETURNING id, github_id, handle, display_name, avatar_url",
    )
    .bind(id)
    .bind(github_id)
    .bind(handle)
    .bind(display_name)
    .bind(avatar_url)
    .fetch_one(pool)
    .await
}

pub async fn create_desktop_exchange_code(
    pool: &PgPool,
    user_id: Uuid,
    pepper: &str,
) -> Result<String, sqlx::Error> {
    let code = random_secret("cw_once_");
    let hash = api_key_hash(&code, pepper);
    sqlx::query(
        "INSERT INTO desktop_exchange_codes (code_hash, user_id, expires_at)
         VALUES ($1, $2, $3)",
    )
    .bind(hash.as_slice())
    .bind(user_id)
    .bind(OffsetDateTime::now_utc() + Duration::seconds(60))
    .execute(pool)
    .await?;
    Ok(code)
}

pub async fn exchange_desktop_code(
    pool: &PgPool,
    raw_code: &str,
    pepper: &str,
) -> Result<Option<IssuedApiKey>, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let code_hash = api_key_hash(raw_code, pepper);
    let user_id = sqlx::query_scalar::<_, Uuid>(
        "DELETE FROM desktop_exchange_codes
         WHERE code_hash = $1 AND expires_at > now() AND consumed_at IS NULL
         RETURNING user_id",
    )
    .bind(code_hash.as_slice())
    .fetch_optional(&mut *transaction)
    .await?;
    let Some(user_id) = user_id else {
        transaction.rollback().await?;
        return Ok(None);
    };

    let api_key = random_secret("cw_key_");
    let token_hash = api_key_hash(&api_key, pepper);
    sqlx::query(
        "INSERT INTO api_keys (id, user_id, token_hash, label)
         VALUES ($1, $2, $3, 'CoWiki Desktop')",
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(token_hash.as_slice())
    .execute(&mut *transaction)
    .await?;
    let user = sqlx::query_as::<_, User>(
        "SELECT id, github_id, handle, display_name, avatar_url FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_one(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(Some(IssuedApiKey { api_key, user }))
}

pub async fn authenticate_api_key(
    pool: &PgPool,
    raw_key: &str,
    pepper: &str,
) -> Result<Option<User>, sqlx::Error> {
    let token_hash = api_key_hash(raw_key, pepper);
    let mut transaction = pool.begin().await?;
    let user = sqlx::query_as::<_, User>(
        "SELECT users.id, users.github_id, users.handle, users.display_name, users.avatar_url
         FROM api_keys
         JOIN users ON users.id = api_keys.user_id
         WHERE api_keys.token_hash = $1
           AND api_keys.revoked_at IS NULL
           AND (api_keys.expires_at IS NULL OR api_keys.expires_at > now())",
    )
    .bind(token_hash.as_slice())
    .fetch_optional(&mut *transaction)
    .await?;
    if user.is_some() {
        sqlx::query(
            "UPDATE api_keys SET last_used_at = now()
             WHERE token_hash = $1 AND revoked_at IS NULL",
        )
        .bind(token_hash.as_slice())
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;
    Ok(user)
}

pub async fn revoke_api_key(
    pool: &PgPool,
    raw_key: &str,
    pepper: &str,
) -> Result<bool, sqlx::Error> {
    let token_hash = api_key_hash(raw_key, pepper);
    let result = sqlx::query(
        "UPDATE api_keys SET revoked_at = now()
         WHERE token_hash = $1 AND revoked_at IS NULL",
    )
    .bind(token_hash.as_slice())
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn member_role(
    pool: &PgPool,
    space_id: Uuid,
    user_id: Uuid,
) -> Result<Option<MemberRole>, sqlx::Error> {
    sqlx::query_scalar::<_, MemberRole>(
        "SELECT role FROM space_members WHERE space_id = $1 AND user_id = $2",
    )
    .bind(space_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

pub async fn create_space(
    pool: &PgPool,
    space_id: Uuid,
    creator_id: Uuid,
    name: &str,
    slug: &str,
) -> Result<SpaceMembership, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    sqlx::query("INSERT INTO spaces (id, slug, name, created_by) VALUES ($1, $2, $3, $4)")
        .bind(space_id)
        .bind(slug)
        .bind(name)
        .bind(creator_id)
        .execute(&mut *transaction)
        .await?;
    sqlx::query("INSERT INTO space_members (space_id, user_id, role) VALUES ($1, $2, 'owner')")
        .bind(space_id)
        .bind(creator_id)
        .execute(&mut *transaction)
        .await?;
    sqlx::query(
        "INSERT INTO audit_events
            (space_id, actor_id, action, subject_type, subject_id, metadata)
         VALUES ($1, $2, 'space.created', 'space', $1::text, jsonb_build_object('slug', $3::text))",
    )
    .bind(space_id)
    .bind(creator_id)
    .bind(slug)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(SpaceMembership {
        id: space_id,
        name: name.to_string(),
        slug: slug.to_string(),
        role: MemberRole::Owner,
    })
}

pub async fn delete_space_after_repository_failure(
    pool: &PgPool,
    space_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM spaces WHERE id = $1")
        .bind(space_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_spaces_for_user(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Vec<SpaceMembership>, sqlx::Error> {
    sqlx::query_as::<_, SpaceMembership>(
        "SELECT spaces.id, spaces.name, spaces.slug, space_members.role
         FROM space_members
         JOIN spaces ON spaces.id = space_members.space_id
         WHERE space_members.user_id = $1
         ORDER BY spaces.created_at, spaces.name",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

pub async fn get_space_for_user(
    pool: &PgPool,
    space_id: Uuid,
    user_id: Uuid,
) -> Result<Option<SpaceMembership>, sqlx::Error> {
    sqlx::query_as::<_, SpaceMembership>(
        "SELECT spaces.id, spaces.name, spaces.slug, space_members.role
         FROM space_members
         JOIN spaces ON spaces.id = space_members.space_id
         WHERE spaces.id = $1 AND space_members.user_id = $2",
    )
    .bind(space_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
}
