// ── Read-path authorization tests (#55) ──────────────────────────────
// Black-box HTTP tests against a running server (same harness style as
// permission_api_tests.rs). They self-skip unless TEST_API_URL is set, e.g.:
//   TEST_API_URL=http://localhost:3000/api cargo test -p cowiki-server
//
// Cover the gates added in PR #87: read routes require membership, and the
// `branch` parameter can only be `main` or the caller's OWN draft branch —
// never another member's `user/{uuid}` draft.

use std::process::Command;

fn api_request(method: &str, path: &str, body: Option<&str>, api_key: &str) -> (String, i32) {
    let mut args = vec!["-s", "-o", "/dev/stderr", "-w", "%{http_code}"];
    if let Some(b) = body {
        args.push("-d");
        args.push(b);
    }
    args.push("-H");
    let auth_header = format!("Authorization: Bearer {}", api_key);
    args.push(&auth_header);
    args.push("-H");
    args.push("Content-Type: application/json");
    args.push("-X");
    args.push(method);

    let base_url =
        std::env::var("TEST_API_URL").unwrap_or_else(|_| "http://localhost:3000/api".into());
    let url = format!("{}{}", base_url, path);
    args.push(&url);

    let output = Command::new("curl")
        .args(&args)
        .output()
        .expect("failed to execute curl");
    let status = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<i32>()
        .unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (stderr, status)
}

fn register_user(name: &str) -> (String, String) {
    let unique = format!(
        "{}-{}",
        name,
        uuid::Uuid::new_v4().to_string().split('-').next().unwrap()
    );
    let body = format!(r#"{{"name":"{}"}}"#, unique);
    let (resp, status) = api_request("POST", "/auth/register", Some(&body), "");
    assert_eq!(status, 200, "register failed: {}", resp);
    let json: serde_json::Value = serde_json::from_str(&resp).unwrap();
    // Response shape: { "user": { "id": ... }, "api_key": ... }
    (
        json["user"]["id"].as_str().unwrap().to_string(),
        json["api_key"].as_str().unwrap().to_string(),
    )
}

fn new_slug(prefix: &str) -> String {
    format!(
        "{}-{}",
        prefix,
        uuid::Uuid::new_v4().to_string().split('-').next().unwrap()
    )
}

fn create_workspace(api_key: &str, slug: &str) {
    let body = format!(
        r#"{{"name":"Read Authz WS","slug":"{}","visibility":"public"}}"#,
        slug
    );
    let (resp, status) = api_request("POST", "/workspaces", Some(&body), api_key);
    assert_eq!(status, 200, "create workspace failed: {}", resp);
}

fn join_workspace(api_key: &str, slug: &str) {
    let (resp, status) = api_request("POST", &format!("/workspaces/{}/join", slug), None, api_key);
    assert_eq!(status, 200, "join failed: {}", resp);
}

fn skip() -> bool {
    if std::env::var("TEST_API_URL").is_err() {
        eprintln!("Skipping: TEST_API_URL not set");
        true
    } else {
        false
    }
}

#[test]
fn test_read_pages_requires_membership() {
    if skip() {
        return;
    }
    let (_o, owner) = register_user("rp-owner");
    let (_x, outsider) = register_user("rp-outsider");
    let slug = new_slug("rp-priv");
    // Private workspace so the outsider has no membership at all.
    let body = format!(
        r#"{{"name":"Priv","slug":"{}","visibility":"private"}}"#,
        slug
    );
    let (resp, status) = api_request("POST", "/workspaces", Some(&body), &owner);
    assert_eq!(status, 200, "create private ws: {}", resp);

    // Owner (member) can list.
    let (_, s) = api_request("GET", &format!("/workspaces/{}/pages", slug), None, &owner);
    assert_eq!(s, 200, "owner list pages → 200");

    // Outsider is denied (membership gate).
    let (_, s) = api_request(
        "GET",
        &format!("/workspaces/{}/pages", slug),
        None,
        &outsider,
    );
    assert!(
        s == 403 || s == 404,
        "outsider list pages → 403/404, got {s}"
    );
}

#[test]
fn test_read_unauthenticated_rejected() {
    if skip() {
        return;
    }
    let (_o, owner) = register_user("rp-anon-owner");
    let slug = new_slug("rp-anon");
    create_workspace(&owner, &slug);
    // No bearer token at all.
    let (_, s) = api_request("GET", &format!("/workspaces/{}/pages", slug), None, "");
    assert_eq!(s, 401, "unauthenticated list pages → 401");
    let (_, s) = api_request("GET", &format!("/workspaces/{}/sources", slug), None, "");
    assert_eq!(s, 401, "unauthenticated list sources → 401");
}

#[test]
fn test_cannot_read_other_members_draft_branch() {
    if skip() {
        return;
    }
    // Two members of the same workspace; one must not read the other's draft.
    let (_o, owner) = register_user("rp-a");
    let (victim_id, victim) = register_user("rp-victim");
    let slug = new_slug("rp-shared");
    create_workspace(&owner, &slug);
    join_workspace(&victim, &slug);

    let victim_branch = format!("user/{}", victim_id);

    // Reading your own branch is fine (owner reading owner's branch via default is main;
    // here we assert the cross-user case is blocked).
    let (_, s) = api_request(
        "GET",
        &format!("/workspaces/{}/pages?branch={}", slug, victim_branch),
        None,
        &owner,
    );
    assert_eq!(
        s, 403,
        "reading another member's draft branch (pages) → 403"
    );

    let (_, s) = api_request(
        "GET",
        &format!("/workspaces/{}/sources?branch={}", slug, victim_branch),
        None,
        &owner,
    );
    assert_eq!(
        s, 403,
        "reading another member's draft branch (sources) → 403"
    );
}

#[test]
fn test_own_and_main_branches_readable() {
    if skip() {
        return;
    }
    let (owner_id, owner) = register_user("rp-self");
    let slug = new_slug("rp-self-ws");
    create_workspace(&owner, &slug);

    // main is always readable.
    let (_, s) = api_request(
        "GET",
        &format!("/workspaces/{}/pages?branch=main", slug),
        None,
        &owner,
    );
    assert_eq!(s, 200, "main branch readable → 200");

    // Caller's own draft branch is readable.
    let own = format!("user/{}", owner_id);
    let (_, s) = api_request(
        "GET",
        &format!("/workspaces/{}/pages?branch={}", slug, own),
        None,
        &owner,
    );
    assert_eq!(s, 200, "own draft branch readable → 200");
}
