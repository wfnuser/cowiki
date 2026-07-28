mod support;

use axum::body::{Body, to_bytes};
use cowiki_cloud::auth::api_key_hash;
use cowiki_cloud::config::Config;
use cowiki_cloud::invitations::validate_invitation_input;
use cowiki_cloud::model::MemberRole;
use http::{Request, StatusCode};
use serde_json::{Value, json};
use std::collections::HashMap;
use support::TestDatabase;
use time::OffsetDateTime;
use tower::ServiceExt;
use uuid::Uuid;

#[test]
fn invitation_roles_and_expiry_are_bounded() {
    assert!(validate_invitation_input(MemberRole::Editor, 168).is_ok());
    assert!(validate_invitation_input(MemberRole::Viewer, 1).is_ok());
    assert!(validate_invitation_input(MemberRole::Owner, 24).is_err());
    assert!(validate_invitation_input(MemberRole::Manager, 0).is_err());
    assert!(validate_invitation_input(MemberRole::Manager, 721).is_err());
}

#[tokio::test]
async fn invitations_are_space_scoped_revocable_and_preserve_existing_roles() {
    let Some(database) = TestDatabase::create().await else {
        eprintln!("TEST_DATABASE_URL is not set; PostgreSQL integration assertion skipped");
        return;
    };
    let owner = insert_user(&database.pool, "invite-owner").await;
    let manager = insert_user(&database.pool, "invite-manager").await;
    let editor = insert_user(&database.pool, "invite-editor").await;
    let candidate = insert_user(&database.pool, "invite-candidate").await;
    let owner_key = insert_api_key(&database.pool, owner).await;
    let manager_key = insert_api_key(&database.pool, manager).await;
    let editor_key = insert_api_key(&database.pool, editor).await;
    let candidate_key = insert_api_key(&database.pool, candidate).await;
    let space = Uuid::new_v4();
    cowiki_cloud::db::create_space(&database.pool, space, owner, "Competition", "competition")
        .await
        .unwrap();
    insert_member(&database.pool, space, manager, MemberRole::Manager).await;
    insert_member(&database.pool, space, editor, MemberRole::Editor).await;
    let repos = tempfile::tempdir().unwrap();
    let app = cowiki_cloud::build_router(
        test_config(repos.path().to_str().unwrap()),
        database.pool.clone(),
    )
    .unwrap();

    let forbidden = app
        .clone()
        .oneshot(json_request(
            "POST",
            &format!("/api/spaces/{space}/invitations"),
            &editor_key,
            json!({ "role": "viewer", "expiresInHours": 24 }),
        ))
        .await
        .unwrap();
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);

    let owner_role = app
        .clone()
        .oneshot(json_request(
            "POST",
            &format!("/api/spaces/{space}/invitations"),
            &owner_key,
            json!({ "role": "owner", "expiresInHours": 24 }),
        ))
        .await
        .unwrap();
    assert_eq!(owner_role.status(), StatusCode::BAD_REQUEST);

    let created = app
        .clone()
        .oneshot(json_request(
            "POST",
            &format!("/api/spaces/{space}/invitations"),
            &manager_key,
            json!({ "role": "editor", "expiresInHours": 24 }),
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let created = response_json(created).await;
    let invitation_id = created["id"].as_str().unwrap();
    let token = created["token"].as_str().unwrap();
    assert_eq!(created["spaceId"], space.to_string());
    assert_eq!(created["role"], "editor");
    assert_eq!(
        created["inviteUrl"],
        format!("https://cloud.cowiki.test/invite/{token}")
    );

    let preview = app
        .clone()
        .oneshot(public_request("GET", &format!("/api/invitations/{token}")))
        .await
        .unwrap();
    assert_eq!(preview.status(), StatusCode::OK);
    let preview = response_json(preview).await;
    assert_eq!(preview["spaceName"], "Competition");
    assert_eq!(preview["spaceSlug"], "competition");
    assert_eq!(preview["role"], "editor");

    let accepted = app
        .clone()
        .oneshot(auth_request(
            "POST",
            &format!("/api/invitations/{token}/accept"),
            &candidate_key,
        ))
        .await
        .unwrap();
    assert_eq!(accepted.status(), StatusCode::OK);
    let accepted = response_json(accepted).await;
    assert_eq!(accepted["id"], space.to_string());
    assert_eq!(accepted["role"], "editor");

    let accepted_again = app
        .clone()
        .oneshot(auth_request(
            "POST",
            &format!("/api/invitations/{token}/accept"),
            &candidate_key,
        ))
        .await
        .unwrap();
    assert_eq!(accepted_again.status(), StatusCode::OK);
    let accepted_count: i32 =
        sqlx::query_scalar("SELECT accepted_count FROM space_invitations WHERE id = $1")
            .bind(Uuid::parse_str(invitation_id).unwrap())
            .fetch_one(&database.pool)
            .await
            .unwrap();
    assert_eq!(accepted_count, 1);

    let viewer_invite = app
        .clone()
        .oneshot(json_request(
            "POST",
            &format!("/api/spaces/{space}/invitations"),
            &owner_key,
            json!({ "role": "viewer", "expiresInHours": 24 }),
        ))
        .await
        .unwrap();
    let viewer_invite = response_json(viewer_invite).await;
    let viewer_token = viewer_invite["token"].as_str().unwrap();
    let manager_accepts = app
        .clone()
        .oneshot(auth_request(
            "POST",
            &format!("/api/invitations/{viewer_token}/accept"),
            &manager_key,
        ))
        .await
        .unwrap();
    assert_eq!(manager_accepts.status(), StatusCode::OK);
    assert_eq!(response_json(manager_accepts).await["role"], "manager");

    let listed = app
        .clone()
        .oneshot(auth_request(
            "GET",
            &format!("/api/spaces/{space}/invitations"),
            &owner_key,
        ))
        .await
        .unwrap();
    assert_eq!(listed.status(), StatusCode::OK);
    let listed = response_json(listed).await;
    assert_eq!(listed.as_array().unwrap().len(), 2);
    assert!(listed[0].get("token").is_none());

    let revoked = app
        .clone()
        .oneshot(auth_request(
            "DELETE",
            &format!("/api/spaces/{space}/invitations/{invitation_id}"),
            &manager_key,
        ))
        .await
        .unwrap();
    assert_eq!(revoked.status(), StatusCode::NO_CONTENT);
    let after_revoke = app
        .clone()
        .oneshot(public_request("GET", &format!("/api/invitations/{token}")))
        .await
        .unwrap();
    assert_eq!(after_revoke.status(), StatusCode::NOT_FOUND);

    sqlx::query("UPDATE space_invitations SET expires_at = now() - interval '1 minute'")
        .execute(&database.pool)
        .await
        .unwrap();
    let expired = app
        .oneshot(public_request(
            "GET",
            &format!("/api/invitations/{viewer_token}"),
        ))
        .await
        .unwrap();
    assert_eq!(expired.status(), StatusCode::NOT_FOUND);

    let audit_actions: Vec<String> = sqlx::query_scalar(
        "SELECT action FROM audit_events
         WHERE space_id = $1 AND action LIKE 'space_invitation.%' ORDER BY id",
    )
    .bind(space)
    .fetch_all(&database.pool)
    .await
    .unwrap();
    assert!(audit_actions.contains(&"space_invitation.created".to_string()));
    assert!(audit_actions.contains(&"space_invitation.accepted".to_string()));
    assert!(audit_actions.contains(&"space_invitation.revoked".to_string()));

    database.finish().await;
}

#[tokio::test]
async fn concurrent_invitations_create_one_membership() {
    let Some(database) = TestDatabase::create().await else {
        eprintln!("TEST_DATABASE_URL is not set; PostgreSQL integration assertion skipped");
        return;
    };
    let owner = insert_user(&database.pool, "concurrent-owner").await;
    let candidate = insert_user(&database.pool, "concurrent-candidate").await;
    let space = Uuid::new_v4();
    cowiki_cloud::db::create_space(
        &database.pool,
        space,
        owner,
        "Concurrent Invitations",
        "concurrent-invitations",
    )
    .await
    .unwrap();
    let first_hash = api_key_hash(
        "cw_invite_concurrent_first",
        "0123456789abcdef0123456789abcdef",
    );
    let second_hash = api_key_hash(
        "cw_invite_concurrent_second",
        "0123456789abcdef0123456789abcdef",
    );
    let expires_at = OffsetDateTime::now_utc() + time::Duration::hours(1);
    cowiki_cloud::db::create_space_invitation(
        &database.pool,
        Uuid::new_v4(),
        space,
        owner,
        MemberRole::Editor,
        &first_hash,
        expires_at,
    )
    .await
    .unwrap();
    cowiki_cloud::db::create_space_invitation(
        &database.pool,
        Uuid::new_v4(),
        space,
        owner,
        MemberRole::Editor,
        &second_hash,
        expires_at,
    )
    .await
    .unwrap();

    let first = cowiki_cloud::db::accept_space_invitation(&database.pool, &first_hash, candidate);
    let second = cowiki_cloud::db::accept_space_invitation(&database.pool, &second_hash, candidate);
    let (first, second) = tokio::join!(first, second);
    assert_eq!(first.unwrap().unwrap().role, MemberRole::Editor);
    assert_eq!(second.unwrap().unwrap().role, MemberRole::Editor);
    let membership_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM space_members WHERE space_id = $1 AND user_id = $2",
    )
    .bind(space)
    .bind(candidate)
    .fetch_one(&database.pool)
    .await
    .unwrap();
    let accepted_count: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(accepted_count), 0) FROM space_invitations WHERE space_id = $1",
    )
    .bind(space)
    .fetch_one(&database.pool)
    .await
    .unwrap();
    assert_eq!(membership_count, 1);
    assert_eq!(accepted_count, 1);

    database.finish().await;
}

async fn insert_user(pool: &sqlx::PgPool, handle: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, github_id, handle, display_name) VALUES ($1, $2, $3, $3)")
        .bind(id)
        .bind((id.as_u128() & i64::MAX as u128) as i64)
        .bind(handle)
        .execute(pool)
        .await
        .unwrap();
    id
}

async fn insert_api_key(pool: &sqlx::PgPool, user_id: Uuid) -> String {
    let key = format!("cw_key_{user_id}");
    let hash = api_key_hash(&key, "0123456789abcdef0123456789abcdef");
    sqlx::query(
        "INSERT INTO api_keys (id, user_id, token_hash, label) VALUES ($1, $2, $3, 'test')",
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(hash.as_slice())
    .execute(pool)
    .await
    .unwrap();
    key
}

async fn insert_member(pool: &sqlx::PgPool, space: Uuid, user: Uuid, role: MemberRole) {
    sqlx::query("INSERT INTO space_members (space_id, user_id, role) VALUES ($1, $2, $3)")
        .bind(space)
        .bind(user)
        .bind(role)
        .execute(pool)
        .await
        .unwrap();
}

fn test_config(repo_root: &str) -> Config {
    Config::from_values(HashMap::from([
        (
            "DATABASE_URL".into(),
            "postgres://postgres:cowiki@127.0.0.1:55432/postgres".into(),
        ),
        ("COWIKI_REPO_ROOT".into(), repo_root.into()),
        (
            "COWIKI_PUBLIC_ORIGIN".into(),
            "https://cloud.cowiki.test".into(),
        ),
        ("GITHUB_CLIENT_ID".into(), "test".into()),
        ("GITHUB_CLIENT_SECRET".into(), "test".into()),
        (
            "COWIKI_TOKEN_PEPPER".into(),
            "0123456789abcdef0123456789abcdef".into(),
        ),
    ]))
    .unwrap()
}

fn json_request(method: &str, uri: &str, token: &str, value: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(value.to_string()))
        .unwrap()
}

fn auth_request(method: &str, uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

fn public_request(method: &str, uri: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

async fn response_json(response: http::Response<Body>) -> Value {
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}
