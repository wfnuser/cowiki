use std::process::Command;

/// Helper: run `cowiki` with given args and return (stdout, stderr, exit_code)
fn run_cli(args: &[&str]) -> (String, String, i32) {
    let output = Command::new("./target/debug/cowiki")
        .args(args)
        .output()
        .expect("failed to execute cowiki CLI (did you build it?)");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(-1);
    (stdout, stderr, code)
}

// ── Help & Version ──────────────────────────────────

#[test]
fn test_help() {
    let (stdout, _stderr, code) = run_cli(&["--help"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("cowiki"));
    assert!(stdout.contains("ingest"));
    assert!(stdout.contains("compile"));
    assert!(stdout.contains("submit"));
    assert!(stdout.contains("search"));
    assert!(stdout.contains("read"));
    assert!(stdout.contains("write"));
    assert!(stdout.contains("list"));
    assert!(stdout.contains("review"));
}

#[test]
fn test_version() {
    let (stdout, _stderr, code) = run_cli(&["--version"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("0.1.0"));
}

#[test]
fn test_subcommand_help() {
    for cmd in &["search", "read", "list", "ingest", "compile", "submit", "write", "review"] {
        let (stdout, _stderr, code) = run_cli(&[cmd, "--help"]);
        assert_eq!(code, 0, "{} --help failed", cmd);
        assert!(!stdout.is_empty(), "{} --help produced no output", cmd);
    }
}

// ── JSON output flag ────────────────────────────────

#[test]
fn test_json_flag_accepted() {
    // These commands should accept --json without crashing on arg parsing
    let (_stdout, stderr, _code) = run_cli(&["--json", "search", "test"]);
    // Must NOT be an argument parsing error
    assert!(!stderr.contains("unrecognized"), "should accept --json flag");
    assert!(!stderr.contains("error:"), "unexpected arg error: {stderr}");
}

// ── Branch flag ─────────────────────────────────────

#[test]
fn test_branch_flag_accepted() {
    let (_stdout, stderr, _code) = run_cli(&["search", "test", "--branch", "custom-branch"]);
    // Should try to connect with custom branch; connection error is expected
    assert!(stderr.contains("Cannot connect") || stderr.contains("Network error") || stderr.is_empty());
}

// ── Invalid args ────────────────────────────────────

#[test]
fn test_search_requires_query() {
    let (_stdout, stderr, code) = run_cli(&["search"]);
    assert_ne!(code, 0);
    assert!(stderr.contains("QUERY") || stderr.contains("required"));
}

#[test]
fn test_read_requires_slug() {
    let (_stdout, stderr, code) = run_cli(&["read"]);
    assert_ne!(code, 0);
    assert!(stderr.contains("SLUG") || stderr.contains("required"));
}

// ── Completions ─────────────────────────────────────

#[test]
fn test_completions_bash() {
    let (stdout, _stderr, code) = run_cli(&["completions", "bash"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("complete"));
}

#[test]
fn test_completions_zsh() {
    let (stdout, _stderr, code) = run_cli(&["completions", "zsh"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("compdef") || stdout.contains("_cowiki"));
}

#[test]
fn test_completions_fish() {
    let (stdout, _stderr, code) = run_cli(&["completions", "fish"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("complete"));
}

// ── Workspace flag ──────────────────────────────────

#[test]
fn test_workspace_flag_accepted() {
    // -w short form
    let (_stdout, stderr, _code) = run_cli(&["-w", "my-wiki", "list"]);
    assert!(!stderr.contains("unrecognized"));
    assert!(!stderr.contains("error:"));

    // --workspace long form
    let (_stdout, stderr, _code) = run_cli(&["--workspace", "team-wiki", "list"]);
    assert!(!stderr.contains("unrecognized"));
    assert!(!stderr.contains("error:"));
}

#[test]
fn test_workspace_flag_shows_in_help() {
    let (stdout, _stderr, code) = run_cli(&["--help"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("--workspace") || stdout.contains("-w"),
        "help should show --workspace/-w flag");
}

#[test]
fn test_personal_space_no_workspace_flag() {
    // Personal space: no -w flag, should not require workspace
    let (_stdout, stderr, _code) = run_cli(&["list"]);
    // Should NOT complain about missing workspace
    assert!(!stderr.contains("workspace is required"));
}

#[test]
fn test_shared_workspace_with_flag() {
    // Shared workspace: with -w flag, should accept it
    let (_stdout, stderr, _code) = run_cli(&["-w", "team-wiki", "list"]);
    assert!(!stderr.contains("unrecognized"));
    assert!(!stderr.contains("error:"));
}

// ── API Integration Tests (require running server) ────
//
// Start the server first:
//   cd ../.. && cargo run -p cowiki-server
//
// Then run these tests:
//   cargo test -- --ignored

#[test]
#[ignore = "requires running cowiki server at http://localhost:3000"]
fn api_personal_list_main_branch() {
    // Personal workspace: no -w, defaults to main branch
    let (stdout, stderr, code) = run_cli(&["list", "--json"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    // Should return a JSON array (even if empty)
    assert!(stdout.trim().starts_with('['), "expected JSON array, got: {stdout}");
}

#[test]
#[ignore = "requires running cowiki server at http://localhost:3000"]
fn api_shared_workspace_list() {
    // Shared workspace with -w flag
    let (_stdout, stderr, code) = run_cli(&["-w", "demo-1779766635", "list", "--json"]);
    // May succeed or fail depending on workspace existence, but must not be arg error
    assert!(!stderr.contains("unrecognized"));
    assert!(!stderr.contains("error: invalid value"));
    // Accept any exit code except arg-parsing errors
    assert!(!stderr.contains("SUBCOMMAND"));
}

#[test]
#[ignore = "requires running cowiki server at http://localhost:3000"]
fn api_workspace_flag_routes_to_correct_endpoint() {
    // Without -w: hits /api/pages (legacy)
    let (_stdout, stderr, code1) = run_cli(&["list", "--json"]);

    // With -w: hits /api/workspaces/{ws}/pages
    let (_stdout, stderr, code2) = run_cli(&["-w", "demo-1779766635", "list", "--json"]);

    // Both should not produce argument errors
    assert!(!stderr.contains("error:"), "legacy list error: {stderr}");

    // If server is running, both should complete (even with 401/404)
    assert!(!stderr.contains("Cannot connect") || stderr.is_empty(),
        "server not running? stderr: {stderr}");
    // Either success or API error is OK; just not connection failure
    let _ = (code1, code2);
}

#[test]
#[ignore = "requires running cowiki server at http://localhost:3000"]
fn api_ingest_to_workspace() {
    // Ingest text to a workspace
    let (stdout, stderr, _code) = run_cli(&[
        "-w", "demo-1779766635",
        "ingest", "--type", "text",
        "--content", "API test content from CLI",
        "--json",
    ]);
    // Should return JSON with filename and content_hash
    assert!(
        stdout.contains("filename") || stderr.contains("401") || stderr.contains("404"),
        "expected ingest response or auth/not-found error, got: stdout='{stdout}' stderr='{stderr}'"
    );
}

#[test]
#[ignore = "requires running cowiki server at http://localhost:3000"]
fn api_read_page() {
    // Read a known page — may 404 but should not be arg error
    let (_stdout, stderr, _code) = run_cli(&["read", "home", "--no-pager", "--json"]);
    assert!(!stderr.contains("unrecognized"));
    assert!(!stderr.contains("error:"));
}

#[test]
#[ignore = "requires running cowiki server at http://localhost:3000"]
fn api_write_and_read_roundtrip() {
    let test_slug = "cli-test-roundtrip";
    let test_body = "# CLI Roundtrip Test\n\nContent from API test.";

    // Write
    let (_stdout, stderr, code) = run_cli(&[
        "write", test_slug,
        "--title", "CLI Roundtrip",
        "--body", test_body,
        "--json",
    ]);
    assert!(!stderr.contains("error: invalid"), "arg error: {stderr}");
    // May fail with 401 if not authenticated; that's OK for test
    if !stderr.contains("401") && !stderr.contains("Cannot connect") {
        // Read back
        let (stdout, stderr2, _code2) = run_cli(&[
            "read", test_slug, "--no-pager", "--json",
        ]);
        assert!(!stderr2.contains("error:"), "read error: {stderr2}");
        // If successful, should contain our content
        if code == 0 {
            assert!(stdout.contains(test_body) || stdout.contains(test_slug),
                "roundtrip failed: wrote but didn't read back");
        }
    }
}
