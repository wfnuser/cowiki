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
