use super::{
    agent_provenance_from_message, file_diff_from_bytes, safe_repo_path, AgentProvenance, FileDiff,
    LocalEngine,
};
use git2::{Oid, Repository, Signature, StatusOptions};
use serde::Serialize;
use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const BASE_REF_PREFIX: &str = "refs/cowiki/agent-bases/";
const BRANCH_REF_PREFIX: &str = "refs/heads/agent/";
const CONFLICT_REF_PREFIX: &str = "refs/cowiki/agent-conflicts/";
const MERGED_REF_PREFIX: &str = "refs/cowiki/agent-merged/";
const DISCARDED_REF_PREFIX: &str = "refs/cowiki/agent-discarded/";
const INTEGRATED_REF_PREFIX: &str = "refs/cowiki/agent-integrated/";

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
    #[cfg(test)]
    pub fn create_agent_change(
        &self,
        space_slug: &str,
        agent_name: &str,
    ) -> Result<AgentChange, String> {
        self.create_agent_change_with_identity(space_slug, agent_name, agent_name)
    }

    pub fn create_agent_change_with_identity(
        &self,
        space_slug: &str,
        title: &str,
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
        let title = clean_agent_name(title);
        let agent_name = clean_agent_name(agent_name);
        let base_ref = base_ref(&id);
        let message = format!(
            "CoWiki Agent Change: {title}\n\nCoWiki-Agent: {agent_name}\nCoWiki-Agent-Change: {id}\nCoWiki-Agent-Task: {title}"
        );
        let base_id = repo
            .commit(
                Some(&base_ref),
                &signature,
                &signature,
                &message,
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

        let worktree_path = match self.prepare_change_worktree_path(&space.id, &id) {
            Ok(path) => path,
            Err(error) => {
                delete_reference_if_present(&repo, &branch_ref);
                delete_reference_if_present(&repo, &base_ref);
                return Err(error);
            }
        };
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
        let space = self.find_space(space_slug)?;
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
        for id in &ids {
            if matches!(change_status(&repo, id), "merged" | "discarded") {
                if let Err(error) = self.cleanup_agent_worktree(&repo, &space.id, id) {
                    warn_cleanup_failure(id, &error);
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
        self.merge_agent_change_internal(
            space_slug,
            change_id,
            |_| Ok(()),
            |engine, repo, space_id, id| engine.cleanup_agent_worktree(repo, space_id, id),
        )
    }

    #[cfg(test)]
    pub fn merge_agent_change_with_pre_write_hook<F>(
        &self,
        space_slug: &str,
        change_id: &str,
        before_write: F,
    ) -> Result<AgentChange, String>
    where
        F: FnMut(&Path) -> Result<(), String>,
    {
        self.merge_agent_change_internal(
            space_slug,
            change_id,
            before_write,
            |engine, repo, space_id, id| engine.cleanup_agent_worktree(repo, space_id, id),
        )
    }

    #[cfg(test)]
    pub fn merge_agent_change_with_cleanup_hook<C>(
        &self,
        space_slug: &str,
        change_id: &str,
        mut cleanup: C,
    ) -> Result<AgentChange, String>
    where
        C: FnMut() -> Result<(), String>,
    {
        self.merge_agent_change_internal(
            space_slug,
            change_id,
            |_| Ok(()),
            move |_, _, _, _| cleanup(),
        )
    }

    fn merge_agent_change_internal<F, C>(
        &self,
        space_slug: &str,
        change_id: &str,
        mut before_write: F,
        mut cleanup: C,
    ) -> Result<AgentChange, String>
    where
        F: FnMut(&Path) -> Result<(), String>,
        C: FnMut(&LocalEngine, &Repository, &str, &str) -> Result<(), String>,
    {
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
        apply_tree_transition(
            &repo,
            &space.local_path,
            &draft_tree,
            &merged_tree,
            &mut before_write,
        )?;
        repo.reference(
            &merged_ref(change_id),
            result_id,
            true,
            "Merge CoWiki Agent Change into Draft",
        )
        .map_err(|error| error.to_string())?;
        delete_reference_if_present(&repo, &conflict_ref(change_id));
        if let Err(error) = cleanup(self, &repo, &space.id, change_id) {
            warn_cleanup_failure(change_id, &error);
        }
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
        self.discard_agent_change_internal(space_slug, change_id, |engine, repo, space_id, id| {
            engine.cleanup_agent_worktree(repo, space_id, id)
        })
    }

    #[cfg(test)]
    pub fn discard_agent_change_with_cleanup_hook<C>(
        &self,
        space_slug: &str,
        change_id: &str,
        mut cleanup: C,
    ) -> Result<AgentChange, String>
    where
        C: FnMut() -> Result<(), String>,
    {
        self.discard_agent_change_internal(space_slug, change_id, move |_, _, _, _| cleanup())
    }

    fn discard_agent_change_internal<C>(
        &self,
        space_slug: &str,
        change_id: &str,
        mut cleanup: C,
    ) -> Result<AgentChange, String>
    where
        C: FnMut(&LocalEngine, &Repository, &str, &str) -> Result<(), String>,
    {
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
        if let Err(error) = cleanup(self, &repo, &space.id, change_id) {
            warn_cleanup_failure(change_id, &error);
        }
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
        ensure_active_change(&repo, change_id)?;
        self.validate_managed_worktree_path(&space.id, change_id)
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
        let worktree_path = self.validate_managed_worktree_path(space_id, change_id)?;
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
            .canonicalize()
            .unwrap_or_else(|_| self.metadata_dir.clone())
            .join("agent-worktrees")
            .join(space_id)
            .join(change_id)
    }

    fn prepare_change_worktree_path(
        &self,
        space_id: &str,
        change_id: &str,
    ) -> Result<PathBuf, String> {
        validate_change_id(change_id)?;
        let metadata = self
            .metadata_dir
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let parent = ensure_safe_directory_tree(
            &metadata,
            Path::new("agent-worktrees").join(space_id).as_path(),
        )?;
        let path = parent.join(change_id);
        if std::fs::symlink_metadata(&path).is_ok() {
            return Err("Agent Change worktree path already exists".to_string());
        }
        Ok(path)
    }

    fn validate_managed_worktree_path(
        &self,
        space_id: &str,
        change_id: &str,
    ) -> Result<PathBuf, String> {
        let metadata = self
            .metadata_dir
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let expected = metadata
            .join("agent-worktrees")
            .join(space_id)
            .join(change_id);
        validate_no_symlink_components(&metadata, &expected)?;
        let canonical = expected
            .canonicalize()
            .map_err(|error| format!("Agent Change worktree is unavailable: {error}"))?;
        if canonical != expected || !canonical.starts_with(&metadata) || !canonical.is_dir() {
            return Err("Agent Change worktree escaped its managed location".to_string());
        }
        Ok(canonical)
    }

    fn cleanup_agent_worktree(
        &self,
        repo: &Repository,
        space_id: &str,
        change_id: &str,
    ) -> Result<(), String> {
        let worktree_path = self.change_worktree_path(space_id, change_id);
        match std::fs::symlink_metadata(&worktree_path) {
            Ok(_) => {
                let worktree_path = self.validate_managed_worktree_path(space_id, change_id)?;
                std::fs::remove_dir_all(&worktree_path).map_err(|error| error.to_string())?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
        match repo.find_worktree(&worktree_name(change_id)) {
            Ok(worktree) => worktree.prune(None).map_err(|error| error.to_string()),
            Err(error) if error.code() == git2::ErrorCode::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }
}

pub(super) fn pending_merged_agent_contributions(
    repo: &Repository,
    parent: Option<&git2::Commit<'_>>,
    committed_tree: &git2::Tree<'_>,
) -> Result<Vec<AgentProvenance>, String> {
    let parent_tree = parent
        .map(|commit| commit.tree().map_err(|error| error.to_string()))
        .transpose()?;
    let committed_paths = changed_paths(repo, parent_tree.as_ref(), committed_tree)?;
    if committed_paths.is_empty() {
        return Ok(Vec::new());
    }
    let mut contributions = Vec::new();
    let references = repo
        .references_glob(&format!("{MERGED_REF_PREFIX}*"))
        .map_err(|error| error.to_string())?;
    for reference in references {
        let reference = reference.map_err(|error| error.to_string())?;
        let Some(name) = reference.name() else {
            continue;
        };
        let Some(change_id) = name.strip_prefix(MERGED_REF_PREFIX) else {
            continue;
        };
        if repo.find_reference(&integrated_ref(change_id)).is_ok() {
            continue;
        }
        let base = reference_commit(repo, &base_ref(change_id))?;
        let result = reference
            .peel_to_commit()
            .map_err(|error| error.to_string())?;
        let base_tree = base.tree().map_err(|error| error.to_string())?;
        let result_tree = result.tree().map_err(|error| error.to_string())?;
        let agent_paths = changed_paths(repo, Some(&base_tree), &result_tree)?;
        if !agent_paths
            .iter()
            .any(|path| committed_paths.contains(path))
        {
            continue;
        }
        let mut agents = agent_provenance_from_message(base.message().unwrap_or_default());
        if let Some(agent) = agents.pop() {
            contributions.push(agent);
        }
    }
    contributions.sort_by(|left, right| left.change_id.cmp(&right.change_id));
    Ok(contributions)
}

pub(super) fn mark_agent_contributions_committed(
    repo: &Repository,
    commit_oid: Oid,
    contributions: &[AgentProvenance],
) {
    for contribution in contributions {
        if let Err(error) = repo.reference(
            &integrated_ref(&contribution.change_id),
            commit_oid,
            true,
            "Record portable CoWiki Agent provenance",
        ) {
            eprintln!(
                "CoWiki committed Agent Change '{}' but could not record its local integration marker: {error}",
                contribution.change_id
            );
        }
    }
}

fn changed_paths(
    repo: &Repository,
    old_tree: Option<&git2::Tree<'_>>,
    new_tree: &git2::Tree<'_>,
) -> Result<BTreeSet<PathBuf>, String> {
    let diff = repo
        .diff_tree_to_tree(old_tree, Some(new_tree), None)
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
    Ok(paths)
}

fn warn_cleanup_failure(change_id: &str, error: &str) {
    eprintln!(
        "CoWiki closed Agent Change '{change_id}' but could not clean its worktree; cleanup will retry when Reviews reload: {error}"
    );
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
        let Ok(relative) = safe_repo_path(relative) else {
            continue;
        };
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
        let old_bytes = tree_bytes(repo, old_tree, &path)?;
        let new_bytes = tree_bytes(repo, new_tree, &path)?;
        if old_bytes.is_none() && new_bytes.is_none() {
            continue;
        }
        result.push(file_diff_from_bytes(path_text, old_bytes, new_bytes));
    }
    Ok(result)
}

fn apply_tree_transition<F>(
    repo: &Repository,
    root: &Path,
    from_tree: &git2::Tree<'_>,
    to_tree: &git2::Tree<'_>,
    before_write: &mut F,
) -> Result<(), String>
where
    F: FnMut(&Path) -> Result<(), String>,
{
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
        let expected = tree_bytes(repo, from_tree, &relative)?;
        let actual = read_optional_file_no_follow(root, &relative)?;
        if actual != expected {
            return Err(
                "The current Draft changed while the Agent Change was merging. Retry the merge."
                    .to_string(),
            );
        }
        let target = tree_bytes(repo, to_tree, &relative)?;
        operations.push((relative, actual, target));
    }

    for (applied, (relative, previous, target)) in operations.iter().enumerate() {
        before_write(&root.join(relative))?;
        if let Err(error) =
            write_optional_file_cas(root, relative, previous.as_deref(), target.as_deref())
        {
            let mut rollback_errors = Vec::new();
            for (rollback_path, rollback_previous, rollback_target) in
                operations[..applied].iter().rev()
            {
                if let Err(rollback_error) = write_optional_file_cas(
                    root,
                    rollback_path,
                    rollback_target.as_deref(),
                    rollback_previous.as_deref(),
                ) {
                    rollback_errors.push(format!("{}: {rollback_error}", rollback_path.display()));
                }
            }
            if !rollback_errors.is_empty() {
                return Err(format!(
                    "{error}; concurrent changes prevented rollback for {}",
                    rollback_errors.join(", ")
                ));
            }
            return Err(error);
        }
    }
    Ok(())
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

fn read_optional_file_no_follow(root: &Path, relative: &Path) -> Result<Option<Vec<u8>>, String> {
    let path = root.join(relative);
    validate_no_symlink_components(root, &path)?;
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "symbolic link target is not allowed while merging: {}",
            path.display()
        ));
    }
    if !metadata.is_file() {
        return Err(format!(
            "merged path is not a regular file: {}",
            path.display()
        ));
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options.open(&path).map_err(|error| error.to_string())?;
    if !file
        .metadata()
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err(format!(
            "merged path is not a regular file: {}",
            path.display()
        ));
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    Ok(Some(bytes))
}

fn write_optional_file_cas(
    root: &Path,
    relative: &Path,
    expected: Option<&[u8]>,
    content: Option<&[u8]>,
) -> Result<(), String> {
    let path = root.join(relative);
    if content.is_some() {
        ensure_safe_parent_directories(root, relative)?;
    }
    ensure_expected_content(root, relative, expected)?;
    match content {
        Some(content) => {
            let parent = path
                .parent()
                .ok_or_else(|| "merged path has no parent directory".to_string())?;
            validate_no_symlink_components(root, parent)?;
            let temporary = parent.join(format!(
                ".cowiki-merge-{}.tmp",
                uuid::Uuid::new_v4().simple()
            ));
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
            }
            let mut file = options
                .open(&temporary)
                .map_err(|error| error.to_string())?;
            if let Err(error) = file.write_all(content).and_then(|_| file.flush()) {
                drop(file);
                let _ = std::fs::remove_file(&temporary);
                return Err(error.to_string());
            }
            drop(file);
            validate_no_symlink_components(root, parent)?;
            if let Err(error) = ensure_expected_content(root, relative, expected) {
                let _ = std::fs::remove_file(&temporary);
                return Err(error);
            }
            if let Err(error) = std::fs::rename(&temporary, &path) {
                let _ = std::fs::remove_file(&temporary);
                return Err(error.to_string());
            }
            Ok(())
        }
        None => {
            ensure_expected_content(root, relative, expected)?;
            match std::fs::remove_file(&path) {
                Ok(()) => Ok(()),
                Err(error)
                    if error.kind() == std::io::ErrorKind::NotFound && expected.is_none() =>
                {
                    Ok(())
                }
                Err(error) => Err(error.to_string()),
            }
        }
    }
}

fn ensure_expected_content(
    root: &Path,
    relative: &Path,
    expected: Option<&[u8]>,
) -> Result<(), String> {
    let current = read_optional_file_no_follow(root, relative)?;
    if current.as_deref() != expected {
        return Err(
            "The current Draft changed while the Agent Change was merging. Retry the merge."
                .to_string(),
        );
    }
    Ok(())
}

fn ensure_safe_parent_directories(root: &Path, relative: &Path) -> Result<(), String> {
    let parent = relative
        .parent()
        .ok_or_else(|| "merged path has no parent directory".to_string())?;
    ensure_safe_directory_tree(root, parent).map(|_| ())
}

fn ensure_safe_directory_tree(root: &Path, relative: &Path) -> Result<PathBuf, String> {
    let mut current = root.to_path_buf();
    validate_directory(&current)?;
    for component in relative.components() {
        let std::path::Component::Normal(component) = component else {
            return Err("managed path may not escape its root".to_string());
        };
        current.push(component);
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) => validate_directory_metadata(&current, &metadata)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match std::fs::create_dir(&current) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(error.to_string()),
                }
                validate_directory(&current)?;
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(current)
}

fn validate_no_symlink_components(root: &Path, path: &Path) -> Result<(), String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| "managed path escaped its root".to_string())?;
    validate_directory(root)?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let std::path::Component::Normal(component) = component else {
            return Err("managed path may not escape its root".to_string());
        };
        current.push(component);
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!(
                    "symbolic link is not allowed in managed path: {}",
                    current.display()
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(())
}

fn validate_directory(path: &Path) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    validate_directory_metadata(path, &metadata)
}

fn validate_directory_metadata(path: &Path, metadata: &std::fs::Metadata) -> Result<(), String> {
    if metadata.file_type().is_symlink() {
        Err(format!(
            "symbolic link is not allowed in managed path: {}",
            path.display()
        ))
    } else if !metadata.is_dir() {
        Err(format!(
            "managed path is not a directory: {}",
            path.display()
        ))
    } else {
        Ok(())
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

fn integrated_ref(change_id: &str) -> String {
    format!("{INTEGRATED_REF_PREFIX}{change_id}")
}

fn worktree_name(change_id: &str) -> String {
    format!("cowiki-agent-{change_id}")
}

fn delete_reference_if_present(repo: &Repository, name: &str) {
    if let Ok(mut reference) = repo.find_reference(name) {
        let _ = reference.delete();
    }
}
