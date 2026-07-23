mod support;

use axum::body::{Body, to_bytes};
use cowiki_cloud::auth::api_key_hash;
use cowiki_cloud::config::Config;
use cowiki_cloud::git_repo::GitRepoStore;
use http::{Request, StatusCode};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use support::TestDatabase;
use tower::ServiceExt;
use uuid::Uuid;

#[test]
fn bare_repository_reads_main_without_a_worktree() {
    let repos = tempfile::tempdir().unwrap();
    let store = GitRepoStore::new(repos.path()).unwrap();
    let space = Uuid::new_v4();
    let bare = store.ensure_space(space).unwrap();
    let commit_oid = seed_main(&bare);

    let tree = store.read_markdown_tree(space, "main").unwrap();
    assert_eq!(tree.oid, commit_oid);
    assert_eq!(
        tree.entries
            .iter()
            .map(|entry| (entry.path.as_str(), entry.kind.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("guide", "folder"),
            ("guide/setup.md", "page"),
            ("index.md", "page"),
        ]
    );

    let content = store
        .read_content_blob(space, "main", "guide/setup.md")
        .unwrap();
    assert_eq!(content.oid, commit_oid);
    assert_eq!(content.path, "guide/setup.md");
    assert_eq!(content.bytes, b"# Setup\n\nShared from Cloud main.\n");

    for invalid in [
        "",
        "/index.md",
        "guide\\setup.md",
        "guide//setup.md",
        "guide/./setup.md",
        "../index.md",
    ] {
        assert!(
            store.read_content_blob(space, "main", invalid).is_err(),
            "{invalid:?}"
        );
    }
}

#[tokio::test]
async fn members_can_read_cloud_main_but_repository_boundaries_are_preserved() {
    let Some(database) = TestDatabase::create().await else {
        eprintln!("TEST_DATABASE_URL is not set; PostgreSQL integration assertion skipped");
        return;
    };
    let owner = insert_user(&database.pool, "content-owner").await;
    let outsider = insert_user(&database.pool, "content-outsider").await;
    let owner_key = insert_api_key(&database.pool, owner).await;
    let outsider_key = insert_api_key(&database.pool, outsider).await;
    let space = Uuid::new_v4();
    cowiki_cloud::db::create_space(
        &database.pool,
        space,
        owner,
        "Content Space",
        "content-space",
    )
    .await
    .unwrap();

    let repos = tempfile::tempdir().unwrap();
    let store = GitRepoStore::new(repos.path()).unwrap();
    let bare = store.ensure_space(space).unwrap();
    let commit_oid = seed_main(&bare);
    let app = cowiki_cloud::build_router(
        test_config(repos.path().to_str().unwrap()),
        database.pool.clone(),
    )
    .unwrap();

    let tree = app
        .clone()
        .oneshot(auth_request(
            "GET",
            &format!("/api/spaces/{space}/tree?ref=main"),
            &owner_key,
        ))
        .await
        .unwrap();
    assert_eq!(tree.status(), StatusCode::OK);
    let tree = response_json(tree).await;
    assert_eq!(tree["ref"], "main");
    assert_eq!(tree["oid"], commit_oid);
    assert_eq!(
        tree["entries"],
        json!([
            { "path": "guide", "kind": "folder" },
            { "path": "guide/setup.md", "kind": "page" },
            { "path": "index.md", "kind": "page" }
        ])
    );

    let content = app
        .clone()
        .oneshot(auth_request(
            "GET",
            &format!("/api/spaces/{space}/content?ref=main&path=guide%2Fsetup.md"),
            &owner_key,
        ))
        .await
        .unwrap();
    assert_eq!(content.status(), StatusCode::OK);
    assert_eq!(
        response_json(content).await,
        json!({
            "ref": "main",
            "oid": commit_oid,
            "path": "guide/setup.md",
            "content": "# Setup\n\nShared from Cloud main.\n"
        })
    );

    let outsider_tree = app
        .clone()
        .oneshot(auth_request(
            "GET",
            &format!("/api/spaces/{space}/tree?ref=main"),
            &outsider_key,
        ))
        .await
        .unwrap();
    assert_eq!(outsider_tree.status(), StatusCode::NOT_FOUND);

    for uri in [
        format!("/api/spaces/{space}/content?ref=main&path=..%2Fsecret.md"),
        format!("/api/spaces/{space}/content?ref=main&path=%2Fetc%2Fpasswd"),
        format!("/api/spaces/{space}/content?ref=main&path=guide%5Csetup.md"),
        format!("/api/spaces/{space}/content?ref=dev&path=index.md"),
    ] {
        let response = app
            .clone()
            .oneshot(auth_request("GET", &uri, &owner_key))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{uri}");
    }

    let hidden = app
        .clone()
        .oneshot(auth_request(
            "GET",
            &format!("/api/spaces/{space}/content?ref=main&path=.secret.md"),
            &owner_key,
        ))
        .await
        .unwrap();
    assert_eq!(hidden.status(), StatusCode::NOT_FOUND);

    let binary = app
        .oneshot(auth_request(
            "GET",
            &format!("/api/spaces/{space}/content?ref=main&path=image.bin"),
            &owner_key,
        ))
        .await
        .unwrap();
    assert_eq!(binary.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);

    database.finish().await;
}

fn seed_main(bare: &Path) -> String {
    let worktree = tempfile::tempdir().unwrap();
    run(worktree.path(), &["init", "--initial-branch=main"]);
    run(worktree.path(), &["config", "user.name", "CoWiki Test"]);
    run(
        worktree.path(),
        &["config", "user.email", "test@cowiki.local"],
    );
    fs::create_dir_all(worktree.path().join("guide")).unwrap();
    fs::write(worktree.path().join("index.md"), "# Home\n").unwrap();
    fs::write(
        worktree.path().join("guide/setup.md"),
        "# Setup\n\nShared from Cloud main.\n",
    )
    .unwrap();
    fs::write(worktree.path().join("notes.txt"), "not a page\n").unwrap();
    fs::write(worktree.path().join(".secret.md"), "# Hidden\n").unwrap();
    fs::write(worktree.path().join("image.bin"), [0xff, 0xfe, 0xfd]).unwrap();
    run(worktree.path(), &["add", "."]);
    run(worktree.path(), &["commit", "-m", "seed Cloud main"]);
    let bare = bare.to_str().unwrap();
    let output = Command::new("git")
        .current_dir(worktree.path())
        .env("COWIKI_INTERNAL", "1")
        .args(["push", bare, "HEAD:refs/heads/main"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git push failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    command_stdout(worktree.path(), &["rev-parse", "HEAD"])
}

fn run(path: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(path)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn command_stdout(path: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(path)
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
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
