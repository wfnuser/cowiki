mod support;

use axum::body::{Body, to_bytes};
use cowiki_cloud::auth::api_key_hash;
use cowiki_cloud::config::Config;
use cowiki_cloud::model::MemberRole;
use cowiki_cloud::spaces::validate_space_input;
use http::{Request, StatusCode};
use serde_json::{Value, json};
use std::collections::HashMap;
use support::TestDatabase;
use tower::ServiceExt;
use uuid::Uuid;

#[test]
fn space_names_and_slugs_are_bounded() {
    assert!(validate_space_input("Research Lab", "research-lab").is_ok());
    for (name, slug) in [
        ("", "research"),
        ("Research", "Research"),
        ("Research", "-research"),
        ("Research", "research/lab"),
        ("Research", ""),
    ] {
        assert!(
            validate_space_input(name, slug).is_err(),
            "accepted {name:?} {slug:?}"
        );
    }
}

#[test]
fn existing_role_matrix_is_preserved() {
    assert!(MemberRole::Owner.can_merge());
    assert!(MemberRole::Manager.can_merge());
    assert!(!MemberRole::Editor.can_merge());
    assert!(MemberRole::Editor.can_push());
    assert!(!MemberRole::Viewer.can_push());
}

#[tokio::test]
async fn creating_a_space_grants_owner_and_hides_it_from_non_members() {
    let Some(database) = TestDatabase::create().await else {
        eprintln!("TEST_DATABASE_URL is not set; PostgreSQL integration assertion skipped");
        return;
    };
    let owner = insert_user(&database.pool, "owner").await;
    let outsider = insert_user(&database.pool, "outsider").await;
    let owner_key = insert_api_key(&database.pool, owner).await;
    let outsider_key = insert_api_key(&database.pool, outsider).await;
    let repos = tempfile::tempdir().unwrap();
    let config = test_config(repos.path().to_str().unwrap());
    let app = cowiki_cloud::build_router(config, database.pool.clone()).unwrap();

    let created = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/spaces",
            &owner_key,
            json!({ "name": "Research Lab", "slug": "research-lab" }),
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let created = response_json(created).await;
    assert_eq!(created["role"], "owner");
    assert_eq!(created["mainRef"], "main");
    assert_eq!(created["userRef"], format!("user/{owner}"));
    assert!(created["gitUrl"].as_str().unwrap().ends_with(".git"));

    let duplicate = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/spaces",
            &owner_key,
            json!({ "name": "Other", "slug": "research-lab" }),
        ))
        .await
        .unwrap();
    assert_eq!(duplicate.status(), StatusCode::CONFLICT);

    let outsider_list = app
        .oneshot(auth_request("GET", "/api/spaces", &outsider_key))
        .await
        .unwrap();
    assert_eq!(outsider_list.status(), StatusCode::OK);
    assert_eq!(response_json(outsider_list).await, json!([]));

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

fn test_config(repo_root: &str) -> Config {
    Config::from_iter(HashMap::from([
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

async fn response_json(response: http::Response<Body>) -> Value {
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}
