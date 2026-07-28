use serde::Serialize;
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
    #[error("repository path is invalid: {0}")]
    InvalidPath(String),
    #[error("repository object does not exist: {0}")]
    ObjectNotFound(String),
    #[error("repository lock registry is unavailable")]
    LockPoisoned,
    #[error("pull request diff exceeds the {0} byte size limit")]
    DiffTooLarge(usize),
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownTreeEntry {
    pub path: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownTreeSnapshot {
    pub oid: String,
    pub entries: Vec<MarkdownTreeEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentBlob {
    pub oid: String,
    pub path: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedMarkdownFile {
    pub path: String,
    pub status: String,
    pub additions: u64,
    pub deletions: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestDiff {
    pub base_oid: String,
    pub head_oid: String,
    pub files: Vec<ChangedMarkdownFile>,
    pub patch: String,
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

    pub fn root(&self) -> &Path {
        &self.root
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

    pub fn read_markdown_tree(
        &self,
        space_id: Uuid,
        branch: &str,
    ) -> Result<MarkdownTreeSnapshot, GitRepoError> {
        validate_branch_name(branch)?;
        let oid = self
            .ref_oid(space_id, branch)?
            .ok_or_else(|| GitRepoError::MissingRef(branch.to_owned()))?;
        let path = self.repo_path(space_id);
        let output = self.git_output(&path, &["ls-tree", "-r", "-z", "--name-only", &oid])?;
        let mut entries = Vec::new();
        let mut folders = std::collections::BTreeSet::new();

        for raw_path in output.stdout.split(|byte| *byte == 0) {
            if raw_path.is_empty() {
                continue;
            }
            let Ok(file_path) = std::str::from_utf8(raw_path) else {
                continue;
            };
            if !is_visible_path(file_path) || !has_markdown_extension(file_path) {
                continue;
            }
            let mut parent = Path::new(file_path).parent();
            while let Some(folder) = parent {
                if folder.as_os_str().is_empty() {
                    break;
                }
                folders.insert(folder.to_string_lossy().replace('\\', "/"));
                parent = folder.parent();
            }
            entries.push(MarkdownTreeEntry {
                path: file_path.to_owned(),
                kind: "page".into(),
            });
        }
        entries.extend(folders.into_iter().map(|path| MarkdownTreeEntry {
            path,
            kind: "folder".into(),
        }));
        entries.sort_by(|left, right| {
            left.path
                .cmp(&right.path)
                .then_with(|| left.kind.cmp(&right.kind))
        });
        Ok(MarkdownTreeSnapshot { oid, entries })
    }

    pub fn read_content_blob(
        &self,
        space_id: Uuid,
        branch: &str,
        requested_path: &str,
    ) -> Result<ContentBlob, GitRepoError> {
        validate_branch_name(branch)?;
        let normalized_path = validate_repository_path(requested_path)?;
        if !is_visible_path(&normalized_path) {
            return Err(GitRepoError::ObjectNotFound(normalized_path));
        }
        let oid = self
            .ref_oid(space_id, branch)?
            .ok_or_else(|| GitRepoError::MissingRef(branch.to_owned()))?;
        let repository = self.repo_path(space_id);
        let object = format!("{oid}:{normalized_path}");
        let object_type = Command::new("git")
            .args(["--git-dir"])
            .arg(&repository)
            .args(["cat-file", "-t", &object])
            .stdin(Stdio::null())
            .output()?;
        if !object_type.status.success()
            || String::from_utf8_lossy(&object_type.stdout).trim() != "blob"
        {
            return Err(GitRepoError::ObjectNotFound(normalized_path));
        }
        let output = self.git_output(&repository, &["cat-file", "blob", &object])?;
        Ok(ContentBlob {
            oid,
            path: normalized_path,
            bytes: output.stdout,
        })
    }

    pub fn markdown_diff(
        &self,
        space_id: Uuid,
        base_oid: &str,
        head_oid: &str,
        max_bytes: usize,
    ) -> Result<PullRequestDiff, GitRepoError> {
        validate_oid(base_oid)?;
        validate_oid(head_oid)?;
        let repository = self.repo_path(space_id);
        let status_output = self.git_output(
            &repository,
            &[
                "diff",
                "--name-status",
                "--no-renames",
                "-z",
                base_oid,
                head_oid,
                "--",
                ":(glob)**/*.md",
            ],
        )?;
        let mut statuses = HashMap::new();
        let status_parts = status_output
            .stdout
            .split(|byte| *byte == 0)
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        for pair in status_parts.chunks_exact(2) {
            let status = std::str::from_utf8(pair[0])
                .map_err(|_| GitRepoError::Git("diff status is not UTF-8".into()))?;
            let path = std::str::from_utf8(pair[1])
                .map_err(|_| GitRepoError::Git("diff path is not UTF-8".into()))?;
            statuses.insert(path.to_string(), diff_status_label(status).to_string());
        }

        let stat_output = self.git_output(
            &repository,
            &[
                "diff",
                "--numstat",
                "--no-renames",
                "-z",
                base_oid,
                head_oid,
                "--",
                ":(glob)**/*.md",
            ],
        )?;
        let mut files = Vec::new();
        for entry in stat_output
            .stdout
            .split(|byte| *byte == 0)
            .filter(|part| !part.is_empty())
        {
            let entry = std::str::from_utf8(entry)
                .map_err(|_| GitRepoError::Git("diff statistics are not UTF-8".into()))?;
            let mut fields = entry.splitn(3, '\t');
            let additions = parse_diff_count(fields.next())?;
            let deletions = parse_diff_count(fields.next())?;
            let path = fields
                .next()
                .ok_or_else(|| GitRepoError::Git("invalid diff statistics".into()))?;
            files.push(ChangedMarkdownFile {
                path: path.to_string(),
                status: statuses
                    .remove(path)
                    .unwrap_or_else(|| "modified".to_string()),
                additions,
                deletions,
            });
        }

        let patch_output = self.git_output(
            &repository,
            &[
                "diff",
                "--patch",
                "--no-ext-diff",
                "--no-color",
                "--no-renames",
                "--unified=3",
                base_oid,
                head_oid,
                "--",
                ":(glob)**/*.md",
            ],
        )?;
        if patch_output.stdout.len() > max_bytes {
            return Err(GitRepoError::DiffTooLarge(max_bytes));
        }
        let patch = String::from_utf8(patch_output.stdout)
            .map_err(|_| GitRepoError::Git("Markdown diff is not UTF-8".into()))?;
        Ok(PullRequestDiff {
            base_oid: base_oid.to_string(),
            head_oid: head_oid.to_string(),
            files,
            patch,
        })
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

fn diff_status_label(value: &str) -> &'static str {
    match value.chars().next() {
        Some('A') => "added",
        Some('D') => "deleted",
        Some('T') => "type-changed",
        _ => "modified",
    }
}

fn parse_diff_count(value: Option<&str>) -> Result<u64, GitRepoError> {
    match value {
        Some("-") => Ok(0),
        Some(value) => value
            .parse()
            .map_err(|_| GitRepoError::Git("invalid diff line count".into())),
        None => Err(GitRepoError::Git("invalid diff statistics".into())),
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

fn validate_repository_path(value: &str) -> Result<String, GitRepoError> {
    if value.is_empty()
        || value.starts_with('/')
        || value.contains('\\')
        || value.contains('\0')
        || value
            .split('/')
            .any(|component| component.is_empty() || matches!(component, "." | ".."))
    {
        return Err(GitRepoError::InvalidPath(value.to_owned()));
    }
    let path = Path::new(value);
    if path
        .components()
        .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(GitRepoError::InvalidPath(value.to_owned()));
    }
    Ok(path.to_string_lossy().into_owned())
}

fn is_visible_path(value: &str) -> bool {
    Path::new(value)
        .components()
        .all(|component| match component {
            std::path::Component::Normal(name) => !name.to_string_lossy().starts_with('.'),
            _ => false,
        })
}

fn has_markdown_extension(value: &str) -> bool {
    Path::new(value)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
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
