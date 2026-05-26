// ── Permission System API Integration Tests ──────────────────────────
// These tests validate the full permission matrix at the API level.
// Requires the server to be running and TEST_API_URL to be set.

use std::process::Command;

/// Helper: make an HTTP request to the API
fn api_request(
    method: &str,
    path: &str,
    body: Option<&str>,
    api_key: &str,
) -> (String, i32) {
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

    let base_url = std::env::var("TEST_API_URL").unwrap_or_else(|_| "http://localhost:3000/api".into());
    let url = format!("{}{}", base_url, path);
    args.push(&url);

    let output = Command::new("curl")
        .args(&args)
        .output()
        .expect("failed to execute curl (is it installed?)");

    let status = String::from_utf8_lossy(&output.stdout).trim().parse::<i32>().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if status == 0 {
        eprintln!("curl failed (server not running?): {}", stderr);
        return (stderr, -1);
    }

    (stderr, status)
}

/// Helper: register a test user and return (user_id, api_key)
fn register_user(name: &str) -> (String, String) {
    let body = format!(r#"{{"name":"{}"}}"#, name);
    let (resp, status) = api_request("POST", "/auth/register", Some(&body), "");
    assert_eq!(status, 200, "register failed: {}", resp);

    let json: serde_json::Value = serde_json::from_str(&resp).expect("invalid JSON from register");
    let user_id = json["id"].as_str().unwrap().to_string();
    let api_key = json["api_key"].as_str().unwrap().to_string();
    (user_id, api_key)
}

/// Helper: create a workspace and return the slug
fn create_workspace(api_key: &str, name: &str, slug: &str, visibility: &str) -> String {
    let body = format!(r#"{{"name":"{}","slug":"{}","visibility":"{}"}}"#, name, slug, visibility);
    let (resp, status) = api_request("POST", "/workspaces", Some(&body), api_key);
    assert_eq!(status, 200, "create workspace failed: {}", resp);
    slug.to_string()
}

/// Helper: join a public workspace
fn join_workspace(api_key: &str, slug: &str) -> String {
    let (resp, status) = api_request("POST", &format!("/workspaces/{}/join", slug), None, api_key);
    assert_eq!(status, 200, "join workspace failed: {} (status: {})", resp, status);
    let json: serde_json::Value = serde_json::from_str(&resp).unwrap();
    json["role"].as_str().unwrap().to_string()
}

// ═══════════════════════════════════════════════════════════════
// Basic CRUD — Workspace + Member
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_create_and_list_workspaces() {
    if std::env::var("TEST_API_URL").is_err() {
        eprintln!("Skipping: TEST_API_URL not set");
        return;
    }
    let (_uid, key) = register_user("testuser-perm-1");
    let slug = format!("perm-test-{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap());

    create_workspace(&key, "Perm Test WS", &slug, "public");

    // List my workspaces
    let (resp, status) = api_request("GET", "/workspaces", None, &key);
    assert_eq!(status, 200);
    assert!(resp.contains(&slug));
    assert!(resp.contains("owner"), "creator should be owner");
}

#[test]
fn test_list_workspaces_returns_correct_role() {
    if std::env::var("TEST_API_URL").is_err() {
        eprintln!("Skipping: TEST_API_URL not set");
        return;
    }
    let (_uid_a, key_a) = register_user("testuser-perm-2a");
    let (_uid_b, key_b) = register_user("testuser-perm-2b");
    let slug = format!("perm-test-{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap());

    create_workspace(&key_a, "Role Test WS", &slug, "public");
    join_workspace(&key_b, &slug);

    // User A (owner) lists
    let (resp_a, _) = api_request("GET", "/workspaces", None, &key_a);
    assert!(resp_a.contains("owner"), "user_a should be owner");

    // User B (writer, joined public) lists
    let (resp_b, _) = api_request("GET", "/workspaces", None, &key_b);
    assert!(resp_b.contains("writer"), "user_b should be writer for joined workspace");
}

// ═══════════════════════════════════════════════════════════════
// Permission Enforcement — Invite (owner-only)
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_invite_requires_owner() {
    if std::env::var("TEST_API_URL").is_err() {
        eprintln!("Skipping: TEST_API_URL not set");
        return;
    }
    let (_uid_a, key_a) = register_user("testuser-inv-a");
    let (_uid_b, key_b) = register_user("testuser-inv-b");
    let (_uid_c, key_c) = register_user("testuser-inv-c");
    let slug = format!("perm-inv-{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap());

    create_workspace(&key_a, "Invite Test WS", &slug, "public");
    join_workspace(&key_b, &slug); // user_b joins as writer
    join_workspace(&key_c, &slug); // user_c joins as writer

    // Writer tries to invite → should get 403 Forbidden
    let body = r#"{"email":"new@test.com","role":"writer"}"#;
    let (_resp, status) = api_request("POST", &format!("/workspaces/{}/invite", slug), Some(body), &key_b);
    assert_eq!(status, 403, "writer should not be able to invite members");

    // Owner can invite
    let (_resp, status) = api_request("POST", &format!("/workspaces/{}/invite", slug), Some(body), &key_a);
    assert_eq!(status, 200, "owner should be able to invite members");
}

#[test]
fn test_invite_with_invalid_role_rejected() {
    if std::env::var("TEST_API_URL").is_err() {
        eprintln!("Skipping: TEST_API_URL not set");
        return;
    }
    let (_uid_a, key_a) = register_user("testuser-inv-role");
    let slug = format!("perm-inv-role-{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap());

    create_workspace(&key_a, "Invite Role WS", &slug, "public");

    // Invalid role
    let body = r#"{"email":"new@test.com","role":"admin"}"#;
    let (_resp, status) = api_request("POST", &format!("/workspaces/{}/invite", slug), Some(body), &key_a);
    assert_eq!(status, 400, "invalid role should be rejected");
}

// ═══════════════════════════════════════════════════════════════
// Permission Enforcement — Member Management (owner-only)
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_list_members_all_roles_can_view() {
    if std::env::var("TEST_API_URL").is_err() {
        eprintln!("Skipping: TEST_API_URL not set");
        return;
    }
    let (_uid_a, key_a) = register_user("testuser-mem-a");
    let (_uid_b, key_b) = register_user("testuser-mem-b");
    let slug = format!("perm-mem-{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap());

    create_workspace(&key_a, "Member View WS", &slug, "public");
    join_workspace(&key_b, &slug);

    // Both owner and writer should be able to list members
    let (_resp, status_a) = api_request("GET", &format!("/workspaces/{}/members", slug), None, &key_a);
    assert_eq!(status_a, 200, "owner should list members");

    let (_resp, status_b) = api_request("GET", &format!("/workspaces/{}/members", slug), None, &key_b);
    assert_eq!(status_b, 200, "writer should list members");
}

#[test]
fn test_list_members_non_member_rejected() {
    if std::env::var("TEST_API_URL").is_err() {
        eprintln!("Skipping: TEST_API_URL not set");
        return;
    }
    let (_uid_a, key_a) = register_user("testuser-nonmem-a");
    let (_uid_b, key_b) = register_user("testuser-nonmem-b");
    let slug = format!("perm-nonmem-{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap());

    create_workspace(&key_a, "NonMember WS", &slug, "public");

    // user_b is not a member → should get 403
    let (_resp, status) = api_request("GET", &format!("/workspaces/{}/members", slug), None, &key_b);
    assert_eq!(status, 403, "non-member should not see members");
}

#[test]
fn test_remove_member_owner_only() {
    if std::env::var("TEST_API_URL").is_err() {
        eprintln!("Skipping: TEST_API_URL not set");
        return;
    }
    let (_uid_a, key_a) = register_user("testuser-rm-a");
    let (_uid_b, key_b) = register_user("testuser-rm-b");
    let slug = format!("perm-rm-{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap());

    create_workspace(&key_a, "Remove Test WS", &slug, "public");
    join_workspace(&key_b, &slug);

    // Get user_b's ID from member list
    let (resp, _) = api_request("GET", &format!("/workspaces/{}/members", slug), None, &key_a);
    let members: Vec<serde_json::Value> = serde_json::from_str(&resp).unwrap();
    let user_b_id = members.iter()
        .find(|m| m["name"].as_str() == Some("testuser-rm-b"))
        .map(|m| m["id"].as_str().unwrap())
        .expect("user_b should be in member list");

    // Writer tries to remove → 403
    let body = format!(r#"{{"user_id":"{}"}}"#, user_b_id);
    let (_resp, status) = api_request("POST", &format!("/workspaces/{}/members/remove", slug), Some(&body), &key_b);
    assert_eq!(status, 403, "writer should not remove members");

    // Owner removes → 200
    let (_resp, status) = api_request("POST", &format!("/workspaces/{}/members/remove", slug), Some(&body), &key_a);
    assert_eq!(status, 200, "owner should remove members");
}

#[test]
fn test_change_role_owner_only() {
    if std::env::var("TEST_API_URL").is_err() {
        eprintln!("Skipping: TEST_API_URL not set");
        return;
    }
    let (_uid_a, key_a) = register_user("testuser-cr-a");
    let (_uid_b, key_b) = register_user("testuser-cr-b");
    let slug = format!("perm-cr-{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap());

    create_workspace(&key_a, "ChangeRole WS", &slug, "public");
    join_workspace(&key_b, &slug);

    // Get user_b's ID
    let (resp, _) = api_request("GET", &format!("/workspaces/{}/members", slug), None, &key_a);
    let members: Vec<serde_json::Value> = serde_json::from_str(&resp).unwrap();
    let user_b_id = members.iter()
        .find(|m| m["name"].as_str() == Some("testuser-cr-b"))
        .map(|m| m["id"].as_str().unwrap())
        .expect("user_b should be in member list");

    // Writer tries to change role → 403
    let body = format!(r#"{{"user_id":"{}","role":"reader"}}"#, user_b_id);
    let (_resp, status) = api_request("POST", &format!("/workspaces/{}/members/role", slug), Some(&body), &key_b);
    assert_eq!(status, 403, "writer should not change roles");

    // Owner changes role → 200
    let (_resp, status) = api_request("POST", &format!("/workspaces/{}/members/role", slug), Some(&body), &key_a);
    assert_eq!(status, 200, "owner should change roles");
}

// ═══════════════════════════════════════════════════════════════
// Permission Enforcement — Rename (owner-only)
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_rename_requires_owner() {
    if std::env::var("TEST_API_URL").is_err() {
        eprintln!("Skipping: TEST_API_URL not set");
        return;
    }
    let (_uid_a, key_a) = register_user("testuser-rn-a");
    let (_uid_b, key_b) = register_user("testuser-rn-b");
    let slug = format!("perm-rn-{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap());

    create_workspace(&key_a, "Rename Test WS", &slug, "public");
    join_workspace(&key_b, &slug);

    // Writer tries to rename → 403
    let body = r#"{"name":"Renamed by Writer"}"#;
    let (_resp, status) = api_request("POST", &format!("/workspaces/{}/rename", slug), Some(body), &key_b);
    assert_eq!(status, 403, "writer should not rename workspace");

    // Owner renames → 200
    let body = r#"{"name":"Renamed by Owner"}"#;
    let (_resp, status) = api_request("POST", &format!("/workspaces/{}/rename", slug), Some(body), &key_a);
    assert_eq!(status, 200, "owner should rename workspace");
}

// ═══════════════════════════════════════════════════════════════
// Permission Enforcement — Delete (owner-only)
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_delete_requires_owner() {
    if std::env::var("TEST_API_URL").is_err() {
        eprintln!("Skipping: TEST_API_URL not set");
        return;
    }
    let (_uid_a, key_a) = register_user("testuser-del-a");
    let (_uid_b, key_b) = register_user("testuser-del-b");
    let slug = format!("perm-del-{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap());

    create_workspace(&key_a, "Delete Test WS", &slug, "public");
    join_workspace(&key_b, &slug);

    // Writer tries to delete → 403
    let (_resp, status) = api_request("DELETE", &format!("/workspaces/{}", slug), None, &key_b);
    assert_eq!(status, 403, "writer should not delete workspace");

    // Owner deletes → 200
    let (_resp, status) = api_request("DELETE", &format!("/workspaces/{}", slug), None, &key_a);
    assert_eq!(status, 200, "owner should delete workspace");
}

// ═══════════════════════════════════════════════════════════════
// Full Permission Matrix Test
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_full_permission_matrix() {
    if std::env::var("TEST_API_URL").is_err() {
        eprintln!("Skipping: TEST_API_URL not set");
        return;
    }

    // Setup: owner Alice, writer Bob, reader Carol
    let (_uid_a, key_a) = register_user("perm-matrix-alice");
    let (_uid_b, key_b) = register_user("perm-matrix-bob");
    let (_uid_c, _key_c) = register_user("perm-matrix-carol");
    let slug = format!("perm-matrix-{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap());

    // Alice creates the workspace (becomes owner)
    create_workspace(&key_a, "Matrix Test WS", &slug, "public");

    // Bob joins (writer)
    join_workspace(&key_b, &slug);

    // Owner invites Carol as reader
    let invite_body = format!(r#"{{"email":"perm-matrix-carol@test.com","role":"reader"}}"#);
    let (invite_resp, inv_status) = api_request("POST", &format!("/workspaces/{}/invite", slug), Some(&invite_body), &key_a);
    assert_eq!(inv_status, 200, "owner should invite reader: {}", invite_resp);

    // Get Carol's user_id from invite response (or reader list would have it)
    // For simplicity, we'll test that Carol can see the workspace but not manage
    // (Note: in real flow, Carol would need to accept invitation first,
    //  but for this test we just test the permission matrix concept)

    // ── Matrix verification ──

    // Operations that ALL can do: list members
    let (_, s) = api_request("GET", &format!("/workspaces/{}/members", slug), None, &key_a);
    assert_eq!(s, 200, "owner list members");
    let (_, s) = api_request("GET", &format!("/workspaces/{}/members", slug), None, &key_b);
    assert_eq!(s, 200, "writer list members");

    // Operations that ONLY owner can do: invite
    let (_, s) = api_request("POST", &format!("/workspaces/{}/invite", slug), Some(r#"{"email":"x@t.com","role":"reader"}"#), &key_b);
    assert_eq!(s, 403, "writer invite → 403");
    let (_, s) = api_request("POST", &format!("/workspaces/{}/invite", slug), Some(r#"{"email":"x@t.com","role":"reader"}"#), &key_a);
    assert_eq!(s, 200, "owner invite → 200");

    // Operations that ONLY owner can do: rename
    let (_, s) = api_request("POST", &format!("/workspaces/{}/rename", slug), Some(r#"{"name":"Hacked"}"#), &key_b);
    assert_eq!(s, 403, "writer rename → 403");

    // Operations that ONLY owner can do: delete
    let slug_del = format!("perm-matrix-del-{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap());
    create_workspace(&key_a, "Matrix Del WS", &slug_del, "public");
    join_workspace(&key_b, &slug_del);
    let (_, s) = api_request("DELETE", &format!("/workspaces/{}", slug_del), None, &key_b);
    assert_eq!(s, 403, "writer delete → 403");
}
