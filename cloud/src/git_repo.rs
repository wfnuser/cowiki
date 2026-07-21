use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

use crate::model::MemberRole;

pub const ZERO_OID: &str = "0000000000000000000000000000000000000000";

#[derive(Debug, thiserror::Error)]
pub enum GitRepoError {
    #[error("repository root must be absolute")]
    RelativeRoot,
    #[error("repository storage failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("Git command failed: {0}")]
    Git(String),
    #[error("invalid receive: {0}")]
    InvalidReceive(String),
    #[error("pull request head changed; expected {expected}, found {actual}")]
    StaleHead { expected: String, actual: String },
    #[error("pull request head is not based on the current Cloud main")]
    NotFastForward,
    #[error("required Git ref {0} does not exist")]
    MissingRef(String),
    #[error("repository lock registry is unavailable")]
    LockPoisoned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiveMode {
    Bootstrap,
    Normal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiveUpdate {
    pub old_oid: String,
    pub new_oid: String,
    pub reference: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FastForwardResult {
    pub old_main_oid: String,
    pub main_oid: String,
    pub already_merged: bool,
}

#[derive(Clone)]
pub struct GitRepoStore {
    root: PathBuf,
    locks: Arc<Mutex<HashMap<Uuid, Arc<AsyncMutex<()>>>>>,
}

impl GitRepoStore {
    pub fn new(root: impl AsRef<Path>) -> Result<Self, GitRepoError> {
        let root = root.as_ref();
        if !root.is_absolute() {
            return Err(GitRepoError::RelativeRoot);
        }
        fs::create_dir_all(root)?;
        Ok(Self {
            root: root.to_path_buf(),
            locks: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn repo_path(&self, space_id: Uuid) -> PathBuf {
        self.root.join(format!("{space_id}.git"))
    }

    pub fn space_lock(&self, space_id: Uuid) -> Result<Arc<AsyncMutex<()>>, GitRepoError> {
        let mut locks = self.locks.lock().map_err(|_| GitRepoError::LockPoisoned)?;
        Ok(locks
            .entry(space_id)
            .or_insert_with(|| Arc::new(AsyncMutex::new(())))
            .clone())
    }

    pub fn ensure_space(&self, space_id: Uuid) -> Result<PathBuf, GitRepoError> {
        let path = self.repo_path(space_id);
        if path.exists() {
            let bare = self.git_output(&path, &["rev-parse", "--is-bare-repository"])?;
            if String::from_utf8_lossy(&bare.stdout).trim() != "true" {
                return Err(GitRepoError::Git(format!(
                    "{} exists but is not a bare Git repository",
                    path.display()
                )));
            }
        } else {
            let output = Command::new("git")
                .args(["init", "--bare", "--initial-branch=main"])
                .arg(&path)
                .output()?;
            ensure_success(output)?;
        }
        ensure_success(
            Command::new("git")
                .args(["--git-dir"])
                .arg(&path)
                .args(["config", "http.receivepack", "true"])
                .output()?,
        )?;
        install_pre_receive_hook(&path)?;
        Ok(path)
    }

    pub fn main_exists(&self, space_id: Uuid) -> Result<bool, GitRepoError> {
        Ok(self.ref_oid(space_id, "main")?.is_some())
    }

    pub fn ref_oid(&self, space_id: Uuid, branch: &str) -> Result<Option<String>, GitRepoError> {
        validate_branch_name(branch)?;
        let path = self.repo_path(space_id);
        let output = Command::new("git")
            .args(["--git-dir"])
            .arg(&path)
            .args(["rev-parse", "--verify", &format!("refs/heads/{branch}")])
            .output()?;
        if output.status.success() {
            Ok(Some(
                String::from_utf8_lossy(&output.stdout).trim().to_string(),
            ))
        } else {
            Ok(None)
        }
    }

    pub fn fast_forward_main(
        &self,
        space_id: Uuid,
        head_branch: &str,
        expected_head_oid: &str,
    ) -> Result<FastForwardResult, GitRepoError> {
        if !head_branch.starts_with("user/") {
            return Err(GitRepoError::InvalidReceive(
                "pull request head must be a user branch".into(),
            ));
        }
        validate_oid(expected_head_oid)?;
        let path = self.repo_path(space_id);
        let head_oid = self
            .ref_oid(space_id, head_branch)?
            .ok_or_else(|| GitRepoError::MissingRef(head_branch.to_string()))?;
        if head_oid != expected_head_oid {
            return Err(GitRepoError::StaleHead {
                expected: expected_head_oid.to_string(),
                actual: head_oid,
            });
        }
        let main_oid = self
            .ref_oid(space_id, "main")?
            .ok_or_else(|| GitRepoError::MissingRef("main".into()))?;
        if main_oid == head_oid {
            return Ok(FastForwardResult {
                old_main_oid: main_oid.clone(),
                main_oid,
                already_merged: true,
            });
        }

        let ancestor = Command::new("git")
            .args(["--git-dir"])
            .arg(&path)
            .args(["merge-base", "--is-ancestor", &main_oid, &head_oid])
            .status()?;
        if !ancestor.success() {
            return Err(GitRepoError::NotFastForward);
        }
        let output = Command::new("git")
            .args(["--git-dir"])
            .arg(&path)
            .args([
                "update-ref",
                "-m",
                "CoWiki pull request merge",
                "refs/heads/main",
                &head_oid,
                &main_oid,
            ])
            .output()?;
        ensure_success(output)?;
        Ok(FastForwardResult {
            old_main_oid: main_oid,
            main_oid: head_oid,
            already_merged: false,
        })
    }

    fn git_output(&self, path: &Path, arguments: &[&str]) -> Result<Output, GitRepoError> {
        let output = Command::new("git")
            .args(["--git-dir"])
            .arg(path)
            .args(arguments)
            .stdin(Stdio::null())
            .output()?;
        ensure_success(output)
    }
}

pub fn validate_receive_updates(
    updates: &[ReceiveUpdate],
    user_id: Uuid,
    role: MemberRole,
    mode: ReceiveMode,
) -> Result<(), GitRepoError> {
    if !role.can_push() {
        return Err(GitRepoError::InvalidReceive(
            "role cannot push to this Space".into(),
        ));
    }
    for update in updates {
        validate_oid(&update.old_oid)?;
        validate_oid(&update.new_oid)?;
    }
    let user_ref = format!("refs/heads/user/{user_id}");
    match mode {
        ReceiveMode::Bootstrap => {
            if role != MemberRole::Owner || updates.len() != 2 {
                return Err(GitRepoError::InvalidReceive(
                    "bootstrap requires the owner and exactly two refs".into(),
                ));
            }
            let main = updates
                .iter()
                .find(|update| update.reference == "refs/heads/main");
            let user = updates.iter().find(|update| update.reference == user_ref);
            let (Some(main), Some(user)) = (main, user) else {
                return Err(GitRepoError::InvalidReceive(
                    "bootstrap must create main and the owner's user branch".into(),
                ));
            };
            if main.old_oid != ZERO_OID
                || user.old_oid != ZERO_OID
                || main.new_oid == ZERO_OID
                || main.new_oid != user.new_oid
            {
                return Err(GitRepoError::InvalidReceive(
                    "bootstrap refs must be new and point to the same commit".into(),
                ));
            }
        }
        ReceiveMode::Normal => {
            if updates.is_empty() {
                return Err(GitRepoError::InvalidReceive("push has no updates".into()));
            }
            if updates
                .iter()
                .any(|update| update.reference != user_ref || update.new_oid == ZERO_OID)
            {
                return Err(GitRepoError::InvalidReceive(
                    "push may update only the authenticated user's branch and may not delete it"
                        .into(),
                ));
            }
        }
    }
    Ok(())
}

fn validate_oid(oid: &str) -> Result<(), GitRepoError> {
    if matches!(oid.len(), 40 | 64) && oid.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(GitRepoError::InvalidReceive("invalid object id".into()))
    }
}

fn validate_branch_name(branch: &str) -> Result<(), GitRepoError> {
    if branch == "main"
        || branch
            .strip_prefix("user/")
            .and_then(|value| Uuid::parse_str(value).ok())
            .is_some()
    {
        Ok(())
    } else {
        Err(GitRepoError::InvalidReceive(
            "unsupported branch name".into(),
        ))
    }
}

fn ensure_success(output: Output) -> Result<Output, GitRepoError> {
    if output.status.success() {
        Ok(output)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(GitRepoError::Git(if stderr.is_empty() {
            output.status.to_string()
        } else {
            stderr
        }))
    }
}

fn install_pre_receive_hook(repo_path: &Path) -> Result<(), GitRepoError> {
    let hook = repo_path.join("hooks/pre-receive");
    fs::write(&hook, PRE_RECEIVE_HOOK)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

const PRE_RECEIVE_HOOK: &str = r#"#!/bin/sh
set -eu

if [ "${COWIKI_INTERNAL:-}" = "1" ]; then
  exit 0
fi

user_id="${COWIKI_USER_ID:-}"
role="${COWIKI_ROLE:-}"
mode="${COWIKI_RECEIVE_MODE:-normal}"
zero="0000000000000000000000000000000000000000"

case "$role" in
  owner|manager|editor) ;;
  *) echo "CoWiki: this role cannot push" >&2; exit 1 ;;
esac

case "$user_id" in
  ????????-????-????-????-????????????) ;;
  *) echo "CoWiki: missing authenticated user" >&2; exit 1 ;;
esac

updates="$(mktemp "${TMPDIR:-/tmp}/cowiki-receive.XXXXXX")"
trap 'rm -f "$updates"' EXIT HUP INT TERM
cat > "$updates"

if [ "$mode" = "bootstrap" ]; then
  [ "$role" = "owner" ] || { echo "CoWiki: only the owner can bootstrap" >&2; exit 1; }
  [ "$(wc -l < "$updates" | tr -d ' ')" = "2" ] || { echo "CoWiki: bootstrap requires two refs" >&2; exit 1; }
  main_oid=""
  user_oid=""
  while read -r old new ref; do
    [ "$old" = "$zero" ] || { echo "CoWiki: bootstrap refs must be new" >&2; exit 1; }
    [ "$new" != "$zero" ] || { echo "CoWiki: bootstrap cannot delete refs" >&2; exit 1; }
    case "$ref" in
      refs/heads/main) main_oid="$new" ;;
      "refs/heads/user/$user_id") user_oid="$new" ;;
      *) echo "CoWiki: bootstrap accepts only main and the owner branch" >&2; exit 1 ;;
    esac
  done < "$updates"
  [ -n "$main_oid" ] && [ "$main_oid" = "$user_oid" ] || { echo "CoWiki: bootstrap refs must match" >&2; exit 1; }
  exit 0
fi

while read -r _old new ref; do
  [ "$ref" = "refs/heads/user/$user_id" ] || { echo "CoWiki: push only your user branch" >&2; exit 1; }
  [ "$new" != "$zero" ] || { echo "CoWiki: user branches cannot be deleted" >&2; exit 1; }
done < "$updates"
"#;
