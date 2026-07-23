mod support;

use axum::body::{Body, to_bytes};
use cowiki_cloud::auth::api_key_hash;
use cowiki_cloud::config::Config;
use cowiki_cloud::db;
use cowiki_cloud::git_repo::GitRepoStore;
use cowiki_cloud::model::MemberRole;
use http::{Request, StatusCode};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::process::Command;
use support::TestDatabase;
use tower::ServiceExt;
use uuid::Uuid;

#[tokio::test]
async fn live_user_branch_invalidates_approval_and_merges_with_expected_head() {
    let Some(database) = TestDatabase::create().await else {
        eprintln!("TEST_DATABASE_URL is not set; PostgreSQL integration assertion skipped");
        return;
    };
    let owner = insert_user(&database.pool, "owner-pr").await;
    let manager = insert_user(&database.pool, "manager-pr").await;
    let editor = insert_user(&database.pool, "editor-pr").await;
    let owner_key = insert_api_key(&database.pool, owner).await;
    let manager_key = insert_api_key(&database.pool, manager).await;
    let editor_key = insert_api_key(&database.pool, editor).await;
    let space = Uuid::new_v4();
    db::create_space(&database.pool, space, owner, "PR Space", "pr-space")
        .await
        .unwrap();
    insert_member(&database.pool, space, manager, MemberRole::Manager).await;
    insert_member(&database.pool, space, editor, MemberRole::Editor).await;

    let repos = tempfile::tempdir().unwrap();
    let store = GitRepoStore::new(repos.path()).unwrap();
    store.ensure_space(space).unwrap();
    let working = tempfile::tempdir().unwrap();
    let (_main_oid, second_oid) = seed_user_branch(&store, space, editor, working.path());
    let app = cowiki_cloud::build_router(
        test_config(repos.path().to_str().unwrap()),
        database.pool.clone(),
    )
    .unwrap();

    let created = app
        .clone()
        .oneshot(json_request(
            "POST",
            &format!("/api/spaces/{space}/pull-requests"),
            &editor_key,
            json!({ "title": "Share findings", "body": "Ready for review" }),
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let created = response_json(created).await;
    let pr_id = created["id"].as_str().unwrap();
    assert_eq!(created["number"], 1);
    assert_eq!(created["headRef"], format!("user/{editor}"));
    assert_eq!(created["headOid"], second_oid);
    assert_eq!(created["approvalCount"], 0);

    let same = app
        .clone()
        .oneshot(json_request(
            "POST",
            &format!("/api/spaces/{space}/pull-requests"),
            &editor_key,
            json!({ "title": "Updated title" }),
        ))
        .await
        .unwrap();
    assert_eq!(same.status(), StatusCode::OK);
    assert_eq!(response_json(same).await["id"], pr_id);

    let approved = app
        .clone()
        .oneshot(auth_request(
            "POST",
            &format!("/api/spaces/{space}/pull-requests/{pr_id}/approve"),
            &manager_key,
        ))
        .await
        .unwrap();
    assert_eq!(approved.status(), StatusCode::OK);
    assert_eq!(response_json(approved).await["approvalCount"], 1);
    let parsed_pr_id = Uuid::parse_str(pr_id).unwrap();
    assert_eq!(
        db::approval_count(&database.pool, parsed_pr_id, &second_oid)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        db::approval_count(&database.pool, parsed_pr_id, &"f".repeat(40))
            .await
            .unwrap(),
        0
    );

    std::fs::write(working.path().join("index.md"), "# Three\n").unwrap();
    run(working.path(), &["commit", "-am", "three"]);
    let third_oid = rev(working.path(), "HEAD");
    run(
        working.path(),
        &["push", "cloud", &format!("HEAD:refs/heads/user/{editor}")],
    );

    let refreshed = app
        .clone()
        .oneshot(auth_request(
            "GET",
            &format!("/api/spaces/{space}/pull-requests/{pr_id}"),
            &manager_key,
        ))
        .await
        .unwrap();
    assert_eq!(refreshed.status(), StatusCode::OK);
    let refreshed = response_json(refreshed).await;
    assert_eq!(refreshed["headOid"], third_oid);
    assert_eq!(refreshed["approvalCount"], 0);

    let editor_merge = app
        .clone()
        .oneshot(json_request(
            "POST",
            &format!("/api/spaces/{space}/pull-requests/{pr_id}/merge"),
            &editor_key,
            json!({ "expectedHeadOid": third_oid }),
        ))
        .await
        .unwrap();
    assert_eq!(editor_merge.status(), StatusCode::FORBIDDEN);

    let stale_merge = app
        .clone()
        .oneshot(json_request(
            "POST",
            &format!("/api/spaces/{space}/pull-requests/{pr_id}/merge"),
            &manager_key,
            json!({ "expectedHeadOid": second_oid }),
        ))
        .await
        .unwrap();
    assert_eq!(stale_merge.status(), StatusCode::CONFLICT);

    let merged = app
        .clone()
        .oneshot(json_request(
            "POST",
            &format!("/api/spaces/{space}/pull-requests/{pr_id}/merge"),
            &manager_key,
            json!({ "expectedHeadOid": third_oid }),
        ))
        .await
        .unwrap();
    assert_eq!(merged.status(), StatusCode::OK);
    let merged = response_json(merged).await;
    assert_eq!(merged["status"], "merged");
    assert_eq!(store.ref_oid(space, "main").unwrap().unwrap(), third_oid);

    let retried = app
        .clone()
        .oneshot(json_request(
            "POST",
            &format!("/api/spaces/{space}/pull-requests/{pr_id}/merge"),
            &owner_key,
            json!({ "expectedHeadOid": third_oid }),
        ))
        .await
        .unwrap();
    assert_eq!(retried.status(), StatusCode::OK);
    assert_eq!(response_json(retried).await["status"], "merged");

    let empty = app
        .oneshot(json_request(
            "POST",
            &format!("/api/spaces/{space}/pull-requests"),
            &editor_key,
            json!({ "title": "Nothing new" }),
        ))
        .await
        .unwrap();
    assert_eq!(empty.status(), StatusCode::CONFLICT);

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

fn seed_user_branch(
    store: &GitRepoStore,
    space: Uuid,
    user: Uuid,
    working: &std::path::Path,
) -> (String, String) {
    run(working, &["init", "-b", "main"]);
    run(working, &["config", "user.name", "Test"]);
    run(working, &["config", "user.email", "test@cowiki.local"]);
    std::fs::write(working.join("index.md"), "# One\n").unwrap();
    run(working, &["add", "index.md"]);
    run(working, &["commit", "-m", "one"]);
    let main_oid = rev(working, "HEAD");
    run(
        working,
        &[
            "remote",
            "add",
            "cloud",
            store.repo_path(space).to_str().unwrap(),
        ],
    );
    run(working, &["push", "cloud", "main"]);
    std::fs::write(working.join("index.md"), "# Two\n").unwrap();
    run(working, &["commit", "-am", "two"]);
    let head_oid = rev(working, "HEAD");
    run(
        working,
        &["push", "cloud", &format!("HEAD:refs/heads/user/{user}")],
    );
    (main_oid, head_oid)
}

fn run(directory: &std::path::Path, arguments: &[&str]) {
    let output = Command::new("git")
        .args(arguments)
        .env("COWIKI_INTERNAL", "1")
        .current_dir(directory)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {} failed: {}",
        arguments.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn rev(directory: &std::path::Path, revision: &str) -> String {
    let output = Command::new("git")
        .args(["rev-parse", revision])
        .current_dir(directory)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().to_string()
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

async fn response_json(response: http::Response<Body>) -> Value {
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}
