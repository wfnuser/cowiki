mod support;

use axum::body::{Body, to_bytes};
use cowiki_cloud::auth::{
    OAUTH_STATE_TTL, api_key_hash, random_secret, validate_loopback_callback, verify_api_key,
};
use cowiki_cloud::config::Config;
use http::{Request, StatusCode};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use support::TestDatabase;
use tower::ServiceExt;

#[test]
fn desktop_callback_accepts_only_an_exact_loopback_url() {
    assert!(validate_loopback_callback("http://127.0.0.1:39281/auth/callback").is_ok());
    for value in [
        "https://evil.example/auth/callback",
        "http://localhost:39281/auth/callback",
        "http://127.0.0.1/auth/callback",
        "http://127.0.0.1:39281/other",
        "http://127.0.0.1:39281/auth/callback?next=evil",
        "file:///tmp/callback",
    ] {
        assert!(
            validate_loopback_callback(value).is_err(),
            "accepted {value}"
        );
    }
}

#[test]
fn api_keys_are_peppered_and_compared_without_plaintext() {
    let pepper = "0123456789abcdef0123456789abcdef";
    let token = "cw_key_secret-value";
    let stored = api_key_hash(token, pepper);

    assert_ne!(stored.as_slice(), token.as_bytes());
    assert!(verify_api_key(token, pepper, &stored));
    assert!(!verify_api_key("cw_key_other", pepper, &stored));
    assert!(!verify_api_key(
        token,
        "fedcba9876543210fedcba9876543210",
        &stored
    ));
}

#[test]
fn generated_secrets_have_domain_specific_prefixes_and_entropy() {
    let first = random_secret("cw_state_");
    let second = random_secret("cw_state_");
    assert!(first.starts_with("cw_state_"));
    assert!(first.len() >= 50);
    assert_ne!(first, second);
}

#[test]
fn oauth_state_lifetime_is_ten_minutes() {
    assert_eq!(OAUTH_STATE_TTL, Duration::from_secs(600));
    let issued = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
    assert!(issued + OAUTH_STATE_TTL > issued);
}

#[tokio::test]
async fn exchange_codes_are_single_use_and_logout_revokes_the_api_key() {
    let Some(database) = TestDatabase::create().await else {
        eprintln!("TEST_DATABASE_URL is not set; PostgreSQL integration assertion skipped");
        return;
    };
    let user = cowiki_cloud::db::upsert_github_user(
        &database.pool,
        991_337,
        "auth-user",
        "Auth User",
        None,
    )
    .await
    .unwrap();
    let code = cowiki_cloud::db::create_desktop_exchange_code(
        &database.pool,
        user.id,
        "0123456789abcdef0123456789abcdef",
    )
    .await
    .unwrap();
    let repos = tempfile::tempdir().unwrap();
    let app = cowiki_cloud::build_router(
        test_config(repos.path().to_str().unwrap()),
        database.pool.clone(),
    )
    .unwrap();

    let exchanged = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/auth/exchange",
            json!({ "code": code, "client": "web" }),
        ))
        .await
        .unwrap();
    assert_eq!(exchanged.status(), StatusCode::OK);
    let credential = response_json(exchanged).await;
    let api_key = credential["apiKey"].as_str().unwrap();

    let replayed = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/auth/exchange",
            json!({ "code": code, "client": "web" }),
        ))
        .await
        .unwrap();
    assert_eq!(replayed.status(), StatusCode::BAD_REQUEST);

    let authenticated = app
        .clone()
        .oneshot(auth_request("GET", "/api/me", api_key))
        .await
        .unwrap();
    assert_eq!(authenticated.status(), StatusCode::OK);
    let logged_out = app
        .clone()
        .oneshot(auth_request("POST", "/api/auth/logout", api_key))
        .await
        .unwrap();
    assert_eq!(logged_out.status(), StatusCode::NO_CONTENT);
    let revoked = app
        .oneshot(auth_request("GET", "/api/me", api_key))
        .await
        .unwrap();
    assert_eq!(revoked.status(), StatusCode::UNAUTHORIZED);

    database.finish().await;
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

fn json_request(method: &str, uri: &str, value: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
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
