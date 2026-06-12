// ── Pages API Integration Tests ────────────────────────────────
// Tests nested-path write/read/list across wiki/entities/concepts.
// Requires the server to be running and TEST_API_URL to be set.
//
// Run with: cargo test -p cowiki-server --test pages_api_tests -- --ignored
// (marked #[ignore] because they need a running server)

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
        .expect("failed to execute curl (is it installed?)");

    let status = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<i32>()
        .unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if status == 0 {
        eprintln!("curl failed (server not running?): {}", stderr);
        return (stderr, -1);
    }

    (stderr, status)
}

fn register_user(name: &str) -> (String, String) {
    let body = format!(r#"{{"name":"{}"}}"#, name);
    let (resp, status) = api_request("POST", "/auth/register", Some(&body), "");
    assert_eq!(status, 200, "register failed: {}", resp);
    let json: serde_json::Value = serde_json::from_str(&resp).expect("invalid JSON from register");
    let user_id = json["id"].as_str().unwrap().to_string();
    let api_key = json["api_key"].as_str().unwrap().to_string();
    (user_id, api_key)
}

fn write_body(slug: &str, body: &str, branch: &str, path: &str) -> String {
    let escaped_body = body.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        r#"{{"slug":"{}","body":"{}","branch":"{}","path":"{}"}}"#,
        slug, escaped_body, branch, path
    )
}

// ── Nested Path Write then Read (wiki/) ──────────────────────

#[test]
#[ignore]
fn test_write_read_nested_path_in_wiki() {
    let (_uid, api_key) = register_user("nested-wiki-test");

    let slug = "messy/mengnan-dating-guide";
    let body = write_body(slug, "## 约会前准备\\n- 洗澡\\n- 刮胡子", "main", "wiki");

    let (resp, status) = api_request(
        "POST",
        "/workspaces/personal-nested-wiki-test/pages",
        Some(&body),
        &api_key,
    );
    assert_eq!(status, 200, "write nested page failed: {resp}");

    let json: serde_json::Value = serde_json::from_str(&resp).expect("invalid JSON from write");
    assert_eq!(json["ok"], true);
    assert_eq!(json["slug"], slug);
    assert_eq!(json["path"], "wiki/messy/mengnan-dating-guide");

    let (resp2, status2) = api_request(
        "GET",
        &format!("/workspaces/personal-nested-wiki-test/pages/{}?branch=main&dir=wiki", slug),
        None,
        &api_key,
    );
    assert_eq!(status2, 200, "read nested page failed: {resp2}");

    let page: serde_json::Value = serde_json::from_str(&resp2).expect("invalid JSON from read");
    assert_eq!(page["slug"], slug);
    assert!(page["body"].as_str().unwrap().contains("约会前准备"), "body should contain content");
}

// ── Default dir=wiki on read ─────────────────────────────────

#[test]
#[ignore]
fn test_read_nested_path_default_dir() {
    let (_uid, api_key) = register_user("nested-default-test");

    let body = write_body("deep/nested/page", "Deep Page", "main", "wiki");
    let (resp, status) = api_request(
        "POST",
        "/workspaces/personal-nested-default-test/pages",
        Some(&body),
        &api_key,
    );
    assert_eq!(status, 200, "write failed: {resp}");

    let (resp2, status2) = api_request(
        "GET",
        "/workspaces/personal-nested-default-test/pages/deep/nested/page?branch=main",
        None,
        &api_key,
    );
    assert_eq!(status2, 200, "read without dir param failed: {resp2}");
    let page: serde_json::Value = serde_json::from_str(&resp2).unwrap();
    assert_eq!(page["slug"], "deep/nested/page");
}

// ── Nested Path in entities/ ──────────────────────────────────

#[test]
#[ignore]
fn test_write_read_nested_in_entities() {
    let (_uid, api_key) = register_user("nested-entities-test");

    let body = write_body("people/alice", "Alice - Software engineer", "main", "entities");
    let (resp, status) = api_request(
        "POST",
        "/workspaces/personal-nested-entities-test/pages",
        Some(&body),
        &api_key,
    );
    assert_eq!(status, 200, "write entities nested failed: {resp}");

    let (resp2, status2) = api_request(
        "GET",
        "/workspaces/personal-nested-entities-test/pages/people/alice?branch=main&dir=entities",
        None,
        &api_key,
    );
    assert_eq!(status2, 200, "read entities nested failed: {resp2}");
    let page: serde_json::Value = serde_json::from_str(&resp2).unwrap();
    assert!(page["body"].as_str().unwrap().contains("Alice"));
}

// ── Nested Path in concepts/ ──────────────────────────────────

#[test]
#[ignore]
fn test_write_read_nested_in_concepts() {
    let (_uid, api_key) = register_user("nested-concepts-test");

    let body = write_body("patterns/error/handle", "Error Handling Pattern", "main", "concepts");
    let (resp, status) = api_request(
        "POST",
        "/workspaces/personal-nested-concepts-test/pages",
        Some(&body),
        &api_key,
    );
    assert_eq!(status, 200, "write concepts nested failed: {resp}");

    let (resp2, status2) = api_request(
        "GET",
        "/workspaces/personal-nested-concepts-test/pages/patterns/error/handle?branch=main&dir=concepts",
        None,
        &api_key,
    );
    assert_eq!(status2, 200, "read concepts nested failed: {resp2}");
}

// ── list --dir all ────────────────────────────────────────────

#[test]
#[ignore]
fn test_list_all_includes_nested_pages() {
    let (_uid, api_key) = register_user("list-all-test");

    for (slug, body_text, path) in [
        ("page1", "P1", "wiki"),
        ("nested/page2", "P2", "wiki"),
        ("person1", "Person", "entities"),
    ] {
        let body = write_body(slug, body_text, "main", path);
        let (resp, status) = api_request(
            "POST",
            "/workspaces/personal-list-all-test/pages",
            Some(&body),
            &api_key,
        );
        assert_eq!(status, 200, "write failed for {slug}: {resp}");
    }

    let (resp, status) = api_request(
        "GET",
        "/workspaces/personal-list-all-test/pages?branch=main&dir=all",
        None,
        &api_key,
    );
    assert_eq!(status, 200, "list all failed: {resp}");
    let tree: serde_json::Value = serde_json::from_str(&resp).unwrap();
    assert!(tree.is_array());
    let tree_str = serde_json::to_string(&tree).unwrap();
    assert!(tree_str.contains("wiki"), "should contain wiki folder");
    assert!(tree_str.contains("entities"), "should contain entities folder");
}

// ── Reject invalid dir ────────────────────────────────────────

#[test]
#[ignore]
fn test_reject_invalid_dir() {
    let (_uid, api_key) = register_user("invalid-dir-test");

    let body = write_body("test", "Test", "main", "invalid");
    let (resp, status) = api_request(
        "POST",
        "/workspaces/personal-invalid-dir-test/pages",
        Some(&body),
        &api_key,
    );
    assert_eq!(status, 400, "should reject invalid dir, got: {resp}");
    assert!(resp.contains("unknown content dir"), "error: {resp}");
}

// ── Reject path traversal in slug ─────────────────────────────

#[test]
#[ignore]
fn test_reject_path_traversal() {
    let (_uid, api_key) = register_user("traversal-test");

    let body = write_body("../etc/passwd", "Hack", "main", "wiki");
    let (resp, status) = api_request(
        "POST",
        "/workspaces/personal-traversal-test/pages",
        Some(&body),
        &api_key,
    );
    assert_eq!(status, 400, "should reject traversal, got: {resp}");
    assert!(resp.contains("path traversal"), "error: {resp}");
}

// ── Reject dir=all on write ───────────────────────────────────

#[test]
#[ignore]
fn test_reject_dir_all_on_write() {
    let (_uid, api_key) = register_user("dir-all-write-test");

    let body = write_body("test", "Test", "main", "all");
    let (resp, status) = api_request(
        "POST",
        "/workspaces/personal-dir-all-write-test/pages",
        Some(&body),
        &api_key,
    );
    assert_eq!(status, 400, "should reject dir=all on write, got: {resp}");
    assert!(
        resp.contains("listing"),
        "error should mention listing: {resp}"
    );
}
