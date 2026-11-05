//! Which provider requests CoWiki can show as an approval, and the fixed
//! permission boundary every native session runs inside.
//!
//! Fail closed: a request outside this table is answered "unsupported" and
//! never approved. There are no session-wide or permanent grants; each
//! approval allows one action.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodexApproval {
    Command,
    File,
    McpTool,
}

/// Codex app-server request methods CoWiki shows as an approval card.
pub fn classify_approval(method: &str) -> Result<CodexApproval, String> {
    match method {
        "item/commandExecution/requestApproval" => Ok(CodexApproval::Command),
        "item/fileChange/requestApproval" => Ok(CodexApproval::File),
        "mcpServer/elicitation/request" => Ok(CodexApproval::McpTool),
        // Network/filesystem escalation, arbitrary user input and legacy
        // approval methods are not something one click should grant.
        other => Err(format!("CoWiki does not support {other}")),
    }
}

/// The JSON a Codex approval request expects for one allow/deny.
pub fn codex_decision(kind: CodexApproval, allow: bool) -> Value {
    match (kind, allow) {
        (CodexApproval::Command | CodexApproval::File, true) => json!({"decision": "accept"}),
        (CodexApproval::Command | CodexApproval::File, false) => json!({"decision": "decline"}),
        (CodexApproval::McpTool, true) => json!({"action": "accept", "content": {}}),
        (CodexApproval::McpTool, false) => json!({"action": "decline"}),
    }
}

/// Thread-level policy: ask before anything the sandbox does not allow.
pub const CODEX_APPROVAL_POLICY: &str = "on-request";
pub const CODEX_SANDBOX_MODE: &str = "workspace-write";

/// Turn-level sandbox: write only the session directory, no network.
pub fn codex_sandbox_policy(cwd: &Path) -> Value {
    json!({
        "type": "workspaceWrite",
        "writableRoots": [cwd.to_string_lossy()],
        "networkAccess": false,
        "excludeTmpdirEnvVar": false,
        "excludeSlashTmp": false,
    })
}

/// Checks what Codex reports it will actually enforce after the user's
/// `config.toml` was applied. Anything wider than workspace-write without
/// network, or an approval policy that never asks, refuses the session.
pub fn verify_codex_thread_policy(response: &Value, cwd: &Path) -> Result<(), String> {
    let approval = response.get("approvalPolicy");
    if approval.and_then(Value::as_str) == Some("never") {
        return Err(
            "Codex is configured to never ask for approval (approval_policy = \"never\"). CoWiki will not start it with that setting."
                .to_string(),
        );
    }
    let sandbox = response
        .get("sandbox")
        .ok_or_else(|| "Codex did not report its sandbox".to_string())?;
    match sandbox.get("type").and_then(Value::as_str) {
        Some("readOnly") => Ok(()),
        Some("workspaceWrite") => {
            if sandbox.get("networkAccess").and_then(Value::as_bool) == Some(true) {
                return Err(
                    "Codex is configured with network access in its sandbox. CoWiki runs Agents without network access."
                        .to_string(),
                );
            }
            let roots = sandbox
                .get("writableRoots")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            for root in roots {
                let Some(root) = root.as_str() else { continue };
                if !is_allowed_writable_root(Path::new(root), cwd) {
                    return Err(format!(
                        "Codex is configured to write outside this Space ({root}). Remove that writable root to use it in CoWiki."
                    ));
                }
            }
            Ok(())
        }
        Some(other) => Err(format!(
            "Codex is configured with the {other} sandbox. CoWiki only runs Agents in a workspace-write sandbox."
        )),
        None => Err("Codex reported an unknown sandbox".to_string()),
    }
}

fn is_allowed_writable_root(root: &Path, cwd: &Path) -> bool {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    if root.starts_with(cwd) {
        return true;
    }
    temporary_roots()
        .iter()
        .any(|temporary| root.starts_with(temporary))
}

fn temporary_roots() -> Vec<PathBuf> {
    let mut roots = vec![PathBuf::from("/tmp"), PathBuf::from("/private/tmp")];
    let temporary = std::env::temp_dir();
    roots.push(temporary.canonicalize().unwrap_or(temporary));
    roots
}

/// ACP permission options CoWiki can pick for a one-time decision.
pub fn acp_option(options: &[Value], allow: bool) -> Option<String> {
    let wanted = if allow { "allow_once" } else { "reject_once" };
    options
        .iter()
        .find(|option| option.get("kind").and_then(Value::as_str) == Some(wanted))
        .and_then(|option| option.get("optionId").and_then(Value::as_str))
        .map(ToOwned::to_owned)
}

/// Claude Code runs in `acceptEdits`: file edits inside its working
/// directory apply without asking (as Codex's workspace-write sandbox does),
/// everything else that needs permission asks CoWiki.
pub const CLAUDE_PERMISSION_MODE: &str = "acceptEdits";

/// Claude Code's built-in tools that write files, and the input field that
/// names the file.
pub fn claude_file_target(tool_name: &str, input: &Value) -> Option<Option<String>> {
    let field = match tool_name {
        "Write" | "Edit" | "MultiEdit" => "file_path",
        "NotebookEdit" => "notebook_path",
        _ => return None,
    };
    Some(
        input
            .get(field)
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
    )
}

/// Whether `path` (absolute, or relative to `cwd`) stays inside `cwd`
/// after symlinks: the nearest existing ancestor is resolved, so a link that
/// points out of the Space, `..`, or `/tmp` vs `/private/tmp` cannot cross
/// the boundary.
pub fn inside_workspace(path: &str, cwd: &Path) -> bool {
    use std::path::Component;
    let Ok(root) = cwd.canonicalize() else {
        return false;
    };
    let candidate = Path::new(path);
    let candidate = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        cwd.join(candidate)
    };
    if candidate
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return false;
    }
    let mut existing = candidate.as_path();
    let mut rest = Vec::new();
    loop {
        if let Ok(resolved) = existing.canonicalize() {
            let mut full = resolved;
            for part in rest.iter().rev() {
                full.push(part);
            }
            return full.starts_with(&root);
        }
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_os_string());
                existing = parent;
            }
            _ => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_escalation_is_not_a_file_approval() {
        assert!(classify_approval("item/permissions/requestApproval").is_err());
        assert!(classify_approval("item/tool/requestUserInput").is_err());
        assert!(classify_approval("execCommandApproval").is_err());
        assert_eq!(
            classify_approval("item/commandExecution/requestApproval"),
            Ok(CodexApproval::Command)
        );
        assert_eq!(
            classify_approval("item/fileChange/requestApproval"),
            Ok(CodexApproval::File)
        );
    }

    #[test]
    fn decisions_never_grant_for_the_session() {
        for kind in [
            CodexApproval::Command,
            CodexApproval::File,
            CodexApproval::McpTool,
        ] {
            for allow in [true, false] {
                let text = codex_decision(kind, allow).to_string();
                assert!(!text.contains("ForSession"), "{text}");
                assert!(!text.contains("Amendment"), "{text}");
            }
        }
    }

    #[test]
    fn local_config_cannot_widen_the_boundary() {
        let cwd = std::env::temp_dir().canonicalize().unwrap().join("space");
        let ok = json!({"approvalPolicy": "on-request", "sandbox": {"type": "workspaceWrite", "writableRoots": [cwd.to_string_lossy()], "networkAccess": false}});
        assert!(verify_codex_thread_policy(&ok, &cwd).is_ok());
        let full = json!({"approvalPolicy": "on-request", "sandbox": {"type": "dangerFullAccess"}});
        assert!(verify_codex_thread_policy(&full, &cwd).is_err());
        let never = json!({"approvalPolicy": "never", "sandbox": {"type": "readOnly", "networkAccess": false}});
        assert!(verify_codex_thread_policy(&never, &cwd).is_err());
        let network = json!({"approvalPolicy": "untrusted", "sandbox": {"type": "workspaceWrite", "writableRoots": [], "networkAccess": true}});
        assert!(verify_codex_thread_policy(&network, &cwd).is_err());
        let outside = json!({"approvalPolicy": "on-request", "sandbox": {"type": "workspaceWrite", "writableRoots": ["/Users"], "networkAccess": false}});
        assert!(verify_codex_thread_policy(&outside, &cwd).is_err());
    }

    #[test]
    fn acp_choices_are_one_time_only() {
        let options = vec![
            json!({"optionId": "always", "kind": "allow_always", "name": "Always"}),
            json!({"optionId": "once", "kind": "allow_once", "name": "Allow"}),
            json!({"optionId": "no", "kind": "reject_once", "name": "Reject"}),
        ];
        assert_eq!(acp_option(&options, true).as_deref(), Some("once"));
        assert_eq!(acp_option(&options, false).as_deref(), Some("no"));
        assert_eq!(acp_option(&options[..1], true), None);
    }

    #[test]
    fn claude_file_writes_stay_inside_the_workspace() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let space = root.join("space");
        let outside = root.join("outside");
        std::fs::create_dir_all(space.join("notes")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let inside = |path: &str| inside_workspace(path, &space);
        assert!(inside(space.join("a.md").to_str().unwrap()));
        assert!(inside(space.join("new/dir/b.md").to_str().unwrap()));
        assert!(inside("notes/c.md"));
        assert!(!inside(outside.join("x.md").to_str().unwrap()));
        assert!(!inside(space.join("../outside/x.md").to_str().unwrap()));
        assert!(!inside("../outside/x.md"));
        assert!(!inside("/etc/hosts"));
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, space.join("link")).unwrap();
            assert!(!inside(space.join("link/x.md").to_str().unwrap()));
        }
        assert_eq!(
            claude_file_target("Write", &json!({"file_path": "/a"})),
            Some(Some("/a".into()))
        );
        assert_eq!(claude_file_target("NotebookEdit", &json!({})), Some(None));
        assert_eq!(claude_file_target("Bash", &json!({"command": "ls"})), None);
    }
}
