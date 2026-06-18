// ── Review-comment authorization tests (#54) ─────────────────────────
// Black-box HTTP tests (same harness style as permission_api_tests.rs).
// Self-skip unless TEST_API_URL is set, e.g.:
//   TEST_API_URL=http://localhost:3000/api cargo test -p cowiki-server
//
// Cover the authorization boundaries added in PR #86:
//  - cross-workspace submission-id guessing → 404
//  - cross-submission reply smuggling via parent_id → 404
//  - delete authorization matrix (author vs editor vs viewer)

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
        r#"{{"name":"Comment Authz WS","slug":"{}","visibility":"public"}}"#,
        slug
    );
    let (resp, status) = api_request("POST", "/workspaces", Some(&body), api_key);
    assert_eq!(status, 200, "create workspace failed: {}", resp);
}

fn join_workspace(api_key: &str, slug: &str) {
    let (resp, status) = api_request("POST", &format!("/workspaces/{}/join", slug), None, api_key);
    assert_eq!(status, 200, "join failed: {}", resp);
}

fn set_role(owner_key: &str, slug: &str, user_id: &str, role: &str) {
    let body = format!(r#"{{"user_id":"{}","role":"{}"}}"#, user_id, role);
    let (resp, status) = api_request(
        "POST",
        &format!("/workspaces/{}/members/role", slug),
        Some(&body),
        owner_key,
    );
    assert_eq!(status, 200, "set role failed: {}", resp);
}

/// Write a page on the author's own draft branch and submit it for review.
/// Returns the submission id.
fn make_submission(author_key: &str, author_id: &str, slug: &str, page: &str) -> String {
    let branch = format!("user/{}", author_id);
    let write_body = format!(
        r#"{{"slug":"{}","body":"body text","branch":"{}","title":"T"}}"#,
        page, branch
    );
    let (resp, status) = api_request(
        "POST",
        &format!("/workspaces/{}/pages", slug),
        Some(&write_body),
        author_key,
    );
    assert_eq!(status, 200, "write page failed: {}", resp);

    let submit_body = format!(r#"{{"branch":"{}","page_slugs":["{}"]}}"#, branch, page);
    let (resp, status) = api_request(
        "POST",
        &format!("/workspaces/{}/submit", slug),
        Some(&submit_body),
        author_key,
    );
    assert_eq!(status, 200, "submit failed: {}", resp);
    let json: serde_json::Value = serde_json::from_str(&resp).unwrap();
    json["submission_id"].as_str().unwrap().to_string()
}

fn create_comment(api_key: &str, slug: &str, sub_id: &str, body_text: &str) -> (String, i32) {
    let body = format!(
        r#"{{"file_path":"wiki/{}.md","body":"{}"}}"#,
        "p", body_text
    );
    api_request(
        "POST",
        &format!("/workspaces/{}/reviews/{}/comments", slug, sub_id),
        Some(&body),
        api_key,
    )
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
fn test_comment_cross_workspace_denied() {
    if skip() {
        return;
    }
    // Submission lives in WS-A; attacker owns an unrelated WS-B.
    let (a_id, a_key) = register_user("ca-owner-a");
    let (_b_id, b_key) = register_user("ca-owner-b");
    let ws_a = new_slug("ca-a");
    let ws_b = new_slug("ca-b");
    create_workspace(&a_key, &ws_a);
    create_workspace(&b_key, &ws_b);
    let sub = make_submission(&a_key, &a_id, &ws_a, "p");

    // Listing/commenting on WS-A's submission via the WS-B path → 404 (not in WS-B).
    let (_, s) = api_request(
        "GET",
        &format!("/workspaces/{}/reviews/{}/comments", ws_b, sub),
        None,
        &b_key,
    );
    assert_eq!(s, 404, "cross-workspace list comments → 404, got {s}");

    let (_, s) = create_comment(&b_key, &ws_b, &sub, "sneaky");
    assert_eq!(s, 404, "cross-workspace create comment → 404, got {s}");
}

#[test]
fn test_comment_requires_membership() {
    if skip() {
        return;
    }
    let (a_id, a_key) = register_user("ca-priv-owner");
    let (_x_id, outsider) = register_user("ca-outsider");
    let ws = new_slug("ca-priv");
    // Private so the outsider has no membership.
    let body = format!(r#"{{"name":"P","slug":"{}","visibility":"private"}}"#, ws);
    let (resp, status) = api_request("POST", "/workspaces", Some(&body), &a_key);
    assert_eq!(status, 200, "create private ws: {}", resp);
    let sub = make_submission(&a_key, &a_id, &ws, "p");

    let (_, s) = api_request(
        "GET",
        &format!("/workspaces/{}/reviews/{}/comments", ws, sub),
        None,
        &outsider,
    );
    assert!(
        s == 403 || s == 404,
        "outsider list comments → 403/404, got {s}"
    );

    // Unauthenticated → 401.
    let (_, s) = api_request(
        "GET",
        &format!("/workspaces/{}/reviews/{}/comments", ws, sub),
        None,
        "",
    );
    assert_eq!(s, 401, "unauthenticated list comments → 401, got {s}");
}

#[test]
fn test_reply_smuggling_denied() {
    if skip() {
        return;
    }
    // Two submissions in the same workspace; a reply must not target a thread on
    // a different submission.
    let (a_id, a_key) = register_user("ca-smug-owner");
    let ws = new_slug("ca-smug");
    create_workspace(&a_key, &ws);
    let sub1 = make_submission(&a_key, &a_id, &ws, "p1");
    let sub2 = make_submission(&a_key, &a_id, &ws, "p2");

    // Create a comment on sub1.
    let (resp, s) = create_comment(&a_key, &ws, &sub1, "root on sub1");
    assert_eq!(s, 200, "create comment on sub1 → 200: {resp}");
    let c1: serde_json::Value = serde_json::from_str(&resp).unwrap();
    let c1_id = c1["id"].as_str().unwrap();

    // Reply on sub2 with parent_id pointing at sub1's comment → 404.
    let body = format!(
        r#"{{"file_path":"wiki/p2.md","body":"reply","parent_id":"{}"}}"#,
        c1_id
    );
    let (_, s) = api_request(
        "POST",
        &format!("/workspaces/{}/reviews/{}/comments", ws, sub2),
        Some(&body),
        &a_key,
    );
    assert_eq!(s, 404, "reply smuggling across submissions → 404, got {s}");
}

#[test]
fn test_delete_authorization_matrix() {
    if skip() {
        return;
    }
    // Owner (editor+), plus two members we'll demote to viewer.
    let (owner_id, owner) = register_user("ca-del-owner");
    let (author_id, author) = register_user("ca-del-author");
    let (other_id, other) = register_user("ca-del-other");
    let ws = new_slug("ca-del");
    create_workspace(&owner, &ws);
    join_workspace(&author, &ws); // editor
    join_workspace(&other, &ws); // editor

    let sub = make_submission(&owner, &owner_id, &ws, "p");

    // Demote both to viewer so we can test the author-can-delete-own path for a viewer.
    set_role(&owner, &ws, &author_id, "viewer");
    set_role(&owner, &ws, &other_id, "viewer");

    // Viewer author posts a comment (members can comment).
    let (resp, s) = create_comment(&author, &ws, &sub, "by viewer author");
    assert_eq!(s, 200, "viewer can comment → 200: {resp}");
    let cid = serde_json::from_str::<serde_json::Value>(&resp).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Another viewer (non-author) cannot delete it → 403.
    let (_, s) = api_request(
        "DELETE",
        &format!("/workspaces/{}/reviews/{}/comments/{}", ws, sub, cid),
        None,
        &other,
    );
    assert_eq!(s, 403, "viewer non-author delete → 403, got {s}");

    // The author (even as a viewer) can delete their own comment → 200.
    let (_, s) = api_request(
        "DELETE",
        &format!("/workspaces/{}/reviews/{}/comments/{}", ws, sub, cid),
        None,
        &author,
    );
    assert_eq!(s, 200, "viewer author delete own → 200, got {s}");

    // A non-author editor (owner) can delete someone else's comment (moderation).
    let (resp, s) = create_comment(&author, &ws, &sub, "second comment");
    assert_eq!(s, 200, "viewer comment again → 200: {resp}");
    let cid2 = serde_json::from_str::<serde_json::Value>(&resp).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let (_, s) = api_request(
        "DELETE",
        &format!("/workspaces/{}/reviews/{}/comments/{}", ws, sub, cid2),
        None,
        &owner,
    );
    assert_eq!(
        s, 200,
        "editor non-author delete (moderation) → 200, got {s}"
    );
}
