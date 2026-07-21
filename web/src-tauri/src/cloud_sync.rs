use crate::local_engine::{CloudLink, LocalEngine};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;
use url::Url;
use uuid::Uuid;

const REMOTE_NAME: &str = "cowiki";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SyncState {
    Unlinked,
    Dirty,
    UpToDate,
    NeedsSync,
    Synced,
    Conflicted,
    Submitted,
    LeaseRejected,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CloudPullRequest {
    pub id: String,
    pub number: i64,
    pub title: String,
    pub head_ref: String,
    pub head_oid: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CloudSyncResult {
    pub state: SyncState,
    pub conflicts: Vec<String>,
    pub committed: bool,
    pub message: String,
    pub pull_request: Option<CloudPullRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CloudSpaceApi {
    id: String,
    git_url: String,
    user_ref: String,
}

impl CloudSyncResult {
    fn new(state: SyncState, message: impl Into<String>) -> Self {
        Self {
            state,
            conflicts: Vec::new(),
            committed: false,
            message: message.into(),
            pull_request: None,
        }
    }

    fn conflicted(path: &Path) -> Result<Self, String> {
        Ok(Self {
            state: SyncState::Conflicted,
            conflicts: conflict_paths(path)?,
            committed: false,
            message: "Rebase stopped for manual conflict resolution.".into(),
            pull_request: None,
        })
    }
}

pub fn link_space(
    engine: &LocalEngine,
    space_slug: &str,
    cloud_base_url: &str,
    token: &str,
    cloud_space_id: Option<&str>,
    git_url: Option<&str>,
    cloud_name: &str,
    cloud_slug: &str,
    commit_message: Option<&str>,
    user_name: &str,
    user_id: Uuid,
) -> Result<CloudSyncResult, String> {
    let space = engine.find_space(space_slug)?;
    ensure_token(token)?;
    if is_rebase_in_progress(&space.local_path)? {
        return CloudSyncResult::conflicted(&space.local_path);
    }
    let mut committed = false;
    if is_dirty(&space.local_path)? {
        let message = required_commit_message(commit_message)?;
        committed = engine
            .submit_with_message(
                space_slug,
                &[],
                message,
                user_name,
                &format!("{user_id}@users.cowiki.app"),
            )?
            .committed;
        if is_dirty(&space.local_path)? {
            return Err(
                "Cloud submit left unsupported local files uncommitted; review the working tree"
                    .into(),
            );
        }
    }

    let base = validate_cloud_base(cloud_base_url)?;
    let (cloud_space_id, git_url) = match (cloud_space_id, git_url) {
        (Some(id), Some(git_url)) => {
            Uuid::parse_str(id).map_err(|_| "invalid Cloud Space id".to_string())?;
            validate_git_url(git_url)?;
            (id.to_string(), git_url.to_string())
        }
        (None, None) => {
            let created = create_cloud_space(&base, token, cloud_name, cloud_slug)?;
            let expected_user_ref = format!("user/{user_id}");
            if created.user_ref != expected_user_ref {
                return Err("Cloud returned a user branch for a different account".into());
            }
            validate_git_url(&created.git_url)?;
            (created.id, created.git_url)
        }
        _ => {
            return Err(
                "Cloud Space id and Git URL must either both be supplied or both be omitted".into(),
            );
        }
    };
    configure_cowiki_remote(&space.local_path, &git_url, user_id)?;
    let main_exists = remote_main_exists(&space.local_path, token)?;
    if !main_exists {
        bootstrap_remote(&space.local_path, token, user_id)?;
    }
    let link = CloudLink {
        cloud_space_id,
        base_url: base.to_string().trim_end_matches('/').to_string(),
        git_url,
        user_id: user_id.to_string(),
    };
    engine.save_cloud_link(space_slug, &link)?;
    let mut result = if main_exists {
        sync_if_clean_path(&space.local_path, token)?
    } else {
        CloudSyncResult::new(SyncState::UpToDate, "Cloud Space linked and initialized.")
    };
    result.committed = committed;
    Ok(result)
}

pub fn get_status(engine: &LocalEngine, space_slug: &str) -> Result<CloudSyncResult, String> {
    let Some(_) = engine.cloud_link(space_slug)? else {
        return Ok(CloudSyncResult::new(
            SyncState::Unlinked,
            "This Space is local only.",
        ));
    };
    let space = engine.find_space(space_slug)?;
    status_path(&space.local_path)
}

pub fn sync_if_clean(
    engine: &LocalEngine,
    space_slug: &str,
    token: &str,
) -> Result<CloudSyncResult, String> {
    let _link = required_link(engine, space_slug)?;
    ensure_token(token)?;
    let space = engine.find_space(space_slug)?;
    sync_if_clean_path(&space.local_path, token)
}

pub fn submit(
    engine: &LocalEngine,
    space_slug: &str,
    token: &str,
    commit_message: Option<&str>,
    pull_request_title: Option<&str>,
    pull_request_body: Option<&str>,
    user_name: &str,
) -> Result<CloudSyncResult, String> {
    let link = required_link(engine, space_slug)?;
    ensure_token(token)?;
    let user_id = Uuid::parse_str(&link.user_id)
        .map_err(|_| "stored Cloud account id is invalid".to_string())?;
    let cloud_space_id = Uuid::parse_str(&link.cloud_space_id)
        .map_err(|_| "stored Cloud Space id is invalid".to_string())?;
    let space = engine.find_space(space_slug)?;
    if is_rebase_in_progress(&space.local_path)? {
        return CloudSyncResult::conflicted(&space.local_path);
    }

    let mut committed = false;
    if is_dirty(&space.local_path)? {
        let message = required_commit_message(commit_message)?;
        committed = engine
            .submit_with_message(
                space_slug,
                &[],
                message,
                user_name,
                &format!("{user_id}@users.cowiki.app"),
            )?
            .committed;
        if is_dirty(&space.local_path)? {
            return Err(
                "Cloud submit left unsupported local files uncommitted; review the working tree"
                    .into(),
            );
        }
    }

    let mut synced = sync_if_clean_path(&space.local_path, token)?;
    synced.committed = committed;
    if synced.state == SyncState::Conflicted {
        return Ok(synced);
    }
    let pushed = push_user_branch(&space.local_path, token, user_id)?;
    if pushed.state == SyncState::LeaseRejected {
        return Ok(CloudSyncResult {
            committed,
            ..pushed
        });
    }
    let default_title = commit_message
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            git_stdout(&space.local_path, &["log", "-1", "--pretty=%s"], None)
                .unwrap_or_else(|_| "Update Cloud Space".into())
        });
    let title = pull_request_title
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&default_title);
    let pull_request = create_or_update_pull_request(
        &link.base_url,
        token,
        cloud_space_id,
        title,
        pull_request_body.unwrap_or_default().trim(),
    )?;
    Ok(CloudSyncResult {
        state: SyncState::Submitted,
        conflicts: Vec::new(),
        committed,
        message: format!("Submitted Cloud pull request #{}.", pull_request.number),
        pull_request: Some(pull_request),
    })
}

pub fn rebase_continue(engine: &LocalEngine, space_slug: &str) -> Result<CloudSyncResult, String> {
    let _link = required_link(engine, space_slug)?;
    let space = engine.find_space(space_slug)?;
    rebase_continue_path(&space.local_path)
}

pub fn rebase_abort(engine: &LocalEngine, space_slug: &str) -> Result<CloudSyncResult, String> {
    let _link = required_link(engine, space_slug)?;
    let space = engine.find_space(space_slug)?;
    rebase_abort_path(&space.local_path)
}

fn required_link(engine: &LocalEngine, space_slug: &str) -> Result<CloudLink, String> {
    engine
        .cloud_link(space_slug)?
        .ok_or_else(|| "This Space is not linked to CoWiki Cloud".to_string())
}

fn configure_cowiki_remote(path: &Path, git_url: &str, user_id: Uuid) -> Result<(), String> {
    validate_git_url(git_url)?;
    let existing = git_stdout(path, &["remote", "get-url", REMOTE_NAME], None).ok();
    match existing {
        Some(existing) if existing != git_url => {
            return Err(format!(
                "Git remote '{REMOTE_NAME}' already points to a different Cloud Space"
            ));
        }
        Some(_) => {}
        None => git_success(path, &["remote", "add", REMOTE_NAME, git_url], None)?,
    }
    git_success(
        path,
        &[
            "config",
            "--replace-all",
            "remote.cowiki.fetch",
            "+refs/heads/main:refs/remotes/cowiki/main",
        ],
        None,
    )?;
    git_success(
        path,
        &[
            "config",
            "--add",
            "remote.cowiki.fetch",
            &format!("+refs/heads/user/{user_id}:refs/remotes/cowiki/user/{user_id}"),
        ],
        None,
    )
}

fn bootstrap_remote(path: &Path, token: &str, user_id: Uuid) -> Result<(), String> {
    git_success(
        path,
        &[
            "push",
            "--atomic",
            REMOTE_NAME,
            "main:refs/heads/main",
            &format!("main:refs/heads/user/{user_id}"),
        ],
        Some(token),
    )?;
    fetch(path, token)
}

fn remote_main_exists(path: &Path, token: &str) -> Result<bool, String> {
    let output = git_command(
        path,
        &["ls-remote", "--heads", REMOTE_NAME, "refs/heads/main"],
        Some(token),
    )?;
    if !output.status.success() {
        return Err(git_failure(&output));
    }
    Ok(!output.stdout.is_empty())
}

fn sync_if_clean_path(path: &Path, token: &str) -> Result<CloudSyncResult, String> {
    ensure_token(token)?;
    if is_rebase_in_progress(path)? {
        return CloudSyncResult::conflicted(path);
    }
    if is_dirty(path)? {
        return Ok(CloudSyncResult::new(
            SyncState::Dirty,
            "Local main has uncommitted files; automatic sync did not change it.",
        ));
    }
    fetch(path, token)?;
    let previous = git_stdout(path, &["rev-parse", "HEAD"], None)?;
    git_stdout(
        path,
        &["rev-parse", "--verify", "refs/remotes/cowiki/main"],
        None,
    )?;
    let output = git_command(path, &["rebase", "refs/remotes/cowiki/main"], None)?;
    if !output.status.success() {
        if !conflict_paths(path)?.is_empty() {
            return CloudSyncResult::conflicted(path);
        }
        return Err(git_failure(&output));
    }
    let current = git_stdout(path, &["rev-parse", "HEAD"], None)?;
    Ok(if previous == current {
        CloudSyncResult::new(
            SyncState::UpToDate,
            "Local main is up to date with Cloud main.",
        )
    } else {
        CloudSyncResult::new(SyncState::Synced, "Local main rebased onto Cloud main.")
    })
}

fn push_user_branch(path: &Path, token: &str, user_id: Uuid) -> Result<CloudSyncResult, String> {
    let tracking = format!("refs/remotes/cowiki/user/{user_id}");
    let lease_oid = git_stdout(path, &["rev-parse", "--verify", &tracking], None).ok();
    let lease = format!(
        "--force-with-lease=refs/heads/user/{user_id}:{}",
        lease_oid.as_deref().unwrap_or("")
    );
    let output = git_command(
        path,
        &[
            "push",
            &lease,
            REMOTE_NAME,
            &format!("main:refs/heads/user/{user_id}"),
        ],
        Some(token),
    )?;
    if output.status.success() {
        fetch(path, token)?;
        return Ok(CloudSyncResult::new(
            SyncState::Submitted,
            "Local main pushed to the Cloud user branch.",
        ));
    }
    let failure = git_failure(&output);
    let lower = failure.to_ascii_lowercase();
    if lower.contains("stale info")
        || lower.contains("fetch first")
        || lower.contains("force-with-lease")
        || lower.contains("rejected")
    {
        Ok(CloudSyncResult::new(
            SyncState::LeaseRejected,
            "The Cloud user branch changed on another device; fetch and review it before retrying.",
        ))
    } else {
        Err(failure)
    }
}

fn rebase_continue_path(path: &Path) -> Result<CloudSyncResult, String> {
    if !is_rebase_in_progress(path)? {
        return Err("No Cloud rebase is in progress".into());
    }
    let conflicts = conflict_paths(path)?;
    if !conflicts.is_empty() {
        return CloudSyncResult::conflicted(path);
    }
    let output = git_command(
        path,
        &["-c", "core.editor=true", "rebase", "--continue"],
        None,
    )?;
    if output.status.success() {
        Ok(CloudSyncResult::new(
            SyncState::Synced,
            "Conflict resolution completed and local main is rebased.",
        ))
    } else if !conflict_paths(path)?.is_empty() {
        CloudSyncResult::conflicted(path)
    } else {
        Err(git_failure(&output))
    }
}

fn rebase_abort_path(path: &Path) -> Result<CloudSyncResult, String> {
    if is_rebase_in_progress(path)? {
        git_success(path, &["rebase", "--abort"], None)?;
    }
    status_path(path)
}

fn status_path(path: &Path) -> Result<CloudSyncResult, String> {
    if is_rebase_in_progress(path)? {
        return CloudSyncResult::conflicted(path);
    }
    if is_dirty(path)? {
        return Ok(CloudSyncResult::new(
            SyncState::Dirty,
            "Local main has uncommitted files.",
        ));
    }
    let local = git_stdout(path, &["rev-parse", "HEAD"], None)?;
    let remote = match git_stdout(
        path,
        &["rev-parse", "--verify", "refs/remotes/cowiki/main"],
        None,
    ) {
        Ok(remote) => remote,
        Err(_) => {
            return Ok(CloudSyncResult::new(
                SyncState::NeedsSync,
                "Cloud main has not been fetched yet.",
            ));
        }
    };
    Ok(if local == remote {
        CloudSyncResult::new(SyncState::UpToDate, "Local main matches Cloud main.")
    } else {
        CloudSyncResult::new(
            SyncState::NeedsSync,
            "Local main and Cloud main differ; sync before submitting.",
        )
    })
}

fn fetch(path: &Path, token: &str) -> Result<(), String> {
    git_success(path, &["fetch", "--prune", REMOTE_NAME], Some(token))
}

fn is_dirty(path: &Path) -> Result<bool, String> {
    Ok(!git_stdout(
        path,
        &["status", "--porcelain", "--untracked-files=all"],
        None,
    )?
    .is_empty())
}

fn is_rebase_in_progress(path: &Path) -> Result<bool, String> {
    for name in ["rebase-merge", "rebase-apply"] {
        let git_path = git_stdout(path, &["rev-parse", "--git-path", name], None)?;
        let git_path = PathBuf::from(git_path);
        let git_path = if git_path.is_absolute() {
            git_path
        } else {
            path.join(git_path)
        };
        if git_path.exists() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn conflict_paths(path: &Path) -> Result<Vec<String>, String> {
    let output = git_command(
        path,
        &["diff", "--name-only", "--diff-filter=U", "-z"],
        None,
    )?;
    if !output.status.success() {
        return Err(git_failure(&output));
    }
    Ok(output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|value| !value.is_empty())
        .map(|value| String::from_utf8_lossy(value).into_owned())
        .collect())
}

fn create_cloud_space(
    base: &Url,
    token: &str,
    name: &str,
    slug: &str,
) -> Result<CloudSpaceApi, String> {
    let endpoint = base
        .join("/api/spaces")
        .map_err(|error| error.to_string())?;
    let response = http_client()?
        .post(endpoint)
        .bearer_auth(token)
        .json(&serde_json::json!({ "name": name, "slug": slug }))
        .send()
        .map_err(|error| format!("Cloud Space creation failed: {error}"))?;
    parse_json_response(response)
}

fn create_or_update_pull_request(
    base_url: &str,
    token: &str,
    space_id: Uuid,
    title: &str,
    body: &str,
) -> Result<CloudPullRequest, String> {
    let base = validate_cloud_base(base_url)?;
    let endpoint = base
        .join(&format!("/api/spaces/{space_id}/pull-requests"))
        .map_err(|error| error.to_string())?;
    let response = http_client()?
        .post(endpoint)
        .bearer_auth(token)
        .json(&serde_json::json!({ "title": title, "body": body }))
        .send()
        .map_err(|error| format!("Cloud pull request creation failed: {error}"))?;
    parse_json_response(response)
}

fn parse_json_response<T: serde::de::DeserializeOwned>(
    response: reqwest::blocking::Response,
) -> Result<T, String> {
    let status = response.status();
    if status.is_success() {
        return response
            .json::<T>()
            .map_err(|error| format!("Cloud response was invalid: {error}"));
    }
    let value = response.json::<serde_json::Value>().unwrap_or_default();
    let message = value
        .get("error")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("Cloud request failed");
    Err(format!("{message} ({status})"))
}

fn http_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())
}

fn validate_cloud_base(value: &str) -> Result<Url, String> {
    let mut url = Url::parse(value).map_err(|error| format!("invalid Cloud URL: {error}"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("Cloud URL must be an http(s) origin without credentials or query data".into());
    }
    url.set_path("/");
    Ok(url)
}

fn validate_git_url(value: &str) -> Result<(), String> {
    if value.starts_with("http://") || value.starts_with("https://") {
        let url = Url::parse(value).map_err(|error| format!("invalid Cloud Git URL: {error}"))?;
        if !url.username().is_empty() || url.password().is_some() {
            return Err("Cloud Git URL must not contain credentials".into());
        }
    } else if !Path::new(value).is_absolute() {
        return Err("Cloud Git URL must be http(s) or an absolute local test path".into());
    }
    Ok(())
}

fn ensure_token(token: &str) -> Result<(), String> {
    if token.trim().is_empty() {
        Err("Cloud sign-in is required".into())
    } else {
        Ok(())
    }
}

fn required_commit_message(message: Option<&str>) -> Result<&str, String> {
    message
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Confirm a commit message before submitting dirty local files".into())
}

fn git_success(path: &Path, arguments: &[&str], token: Option<&str>) -> Result<(), String> {
    let output = git_command(path, arguments, token)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(git_failure(&output))
    }
}

fn git_stdout(path: &Path, arguments: &[&str], token: Option<&str>) -> Result<String, String> {
    let output = git_command(path, arguments, token)?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(git_failure(&output))
    }
}

fn git_command(path: &Path, arguments: &[&str], token: Option<&str>) -> Result<Output, String> {
    let mut command = Command::new("git");
    command
        .args(arguments)
        .current_dir(path)
        .env("GIT_TERMINAL_PROMPT", "0");
    if let Some(token) = token {
        ensure_token(token)?;
        command
            .env("GIT_CONFIG_COUNT", "1")
            .env("GIT_CONFIG_KEY_0", "http.extraHeader")
            .env(
                "GIT_CONFIG_VALUE_0",
                format!("Authorization: Bearer {token}"),
            );
    }
    command
        .output()
        .map_err(|error| format!("cannot run Git: {error}"))
}

fn git_failure(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        format!("Git exited with {}", output.status)
    } else {
        stderr
    }
}

#[cfg(test)]
mod tests {
    use super::{
        bootstrap_remote, configure_cowiki_remote, push_user_branch, rebase_abort_path,
        sync_if_clean_path, SyncState,
    };
    use std::path::Path;
    use std::process::Command;
    use uuid::Uuid;

    #[test]
    fn link_preserves_origin_and_bootstraps_equal_cloud_refs() {
        let local = tempfile::tempdir().unwrap();
        let remote = tempfile::tempdir().unwrap();
        init_repo(local.path(), "# Initial\n");
        run(remote.path(), &["init", "--bare", "--initial-branch=main"]);
        run(
            local.path(),
            &[
                "remote",
                "add",
                "origin",
                "https://example.com/original.git",
            ],
        );
        let user = Uuid::new_v4();

        configure_cowiki_remote(local.path(), remote.path().to_str().unwrap(), user).unwrap();
        bootstrap_remote(local.path(), "fixture-token", user).unwrap();

        assert_eq!(
            output(local.path(), &["remote", "get-url", "origin"]),
            "https://example.com/original.git"
        );
        assert_eq!(
            output(remote.path(), &["rev-parse", "refs/heads/main"]),
            output(
                remote.path(),
                &["rev-parse", &format!("refs/heads/user/{user}")]
            )
        );
    }

    #[test]
    fn dirty_auto_sync_is_a_no_op() {
        let local = tempfile::tempdir().unwrap();
        let remote = tempfile::tempdir().unwrap();
        init_repo(local.path(), "# Initial\n");
        run(remote.path(), &["init", "--bare", "--initial-branch=main"]);
        let user = Uuid::new_v4();
        configure_cowiki_remote(local.path(), remote.path().to_str().unwrap(), user).unwrap();
        bootstrap_remote(local.path(), "fixture-token", user).unwrap();
        let before = output(local.path(), &["rev-parse", "HEAD"]);
        std::fs::write(local.path().join("draft.md"), "# Unsaved\n").unwrap();

        let result = sync_if_clean_path(local.path(), "fixture-token").unwrap();

        assert_eq!(result.state, SyncState::Dirty);
        assert_eq!(output(local.path(), &["rev-parse", "HEAD"]), before);
        assert_eq!(
            std::fs::read_to_string(local.path().join("draft.md")).unwrap(),
            "# Unsaved\n"
        );
    }

    #[test]
    fn user_branch_push_rejects_a_stale_force_with_lease() {
        let local = tempfile::tempdir().unwrap();
        let remote = tempfile::tempdir().unwrap();
        let collaborator = tempfile::tempdir().unwrap();
        init_repo(local.path(), "# Initial\n");
        run(remote.path(), &["init", "--bare", "--initial-branch=main"]);
        let user = Uuid::new_v4();
        configure_cowiki_remote(local.path(), remote.path().to_str().unwrap(), user).unwrap();
        bootstrap_remote(local.path(), "fixture-token", user).unwrap();

        run(
            collaborator.path(),
            &["clone", remote.path().to_str().unwrap(), "."],
        );
        run(
            collaborator.path(),
            &["config", "user.name", "Other device"],
        );
        run(
            collaborator.path(),
            &["config", "user.email", "other@cowiki.local"],
        );
        std::fs::write(collaborator.path().join("other.md"), "# Other device\n").unwrap();
        run(collaborator.path(), &["add", "other.md"]);
        run(collaborator.path(), &["commit", "-m", "other device"]);
        run(
            collaborator.path(),
            &["push", "origin", &format!("main:refs/heads/user/{user}")],
        );

        std::fs::write(local.path().join("local.md"), "# Local device\n").unwrap();
        run(local.path(), &["add", "local.md"]);
        run(local.path(), &["commit", "-m", "local device"]);

        let result = push_user_branch(local.path(), "fixture-token", user).unwrap();

        assert_eq!(result.state, SyncState::LeaseRejected);
        assert_eq!(
            output(remote.path(), &["show", &format!("user/{user}:other.md")]),
            "# Other device"
        );
    }

    #[test]
    fn rebase_conflicts_remain_visible_and_can_be_aborted() {
        let local = tempfile::tempdir().unwrap();
        let remote = tempfile::tempdir().unwrap();
        let collaborator = tempfile::tempdir().unwrap();
        init_repo(local.path(), "# Base\n");
        run(remote.path(), &["init", "--bare", "--initial-branch=main"]);
        let user = Uuid::new_v4();
        configure_cowiki_remote(local.path(), remote.path().to_str().unwrap(), user).unwrap();
        bootstrap_remote(local.path(), "fixture-token", user).unwrap();

        std::fs::write(local.path().join("index.md"), "# Local\n").unwrap();
        run(local.path(), &["commit", "-am", "local"]);
        run(
            collaborator.path(),
            &["clone", remote.path().to_str().unwrap(), "."],
        );
        run(collaborator.path(), &["config", "user.name", "Cloud"]);
        run(
            collaborator.path(),
            &["config", "user.email", "cloud@cowiki.local"],
        );
        std::fs::write(collaborator.path().join("index.md"), "# Cloud\n").unwrap();
        run(collaborator.path(), &["commit", "-am", "cloud"]);
        run(collaborator.path(), &["push", "origin", "main"]);

        let result = sync_if_clean_path(local.path(), "fixture-token").unwrap();
        assert_eq!(result.state, SyncState::Conflicted);
        assert_eq!(result.conflicts, vec!["index.md"]);
        assert!(git_path(local.path(), "rebase-merge").exists());

        let aborted = rebase_abort_path(local.path()).unwrap();
        assert_eq!(aborted.state, SyncState::NeedsSync);
        assert!(!git_path(local.path(), "rebase-merge").exists());
    }

    fn init_repo(path: &Path, content: &str) {
        run(path, &["init", "-b", "main"]);
        run(path, &["config", "user.name", "Local"]);
        run(path, &["config", "user.email", "local@cowiki.local"]);
        std::fs::write(path.join("index.md"), content).unwrap();
        run(path, &["add", "index.md"]);
        run(path, &["commit", "-m", "initial"]);
    }

    fn git_path(path: &Path, name: &str) -> std::path::PathBuf {
        path.join(output(path, &["rev-parse", "--git-path", name]))
    }

    fn output(directory: &Path, arguments: &[&str]) -> String {
        let output = Command::new("git")
            .args(arguments)
            .current_dir(directory)
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    }

    fn run(directory: &Path, arguments: &[&str]) {
        let output = Command::new("git")
            .args(arguments)
            .current_dir(directory)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {} failed: {}",
            arguments.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
