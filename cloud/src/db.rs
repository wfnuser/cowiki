use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{ConnectOptions, PgPool};
use std::str::FromStr;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use crate::auth::{api_key_hash, random_secret};
use crate::model::{MemberRole, PullRequestStatus, User};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SpaceMembership {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub role: MemberRole,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SpaceMemberRecord {
    pub user_id: Uuid,
    pub handle: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub role: MemberRole,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PullRequestRecord {
    pub id: Uuid,
    pub space_id: Uuid,
    pub number: i64,
    pub author_id: Uuid,
    pub title: String,
    pub body: String,
    pub base_ref: String,
    pub head_ref: String,
    pub base_oid: String,
    pub head_oid: String,
    pub status: PullRequestStatus,
    pub merged_by: Option<Uuid>,
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

pub async fn list_space_members(
    pool: &PgPool,
    space_id: Uuid,
) -> Result<Vec<SpaceMemberRecord>, sqlx::Error> {
    sqlx::query_as::<_, SpaceMemberRecord>(
        "SELECT users.id AS user_id, users.handle, users.display_name, users.avatar_url,
                space_members.role
         FROM space_members
         JOIN users ON users.id = space_members.user_id
         WHERE space_members.space_id = $1
         ORDER BY CASE space_members.role
                    WHEN 'owner' THEN 0 WHEN 'manager' THEN 1
                    WHEN 'editor' THEN 2 ELSE 3
                  END,
                  lower(users.display_name), users.id",
    )
    .bind(space_id)
    .fetch_all(pool)
    .await
}

pub async fn user_by_handle(pool: &PgPool, handle: &str) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, User>(
        "SELECT id, github_id, handle, display_name, avatar_url
         FROM users WHERE lower(handle) = lower($1)",
    )
    .bind(handle)
    .fetch_optional(pool)
    .await
}

pub async fn set_space_member(
    pool: &PgPool,
    space_id: Uuid,
    actor_id: Uuid,
    member: &User,
    role: MemberRole,
) -> Result<SpaceMemberRecord, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    sqlx::query(
        "INSERT INTO space_members (space_id, user_id, role)
         VALUES ($1, $2, $3)
         ON CONFLICT (space_id, user_id) DO UPDATE SET
             role = EXCLUDED.role, updated_at = now()",
    )
    .bind(space_id)
    .bind(member.id)
    .bind(role)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "INSERT INTO audit_events
            (space_id, actor_id, action, subject_type, subject_id, metadata)
         VALUES ($1, $2, 'space_member.updated', 'user', $3,
                 jsonb_build_object('role', $4::text))",
    )
    .bind(space_id)
    .bind(actor_id)
    .bind(member.id.to_string())
    .bind(role.as_str())
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(SpaceMemberRecord {
        user_id: member.id,
        handle: member.handle.clone(),
        display_name: member.display_name.clone(),
        avatar_url: member.avatar_url.clone(),
        role,
    })
}

pub async fn remove_space_member(
    pool: &PgPool,
    space_id: Uuid,
    actor_id: Uuid,
    member_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let result = sqlx::query("DELETE FROM space_members WHERE space_id = $1 AND user_id = $2")
        .bind(space_id)
        .bind(member_id)
        .execute(&mut *transaction)
        .await?;
    if result.rows_affected() == 1 {
        sqlx::query(
            "INSERT INTO audit_events
                (space_id, actor_id, action, subject_type, subject_id)
             VALUES ($1, $2, 'space_member.removed', 'user', $3)",
        )
        .bind(space_id)
        .bind(actor_id)
        .bind(member_id.to_string())
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;
    Ok(result.rows_affected() == 1)
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

const PR_COLUMNS: &str = "id, space_id, number, author_id, title, body, base_ref, head_ref,
     base_oid, head_oid, status, merged_by";

pub async fn create_or_update_pull_request(
    pool: &PgPool,
    space_id: Uuid,
    author_id: Uuid,
    title: &str,
    body: &str,
    base_oid: &str,
    head_oid: &str,
) -> Result<(PullRequestRecord, bool), sqlx::Error> {
    let mut transaction = pool.begin().await?;
    sqlx::query("SELECT id FROM spaces WHERE id = $1 FOR UPDATE")
        .bind(space_id)
        .fetch_one(&mut *transaction)
        .await?;
    let head_ref = format!("user/{author_id}");
    let existing = sqlx::query_as::<_, PullRequestRecord>(&format!(
        "SELECT {PR_COLUMNS} FROM pull_requests
         WHERE space_id = $1 AND head_ref = $2 AND status = 'open'"
    ))
    .bind(space_id)
    .bind(&head_ref)
    .fetch_optional(&mut *transaction)
    .await?;

    let (record, created) = if let Some(existing) = existing {
        if existing.head_oid != head_oid {
            sqlx::query("DELETE FROM pull_request_approvals WHERE pull_request_id = $1")
                .bind(existing.id)
                .execute(&mut *transaction)
                .await?;
        }
        let record = sqlx::query_as::<_, PullRequestRecord>(&format!(
            "UPDATE pull_requests SET
                title = $2, body = $3, base_oid = $4, head_oid = $5, updated_at = now()
             WHERE id = $1 RETURNING {PR_COLUMNS}"
        ))
        .bind(existing.id)
        .bind(title)
        .bind(body)
        .bind(base_oid)
        .bind(head_oid)
        .fetch_one(&mut *transaction)
        .await?;
        (record, false)
    } else {
        let number = sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(MAX(number), 0) + 1 FROM pull_requests WHERE space_id = $1",
        )
        .bind(space_id)
        .fetch_one(&mut *transaction)
        .await?;
        let id = Uuid::new_v4();
        let record = sqlx::query_as::<_, PullRequestRecord>(&format!(
            "INSERT INTO pull_requests
                (id, space_id, number, author_id, title, body, base_ref, head_ref,
                 base_oid, head_oid)
             VALUES ($1, $2, $3, $4, $5, $6, 'main', $7, $8, $9)
             RETURNING {PR_COLUMNS}"
        ))
        .bind(id)
        .bind(space_id)
        .bind(number)
        .bind(author_id)
        .bind(title)
        .bind(body)
        .bind(&head_ref)
        .bind(base_oid)
        .bind(head_oid)
        .fetch_one(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO audit_events
                (space_id, actor_id, action, subject_type, subject_id, metadata)
             VALUES ($1, $2, 'pull_request.created', 'pull_request', $3,
                     jsonb_build_object('number', $4::bigint, 'head_ref', $5::text))",
        )
        .bind(space_id)
        .bind(author_id)
        .bind(id.to_string())
        .bind(number)
        .bind(&head_ref)
        .execute(&mut *transaction)
        .await?;
        (record, true)
    };
    transaction.commit().await?;
    Ok((record, created))
}

pub async fn list_pull_requests(
    pool: &PgPool,
    space_id: Uuid,
) -> Result<Vec<PullRequestRecord>, sqlx::Error> {
    sqlx::query_as::<_, PullRequestRecord>(&format!(
        "SELECT {PR_COLUMNS} FROM pull_requests
         WHERE space_id = $1 ORDER BY number DESC"
    ))
    .bind(space_id)
    .fetch_all(pool)
    .await
}

pub async fn get_pull_request(
    pool: &PgPool,
    space_id: Uuid,
    pull_request_id: Uuid,
) -> Result<Option<PullRequestRecord>, sqlx::Error> {
    sqlx::query_as::<_, PullRequestRecord>(&format!(
        "SELECT {PR_COLUMNS} FROM pull_requests WHERE id = $1 AND space_id = $2"
    ))
    .bind(pull_request_id)
    .bind(space_id)
    .fetch_optional(pool)
    .await
}

pub async fn reconcile_pull_request_head(
    pool: &PgPool,
    record: PullRequestRecord,
    base_oid: &str,
    head_oid: &str,
) -> Result<PullRequestRecord, sqlx::Error> {
    if record.base_oid == base_oid && record.head_oid == head_oid {
        return Ok(record);
    }
    let mut transaction = pool.begin().await?;
    sqlx::query("DELETE FROM pull_request_approvals WHERE pull_request_id = $1")
        .bind(record.id)
        .execute(&mut *transaction)
        .await?;
    let record = sqlx::query_as::<_, PullRequestRecord>(&format!(
        "UPDATE pull_requests SET base_oid = $2, head_oid = $3, updated_at = now()
         WHERE id = $1 AND status = 'open' RETURNING {PR_COLUMNS}"
    ))
    .bind(record.id)
    .bind(base_oid)
    .bind(head_oid)
    .fetch_one(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(record)
}

pub async fn approval_count(
    pool: &PgPool,
    pull_request_id: Uuid,
    head_oid: &str,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM pull_request_approvals
         WHERE pull_request_id = $1 AND head_oid = $2",
    )
    .bind(pull_request_id)
    .bind(head_oid)
    .fetch_one(pool)
    .await
}

pub async fn approve_pull_request(
    pool: &PgPool,
    record: &PullRequestRecord,
    user_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO pull_request_approvals (pull_request_id, user_id, head_oid)
         VALUES ($1, $2, $3)
         ON CONFLICT (pull_request_id, user_id) DO UPDATE SET
             head_oid = EXCLUDED.head_oid, created_at = now()",
    )
    .bind(record.id)
    .bind(user_id)
    .bind(&record.head_oid)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn lock_pull_request_for_merge(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    space_id: Uuid,
    pull_request_id: Uuid,
) -> Result<Option<PullRequestRecord>, sqlx::Error> {
    sqlx::query_as::<_, PullRequestRecord>(&format!(
        "SELECT {PR_COLUMNS} FROM pull_requests
         WHERE id = $1 AND space_id = $2 FOR UPDATE"
    ))
    .bind(pull_request_id)
    .bind(space_id)
    .fetch_optional(&mut **transaction)
    .await
}

pub async fn reconcile_pull_request_head_in_transaction(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    record: &PullRequestRecord,
    base_oid: &str,
    head_oid: &str,
) -> Result<PullRequestRecord, sqlx::Error> {
    if record.base_oid == base_oid && record.head_oid == head_oid {
        return Ok(record.clone());
    }
    sqlx::query("DELETE FROM pull_request_approvals WHERE pull_request_id = $1")
        .bind(record.id)
        .execute(&mut **transaction)
        .await?;
    sqlx::query_as::<_, PullRequestRecord>(&format!(
        "UPDATE pull_requests SET base_oid = $2, head_oid = $3, updated_at = now()
         WHERE id = $1 RETURNING {PR_COLUMNS}"
    ))
    .bind(record.id)
    .bind(base_oid)
    .bind(head_oid)
    .fetch_one(&mut **transaction)
    .await
}

pub async fn mark_pull_request_merged(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    record: &PullRequestRecord,
    merged_by: Uuid,
) -> Result<PullRequestRecord, sqlx::Error> {
    let merged = sqlx::query_as::<_, PullRequestRecord>(&format!(
        "UPDATE pull_requests SET status = 'merged', merged_by = $2,
             merged_at = COALESCE(merged_at, now()), updated_at = now()
         WHERE id = $1 RETURNING {PR_COLUMNS}"
    ))
    .bind(record.id)
    .bind(merged_by)
    .fetch_one(&mut **transaction)
    .await?;
    sqlx::query(
        "INSERT INTO audit_events
            (space_id, actor_id, action, subject_type, subject_id, metadata)
         VALUES ($1, $2, 'pull_request.merged', 'pull_request', $3,
                 jsonb_build_object('number', $4::bigint, 'head_oid', $5::text))",
    )
    .bind(record.space_id)
    .bind(merged_by)
    .bind(record.id.to_string())
    .bind(record.number)
    .bind(&record.head_oid)
    .execute(&mut **transaction)
    .await?;
    Ok(merged)
}
