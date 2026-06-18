// ── OAuth CSRF state tests (#59) ─────────────────────────────────────
// Black-box HTTP tests (same harness style as permission_api_tests.rs).
// Self-skip unless TEST_API_URL is set, e.g.:
//   TEST_API_URL=http://localhost:3000/api cargo test -p cowiki-server
//
// The callback validates the CSRF `state` nonce BEFORE contacting GitHub, so
// the rejection paths (missing / unknown / already-used state) are observable
// without any GitHub credentials — they must not reach the token exchange.

use std::process::Command;

/// GET a path WITHOUT following redirects, returning the HTTP status.
fn get_status(path: &str) -> i32 {
    let base_url =
        std::env::var("TEST_API_URL").unwrap_or_else(|_| "http://localhost:3000/api".into());
    let url = format!("{}{}", base_url, path);
    let output = Command::new("curl")
        .args(["-s", "-o", "/dev/null", "-w", "%{http_code}", &url])
        .output()
        .expect("failed to execute curl");
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<i32>()
        .unwrap_or(-1)
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
fn test_callback_missing_state_rejected() {
    if skip() {
        return;
    }
    // No `state` param at all → 401 (rejected before any GitHub call).
    let s = get_status("/auth/github/callback?code=fake-code");
    assert_eq!(s, 401, "callback without state → 401, got {s}");
}

#[test]
fn test_callback_unknown_state_rejected() {
    if skip() {
        return;
    }
    // A state value the server never minted → 401.
    let s = get_status("/auth/github/callback?code=fake-code&state=never-issued-nonce");
    assert_eq!(s, 401, "callback with unknown state → 401, got {s}");
}

#[test]
fn test_login_redirects_with_state() {
    if skip() {
        return;
    }
    // /auth/github should 3xx-redirect to GitHub with a state param attached.
    let base_url =
        std::env::var("TEST_API_URL").unwrap_or_else(|_| "http://localhost:3000/api".into());
    let url = format!("{}/auth/github", base_url);
    let output = Command::new("curl")
        .args(["-s", "-D", "-", "-o", "/dev/null", &url])
        .output()
        .expect("failed to execute curl");
    let headers = String::from_utf8_lossy(&output.stdout);
    let location = headers
        .lines()
        .find(|l| l.to_lowercase().starts_with("location:"))
        .unwrap_or("");
    assert!(
        location.contains("github.com") && location.contains("state="),
        "login should redirect to GitHub with a state nonce; Location: {location}"
    );
}
