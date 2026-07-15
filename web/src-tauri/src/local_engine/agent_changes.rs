use super::{safe_repo_path, text_file_diff, FileDiff, LocalEngine};
use git2::{Oid, Repository, Signature, StatusOptions};
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const BASE_REF_PREFIX: &str = "refs/cowiki/agent-bases/";
const BRANCH_REF_PREFIX: &str = "refs/heads/cowiki/agent/";
const CONFLICT_REF_PREFIX: &str = "refs/cowiki/agent-conflicts/";
const MERGED_REF_PREFIX: &str = "refs/cowiki/agent-merged/";
const DISCARDED_REF_PREFIX: &str = "refs/cowiki/agent-discarded/";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentChange {
    pub id: String,
    pub title: String,
    pub status: String,
    pub created_at: i64,
    pub worktree_path: PathBuf,
    pub diffs: Vec<FileDiff>,
}

impl LocalEngine {
    pub fn create_agent_change(
        &self,
        space_slug: &str,
        agent_name: &str,
    ) -> Result<AgentChange, String> {
        let space = self.find_space(space_slug)?;
        let repo = Repository::open(&space.local_path).map_err(|error| error.to_string())?;
        let parent = repo
            .head()
            .and_then(|head| head.peel_to_commit())
            .map_err(|_| "the Space needs a Git checkpoint before running an Agent".to_string())?;
        let tree_id = snapshot_worktree(&repo)?;
        let tree = repo.find_tree(tree_id).map_err(|error| error.to_string())?;
        let signature = Signature::now("CoWiki Agent", "agent@cowiki.app")
            .map_err(|error| error.to_string())?;
        let id = uuid::Uuid::new_v4().to_string();
        let title = clean_agent_name(agent_name);
        let base_ref = base_ref(&id);
        let base_id = repo
            .commit(
                Some(&base_ref),
                &signature,
                &signature,
                &format!("CoWiki Agent Change: {title}"),
                &tree,
                &[&parent],
            )
            .map_err(|error| error.to_string())?;
        let branch_ref = branch_ref(&id);
        if let Err(error) =
            repo.reference(&branch_ref, base_id, false, "Create CoWiki Agent Change")
        {
            delete_reference_if_present(&repo, &base_ref);
            return Err(error.to_string());
        }

        let worktree_path = self.change_worktree_path(&space.id, &id);
        if let Some(parent) = worktree_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let branch = repo
            .find_reference(&branch_ref)
            .map_err(|error| error.to_string())?;
        let mut options = git2::WorktreeAddOptions::new();
        options.reference(Some(&branch));
        if let Err(error) = repo.worktree(&worktree_name(&id), &worktree_path, Some(&options)) {
            delete_reference_if_present(&repo, &branch_ref);
            delete_reference_if_present(&repo, &base_ref);
            return Err(error.to_string());
        }

        self.agent_change(space_slug, &id)
    }

    pub fn list_agent_changes(&self, space_slug: &str) -> Result<Vec<AgentChange>, String> {
        let repo = self.repo(space_slug)?;
        let mut ids = Vec::new();
        let references = repo
            .references_glob(&format!("{BASE_REF_PREFIX}*"))
            .map_err(|error| error.to_string())?;
        for reference in references {
            let reference = reference.map_err(|error| error.to_string())?;
            if let Some(id) = reference
                .name()
                .and_then(|name| name.strip_prefix(BASE_REF_PREFIX))
            {
                if uuid::Uuid::parse_str(id).is_ok() {
                    ids.push(id.to_string());
                }
            }
        }
        drop(repo);

        let mut changes = ids
            .iter()
            .map(|id| self.agent_change(space_slug, id))
            .collect::<Result<Vec<_>, _>>()?;
        changes.sort_by(|left, right| {
            right
                .created_at
                .cmp(&left.created_at)
                .then_with(|| right.id.cmp(&left.id))
        });
        Ok(changes)
    }

    pub fn merge_agent_change(
        &self,
        space_slug: &str,
        change_id: &str,
    ) -> Result<AgentChange, String> {
        validate_change_id(change_id)?;
        let space = self.find_space(space_slug)?;
        let repo = Repository::open(&space.local_path).map_err(|error| error.to_string())?;
        ensure_active_change(&repo, change_id)?;
        let result_id = self.capture_agent_result(&space.id, &repo, change_id)?;
        let base_commit = reference_commit(&repo, &base_ref(change_id))?;
        let result_commit = repo
            .find_commit(result_id)
            .map_err(|error| error.to_string())?;
        let draft_tree_id = snapshot_worktree(&repo)?;
        let draft_tree = repo
            .find_tree(draft_tree_id)
            .map_err(|error| error.to_string())?;
        let base_tree = base_commit.tree().map_err(|error| error.to_string())?;
        let result_tree = result_commit.tree().map_err(|error| error.to_string())?;
        let mut merged_index = repo
            .merge_trees(&base_tree, &draft_tree, &result_tree, None)
            .map_err(|error| error.to_string())?;

        if merged_index.has_conflicts() {
            repo.reference(
                &conflict_ref(change_id),
                result_id,
                true,
                "CoWiki Agent Change needs resolution",
            )
            .map_err(|error| error.to_string())?;
            drop(result_tree);
            drop(base_tree);
            drop(draft_tree);
            drop(result_commit);
            drop(base_commit);
            drop(repo);
            return self.agent_change(space_slug, change_id);
        }

        let merged_tree_id = merged_index
            .write_tree_to(&repo)
            .map_err(|error| error.to_string())?;
        let merged_tree = repo
            .find_tree(merged_tree_id)
            .map_err(|error| error.to_string())?;
        apply_tree_transition(&repo, &space.local_path, &draft_tree, &merged_tree)?;
        repo.reference(
            &merged_ref(change_id),
            result_id,
            true,
            "Merge CoWiki Agent Change into Draft",
        )
        .map_err(|error| error.to_string())?;
        delete_reference_if_present(&repo, &conflict_ref(change_id));
        drop(merged_tree);
        drop(result_tree);
        drop(base_tree);
        drop(draft_tree);
        drop(result_commit);
        drop(base_commit);
        drop(repo);
        self.agent_change(space_slug, change_id)
    }

    pub fn discard_agent_change(
        &self,
        space_slug: &str,
        change_id: &str,
    ) -> Result<AgentChange, String> {
        validate_change_id(change_id)?;
        let space = self.find_space(space_slug)?;
        let repo = Repository::open(&space.local_path).map_err(|error| error.to_string())?;
        ensure_active_change(&repo, change_id)?;
        let result_id = self.capture_agent_result(&space.id, &repo, change_id)?;
        repo.reference(
            &discarded_ref(change_id),
            result_id,
            true,
            "Discard CoWiki Agent Change",
        )
        .map_err(|error| error.to_string())?;
        delete_reference_if_present(&repo, &conflict_ref(change_id));
        drop(repo);
        self.agent_change(space_slug, change_id)
    }

    pub fn agent_change_worktree(
        &self,
        space_slug: &str,
        change_id: &str,
    ) -> Result<PathBuf, String> {
        validate_change_id(change_id)?;
        let space = self.find_space(space_slug)?;
        let repo = Repository::open(&space.local_path).map_err(|error| error.to_string())?;
        reference_commit(&repo, &base_ref(change_id))?;
        let worktree_path = self.change_worktree_path(&space.id, change_id);
        worktree_path
            .canonicalize()
            .map_err(|error| format!("Agent Change worktree is unavailable: {error}"))
    }

    fn agent_change(&self, space_slug: &str, change_id: &str) -> Result<AgentChange, String> {
        validate_change_id(change_id)?;
        let space = self.find_space(space_slug)?;
        let repo = Repository::open(&space.local_path).map_err(|error| error.to_string())?;
        let status = change_status(&repo, change_id);
        if status == "open" || status == "needsResolution" {
            self.capture_agent_result(&space.id, &repo, change_id)?;
        }
        let base_commit = reference_commit(&repo, &base_ref(change_id))?;
        let result_commit = reference_commit(&repo, &branch_ref(change_id))?;
        let base_tree = base_commit.tree().map_err(|error| error.to_string())?;
        let result_tree = result_commit.tree().map_err(|error| error.to_string())?;
        let diffs = tree_diffs(&repo, &base_tree, &result_tree)?;
        let title = base_commit
            .summary()
            .and_then(|summary| summary.strip_prefix("CoWiki Agent Change: "))
            .unwrap_or("Agent Change")
            .to_string();
        Ok(AgentChange {
            id: change_id.to_string(),
            title,
            status: change_status(&repo, change_id).to_string(),
            created_at: base_commit.time().seconds(),
            worktree_path: self.change_worktree_path(&space.id, change_id),
            diffs,
        })
    }

    fn capture_agent_result(
        &self,
        space_id: &str,
        repo: &Repository,
        change_id: &str,
    ) -> Result<Oid, String> {
        let branch_ref = branch_ref(change_id);
        let current = reference_commit(repo, &branch_ref)?;
        let worktree_path = self.change_worktree_path(space_id, change_id);
        if !worktree_path.is_dir() {
            return Ok(current.id());
        }
        let worktree_repo = Repository::open(&worktree_path).map_err(|error| error.to_string())?;
        let head_name = worktree_repo
            .head()
            .ok()
            .and_then(|head| head.name().map(ToOwned::to_owned));
        if head_name.as_deref() != Some(branch_ref.as_str()) {
            return Err("Agent Change left its managed Git branch".to_string());
        }
        let tree_id = snapshot_worktree(&worktree_repo)?;
        if tree_id == current.tree_id() {
            return Ok(current.id());
        }
        let tree = repo.find_tree(tree_id).map_err(|error| error.to_string())?;
        let signature = Signature::now("CoWiki Agent", "agent@cowiki.app")
            .map_err(|error| error.to_string())?;
        repo.commit(
            Some(&branch_ref),
            &signature,
            &signature,
            "Capture CoWiki Agent result",
            &tree,
            &[&current],
        )
        .map_err(|error| error.to_string())
    }

    fn change_worktree_path(&self, space_id: &str, change_id: &str) -> PathBuf {
        self.metadata_dir
            .join("agent-worktrees")
            .join(space_id)
            .join(change_id)
    }
}

fn snapshot_worktree(repo: &Repository) -> Result<Oid, String> {
    let root = repo
        .workdir()
        .ok_or_else(|| "Git worktree has no working directory".to_string())?;
    let head = repo
        .head()
        .and_then(|head| head.peel_to_commit())
        .map_err(|error| error.to_string())?;
    let head_tree = head.tree().map_err(|error| error.to_string())?;
    // This is a detached in-memory view of the repository's index. It is
    // deliberately never written, so staged user state remains byte-for-byte
    // untouched while libgit2 writes only new blob/tree objects.
    let mut index = repo.index().map_err(|error| error.to_string())?;
    index
        .read_tree(&head_tree)
        .map_err(|error| error.to_string())?;
    let mut options = StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false);
    let statuses = repo
        .statuses(Some(&mut options))
        .map_err(|error| error.to_string())?;
    for status in statuses.iter() {
        let Some(relative) = status.path() else {
            continue;
        };
        let relative = safe_repo_path(relative)?;
        if std::fs::symlink_metadata(root.join(relative)).is_ok() {
            index
                .add_path(relative)
                .map_err(|error| error.to_string())?;
        } else {
            let _ = index.remove_path(relative);
        }
    }
    index.write_tree().map_err(|error| error.to_string())
}

fn tree_diffs(
    repo: &Repository,
    old_tree: &git2::Tree<'_>,
    new_tree: &git2::Tree<'_>,
) -> Result<Vec<FileDiff>, String> {
    let diff = repo
        .diff_tree_to_tree(Some(old_tree), Some(new_tree), None)
        .map_err(|error| error.to_string())?;
    let mut paths = BTreeSet::new();
    for delta in diff.deltas() {
        if let Some(path) = delta.old_file().path() {
            paths.insert(path.to_path_buf());
        }
        if let Some(path) = delta.new_file().path() {
            paths.insert(path.to_path_buf());
        }
    }
    let mut result = Vec::new();
    for path in paths {
        let path_text = path
            .to_str()
            .ok_or_else(|| "Agent Change contains a non-UTF-8 path".to_string())?;
        if safe_repo_path(path_text).is_err() {
            continue;
        }
        let old_content = tree_text(repo, old_tree, &path)?;
        let new_content = tree_text(repo, new_tree, &path)?;
        if old_content.is_none() && new_content.is_none() {
            continue;
        }
        result.push(text_file_diff(path_text, old_content, new_content));
    }
    Ok(result)
}

fn apply_tree_transition(
    repo: &Repository,
    root: &Path,
    from_tree: &git2::Tree<'_>,
    to_tree: &git2::Tree<'_>,
) -> Result<(), String> {
    let diff = repo
        .diff_tree_to_tree(Some(from_tree), Some(to_tree), None)
        .map_err(|error| error.to_string())?;
    let mut paths = BTreeSet::new();
    for delta in diff.deltas() {
        if let Some(path) = delta.old_file().path() {
            paths.insert(path.to_path_buf());
        }
        if let Some(path) = delta.new_file().path() {
            paths.insert(path.to_path_buf());
        }
    }

    let mut operations = Vec::new();
    for relative in paths {
        let relative_text = relative
            .to_str()
            .ok_or_else(|| "merged Agent Change contains a non-UTF-8 path".to_string())?;
        safe_repo_path(relative_text)?;
        let path = root.join(&relative);
        let expected = tree_bytes(repo, from_tree, &relative)?;
        let actual = read_optional_file(&path)?;
        if actual != expected {
            return Err(
                "The current Draft changed while the Agent Change was merging. Retry the merge."
                    .to_string(),
            );
        }
        let target = tree_bytes(repo, to_tree, &relative)?;
        operations.push((path, actual, target));
    }

    let mut applied = 0usize;
    for (path, _, target) in &operations {
        if let Err(error) = write_optional_file(path, target.as_deref()) {
            for (rollback_path, previous, _) in operations[..applied].iter().rev() {
                let _ = write_optional_file(rollback_path, previous.as_deref());
            }
            return Err(error);
        }
        applied += 1;
    }
    Ok(())
}

fn tree_text(
    repo: &Repository,
    tree: &git2::Tree<'_>,
    path: &Path,
) -> Result<Option<String>, String> {
    Ok(tree_bytes(repo, tree, path)?.and_then(|bytes| String::from_utf8(bytes).ok()))
}

fn tree_bytes(
    repo: &Repository,
    tree: &git2::Tree<'_>,
    path: &Path,
) -> Result<Option<Vec<u8>>, String> {
    let Ok(entry) = tree.get_path(path) else {
        return Ok(None);
    };
    let blob = repo.find_blob(entry.id()).map_err(|error| {
        format!(
            "Agent Change path '{}' is not a regular file: {error}",
            path.display()
        )
    })?;
    Ok(Some(blob.content().to_vec()))
}

fn read_optional_file(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

fn write_optional_file(path: &Path, content: Option<&[u8]>) -> Result<(), String> {
    match content {
        Some(content) => {
            let parent = path
                .parent()
                .ok_or_else(|| "merged path has no parent directory".to_string())?;
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            let temporary = parent.join(format!(
                ".cowiki-merge-{}.tmp",
                uuid::Uuid::new_v4().simple()
            ));
            std::fs::write(&temporary, content).map_err(|error| error.to_string())?;
            if let Err(error) = std::fs::rename(&temporary, path) {
                let _ = std::fs::remove_file(&temporary);
                return Err(error.to_string());
            }
            Ok(())
        }
        None => match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        },
    }
}

fn reference_commit<'repo>(
    repo: &'repo Repository,
    reference_name: &str,
) -> Result<git2::Commit<'repo>, String> {
    repo.find_reference(reference_name)
        .and_then(|reference| reference.peel_to_commit())
        .map_err(|_| "Agent Change does not exist".to_string())
}

fn ensure_active_change(repo: &Repository, change_id: &str) -> Result<(), String> {
    reference_commit(repo, &base_ref(change_id))?;
    match change_status(repo, change_id) {
        "merged" => Err("Agent Change is already merged".to_string()),
        "discarded" => Err("Agent Change is already discarded".to_string()),
        _ => Ok(()),
    }
}

fn change_status(repo: &Repository, change_id: &str) -> &'static str {
    if repo.find_reference(&discarded_ref(change_id)).is_ok() {
        "discarded"
    } else if repo.find_reference(&merged_ref(change_id)).is_ok() {
        "merged"
    } else if repo.find_reference(&conflict_ref(change_id)).is_ok() {
        "needsResolution"
    } else {
        "open"
    }
}

fn clean_agent_name(value: &str) -> String {
    let value = value.lines().next().unwrap_or_default().trim();
    if value.is_empty() {
        "Agent Change".to_string()
    } else {
        value.chars().take(80).collect()
    }
}

fn validate_change_id(change_id: &str) -> Result<(), String> {
    uuid::Uuid::parse_str(change_id)
        .map(|_| ())
        .map_err(|_| "invalid Agent Change id".to_string())
}

fn base_ref(change_id: &str) -> String {
    format!("{BASE_REF_PREFIX}{change_id}")
}

fn branch_ref(change_id: &str) -> String {
    format!("{BRANCH_REF_PREFIX}{change_id}")
}

fn conflict_ref(change_id: &str) -> String {
    format!("{CONFLICT_REF_PREFIX}{change_id}")
}

fn merged_ref(change_id: &str) -> String {
    format!("{MERGED_REF_PREFIX}{change_id}")
}

fn discarded_ref(change_id: &str) -> String {
    format!("{DISCARDED_REF_PREFIX}{change_id}")
}

fn worktree_name(change_id: &str) -> String {
    format!("cowiki-agent-{change_id}")
}

fn delete_reference_if_present(repo: &Repository, name: &str) {
    if let Ok(mut reference) = repo.find_reference(name) {
        let _ = reference.delete();
    }
}
