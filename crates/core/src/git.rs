use git2::{BranchType, Repository, Signature};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, RwLock};

pub struct WikiRepo {
    path: PathBuf,
    write_locks: RwLock<HashMap<String, Arc<RwLock<()>>>>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FileDiff {
    pub path: String,
    pub old_content: Option<String>,
    pub new_content: Option<String>,
    /// Line-level diff hunks
    pub hunks: Vec<DiffHunk>,
    /// Stats: lines added
    pub additions: usize,
    /// Stats: lines deleted
    pub deletions: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DiffHunk {
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DiffLine {
    /// "add", "del", or "ctx" (context)
    pub kind: String,
    /// Line number in old file (None for added lines)
    pub old_line: Option<usize>,
    /// Line number in new file (None for deleted lines)
    pub new_line: Option<usize>,
    pub text: String,
}

impl FileDiff {
    pub fn is_new(&self) -> bool {
        self.old_content.is_none() && self.new_content.is_some()
    }
}

/// Compute line-level diff between two strings, returning hunks with context lines.
fn compute_line_diff(old: &str, new: &str) -> (Vec<DiffHunk>, usize, usize) {
    use similar::{ChangeTag, TextDiff};

    let diff = TextDiff::from_lines(old, new);
    let mut hunks = Vec::new();
    let mut total_adds = 0usize;
    let mut total_dels = 0usize;

    for hunk in diff.unified_diff().context_radius(3).iter_hunks() {
        let header = hunk.header().to_string();
        let mut lines = Vec::new();
        for change in hunk.iter_changes() {
            let (kind, old_line, new_line) = match change.tag() {
                ChangeTag::Equal => (
                    "ctx",
                    change.old_index().map(|i| i + 1),
                    change.new_index().map(|i| i + 1),
                ),
                ChangeTag::Delete => {
                    total_dels += 1;
                    ("del", change.old_index().map(|i| i + 1), None)
                }
                ChangeTag::Insert => {
                    total_adds += 1;
                    ("add", None, change.new_index().map(|i| i + 1))
                }
            };
            lines.push(DiffLine {
                kind: kind.to_string(),
                old_line,
                new_line,
                text: change.value().trim_end_matches('\n').to_string(),
            });
        }
        hunks.push(DiffHunk { header, lines });
    }

    (hunks, total_adds, total_dels)
}

/// Outcome of merging a submission's `pr/{id}` branch into `main`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeOutcome {
    /// Merged cleanly; `main` advanced by one linear squash commit.
    Merged,
    /// Conflicts with `main`. Nothing was written; carries the conflicting paths.
    Conflict(Vec<String>),
}

/// Outcome of bringing a user branch up to date with `main`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RebaseOutcome {
    /// Branch already contains all of `main`; nothing to do.
    UpToDate,
    /// `main`'s changes were folded into the branch; the branch advanced.
    Updated,
    /// Branch conflicts with `main`. The branch was left **untouched**;
    /// carries the conflicting paths for the UI to surface.
    Conflict(Vec<String>),
}

/// Build a git signature from a user display name. libgit2 rejects names containing
/// angle brackets (and control chars), so we strip those and fall back to a service
/// identity when nothing usable remains — a real user name must never 500 a write.
fn signature(author: &str) -> Result<Signature<'static>, git2::Error> {
    let cleaned: String = author
        .chars()
        .filter(|c| !matches!(c, '<' | '>') && !c.is_control())
        .collect();
    let name = match cleaned.trim() {
        "" => "cowiki",
        n => n,
    };
    Signature::now(name, "noreply@cowiki")
}

/// Set one file `segments` (possibly nested, e.g. `["wiki", "a", "b.md"]`) to `blob`
/// inside `base_tree`, rebuilding only the touched subtrees **in memory** — no working
/// directory, no shared on-disk index. Returns the new tree oid.
fn set_path_in_tree(
    repo: &Repository,
    base_tree: Option<&git2::Tree>,
    segments: &[&str],
    blob: git2::Oid,
) -> Result<git2::Oid, git2::Error> {
    let mut builder = repo.treebuilder(base_tree)?;
    let existing = base_tree.and_then(|t| t.get_name(segments[0]));
    if segments.len() == 1 {
        if existing.as_ref().and_then(|e| e.kind()) == Some(git2::ObjectType::Tree) {
            return Err(git2::Error::from_str(&format!(
                "cannot write file over existing directory '{}'",
                segments[0]
            )));
        }
        builder.insert(segments[0], blob, 0o100644)?;
    } else {
        if existing.as_ref().and_then(|e| e.kind()) == Some(git2::ObjectType::Blob) {
            return Err(git2::Error::from_str(&format!(
                "cannot create directory over existing file '{}'",
                segments[0]
            )));
        }
        let sub_base = existing.and_then(|e| repo.find_tree(e.id()).ok());
        let sub_oid = set_path_in_tree(repo, sub_base.as_ref(), &segments[1..], blob)?;
        builder.insert(segments[0], sub_oid, 0o040000)?;
    }
    builder.write()
}

/// Insert an arbitrary object (blob or subtree) at `segments` inside `base_tree`,
/// rebuilding only the touched subtrees in memory. `filemode` is `0o100644` for files
/// and `0o040000` for trees. Returns the new tree oid.
fn insert_entry_in_tree(
    repo: &Repository,
    base_tree: Option<&git2::Tree>,
    segments: &[&str],
    oid: git2::Oid,
    filemode: i32,
) -> Result<git2::Oid, git2::Error> {
    let mut builder = repo.treebuilder(base_tree)?;
    let existing = base_tree.and_then(|t| t.get_name(segments[0]));
    if segments.len() == 1 {
        builder.insert(segments[0], oid, filemode)?;
    } else {
        if existing.as_ref().and_then(|e| e.kind()) == Some(git2::ObjectType::Blob) {
            return Err(git2::Error::from_str(&format!(
                "cannot create directory over existing file '{}'",
                segments[0]
            )));
        }
        let sub_base = existing.and_then(|e| repo.find_tree(e.id()).ok());
        let sub_oid = insert_entry_in_tree(repo, sub_base.as_ref(), &segments[1..], oid, filemode)?;
        builder.insert(segments[0], sub_oid, 0o040000)?;
    }
    builder.write()
}

/// Remove the entry at `segments` from `base_tree`, rebuilding only the touched subtrees
/// in memory and pruning subtrees that become empty. Returns the new tree oid, or `None`
/// if the tree itself ended up empty (the caller then removes the parent entry).
/// Errors if the path does not exist.
fn remove_path_in_tree(
    repo: &Repository,
    base_tree: &git2::Tree,
    segments: &[&str],
) -> Result<Option<git2::Oid>, git2::Error> {
    let mut builder = repo.treebuilder(Some(base_tree))?;
    if segments.len() == 1 {
        if base_tree.get_name(segments[0]).is_none() {
            return Err(git2::Error::from_str(&format!(
                "path component '{}' not found",
                segments[0]
            )));
        }
        builder.remove(segments[0])?;
    } else {
        let entry = base_tree.get_name(segments[0]).ok_or_else(|| {
            git2::Error::from_str(&format!("path component '{}' not found", segments[0]))
        })?;
        let sub = repo
            .find_tree(entry.id())
            .map_err(|_| git2::Error::from_str(&format!("'{}' is not a directory", segments[0])))?;
        match remove_path_in_tree(repo, &sub, &segments[1..])? {
            Some(sub_oid) => {
                builder.insert(segments[0], sub_oid, 0o040000)?;
            }
            None => {
                builder.remove(segments[0])?;
            }
        }
    }
    let oid = builder.write()?;
    if repo.find_tree(oid)?.len() == 0 {
        Ok(None)
    } else {
        Ok(Some(oid))
    }
}

/// Create a commit object for `tree_oid` with the given parents, **without** moving any
/// ref (the caller updates refs explicitly). Lock-free; returns the new commit oid.
fn commit_tree(
    repo: &Repository,
    tree_oid: git2::Oid,
    message: &str,
    author: &str,
    parents: &[&git2::Commit],
) -> Result<git2::Oid, git2::Error> {
    let tree = repo.find_tree(tree_oid)?;
    let sig = signature(author)?;
    repo.commit(None, &sig, &sig, message, &tree, parents)
}

/// Collect conflicting `wiki/...` paths from a merge index.
fn conflict_paths(index: &mut git2::Index) -> Vec<String> {
    index
        .conflicts()
        .map(|cs| {
            cs.filter_map(|c| c.ok())
                .filter_map(|c| {
                    c.our
                        .or(c.their)
                        .or(c.ancestor)
                        .and_then(|e| String::from_utf8(e.path).ok())
                })
                .collect()
        })
        .unwrap_or_default()
}

fn rename_master_to_main(repo_path: &Path) {
    // Use git CLI to avoid libgit2 borrow issues
    let has_master = Command::new("git")
        .args(["branch", "--list", "master"])
        .current_dir(repo_path)
        .output()
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false);

    let has_main = Command::new("git")
        .args(["branch", "--list", "main"])
        .current_dir(repo_path)
        .output()
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false);

    if has_master && !has_main {
        Command::new("git")
            .args(["branch", "-m", "master", "main"])
            .current_dir(repo_path)
            .output()
            .ok();
    }
}

/// Manages multiple WikiRepo instances, one per workspace.
pub struct WikiRepoManager {
    data_dir: PathBuf,
    repos: RwLock<HashMap<String, Arc<WikiRepo>>>,
}

impl WikiRepoManager {
    pub fn new(data_dir: &str) -> Self {
        Self {
            data_dir: PathBuf::from(data_dir),
            repos: RwLock::new(HashMap::new()),
        }
    }

    /// Get or create a WikiRepo for a workspace.
    pub fn get(&self, workspace_slug: &str) -> Result<Arc<WikiRepo>, git2::Error> {
        // Check cache
        {
            let repos = self.repos.read().unwrap();
            if let Some(repo) = repos.get(workspace_slug) {
                return Ok(Arc::clone(repo));
            }
        }

        // Create new repo
        let repo = Arc::new(WikiRepo::open_or_init(
            &self.data_dir.join(workspace_slug).to_string_lossy(),
        )?);

        let mut repos = self.repos.write().unwrap();
        repos.insert(workspace_slug.to_string(), Arc::clone(&repo));
        Ok(repo)
    }
}

impl WikiRepo {
    pub fn open_or_init(data_dir: &str) -> Result<Self, git2::Error> {
        let path = PathBuf::from(data_dir).join("repo");
        if path.exists() {
            Repository::open(&path)?;
        } else {
            let repo = Repository::init(&path)?;
            fs::create_dir_all(path.join("wiki")).ok();
            fs::create_dir_all(path.join("sources")).ok();
            fs::create_dir_all(path.join("entities")).ok();
            fs::create_dir_all(path.join("concepts")).ok();

            fs::write(path.join("wiki/.gitkeep"), "").ok();
            fs::write(path.join("sources/.gitkeep"), "").ok();
            fs::write(path.join("entities/.gitkeep"), "").ok();
            fs::write(path.join("concepts/.gitkeep"), "").ok();

            let sig = Signature::now("cowiki", "cowiki@local")?;
            let mut index = repo.index()?;
            index.add_path(Path::new("wiki/.gitkeep"))?;
            index.add_path(Path::new("sources/.gitkeep"))?;
            index.add_path(Path::new("entities/.gitkeep"))?;
            index.add_path(Path::new("concepts/.gitkeep"))?;
            index.write()?;
            let tree_id = index.write_tree()?;
            let tree = repo.find_tree(tree_id)?;
            repo.commit(Some("HEAD"), &sig, &sig, "init: empty wiki", &tree, &[])?;
        }

        // Ensure main branch exists (rename master → main if needed)
        rename_master_to_main(&path);

        Ok(Self {
            path,
            write_locks: RwLock::new(HashMap::new()),
        })
    }

    fn repo(&self) -> Result<Repository, git2::Error> {
        Repository::open(&self.path)
    }

    /// Get or create a per-branch write lock to serialize git read-modify-write operations.
    fn branch_lock(&self, branch: &str) -> Arc<RwLock<()>> {
        let map = self.write_locks.read().unwrap();
        if let Some(lock) = map.get(branch) {
            return Arc::clone(lock);
        }
        drop(map);
        let mut map = self.write_locks.write().unwrap();
        map.entry(branch.to_string())
            .or_insert_with(|| Arc::new(RwLock::new(())))
            .clone()
    }

    pub fn ensure_user_branch(&self, user_id: &str) -> Result<String, git2::Error> {
        let branch_name = format!("user/{user_id}");
        self.ensure_branch_exists(&branch_name)
    }

    /// Ensure a branch exists, creating it from main if needed.
    pub fn ensure_branch_exists(&self, branch_name: &str) -> Result<String, git2::Error> {
        let repo = self.repo()?;

        if repo.find_branch(branch_name, BranchType::Local).is_ok() {
            return Ok(branch_name.to_string());
        }

        let main = repo
            .find_branch("main", BranchType::Local)
            .or_else(|_| repo.find_branch("master", BranchType::Local))?;
        let commit = main.get().peel_to_commit()?;
        repo.branch(branch_name, &commit, false)?;
        Ok(branch_name.to_string())
    }

    /// Apply one edit to `branch` as a single in-progress working commit.
    ///
    /// The branch carries exactly one commit on top of `merge-base(branch, main)`: each
    /// write rebuilds the tree from the current tip and re-commits with that merge-base
    /// as the parent, so repeated edits **amend** one commit instead of piling up
    /// autosave commits. Pure in-memory — no working directory, no shared on-disk index.
    pub fn write_file(
        &self,
        branch: &str,
        file_path: &str,
        content: &[u8],
        message: &str,
        author: &str,
    ) -> Result<(), git2::Error> {
        let lock = self.branch_lock(branch);
        let _guard = lock.write().unwrap();
        let repo = self.repo()?;

        let main_oid = repo
            .find_branch("main", BranchType::Local)?
            .get()
            .peel_to_commit()?
            .id();
        let tip = repo
            .find_branch(branch, BranchType::Local)?
            .get()
            .peel_to_commit()?;

        // Parent = merge-base(branch, main): keeps the branch to one working commit on
        // top of main. On a freshly forked branch (tip == main) this is the first commit.
        let base_oid = repo
            .merge_base(tip.id(), main_oid)
            .unwrap_or_else(|_| tip.id());
        let base = repo.find_commit(base_oid)?;

        let blob = repo.blob(content)?;
        let segments: Vec<&str> = file_path.split('/').collect();
        let tree_oid = set_path_in_tree(&repo, Some(&tip.tree()?), &segments, blob)?;
        let new_oid = commit_tree(&repo, tree_oid, message, author, &[&base])?;
        repo.reference(&format!("refs/heads/{branch}"), new_oid, true, message)?;
        Ok(())
    }

    /// Delete a file or an entire directory at `path` on `branch`, folded into the
    /// branch's single working commit (same amend semantics as `write_file`). Deletions
    /// flow through submit → snapshot → merge like any other change: the 3-way merge
    /// propagates the removal, and a main-side edit to a deleted page conflicts.
    pub fn delete_path(
        &self,
        branch: &str,
        path: &str,
        message: &str,
        author: &str,
    ) -> Result<(), git2::Error> {
        let lock = self.branch_lock(branch);
        let _guard = lock.write().unwrap();
        let repo = self.repo()?;

        let main_oid = repo
            .find_branch("main", BranchType::Local)?
            .get()
            .peel_to_commit()?
            .id();
        let tip = repo
            .find_branch(branch, BranchType::Local)?
            .get()
            .peel_to_commit()?;
        let base_oid = repo
            .merge_base(tip.id(), main_oid)
            .unwrap_or_else(|_| tip.id());
        let base = repo.find_commit(base_oid)?;

        let segments: Vec<&str> = path.split('/').collect();
        let tree_oid = remove_path_in_tree(&repo, &tip.tree()?, &segments)?
            .unwrap_or_else(|| tip.tree().map(|t| t.id()).unwrap_or_else(|_| base_oid));
        let new_oid = commit_tree(&repo, tree_oid, message, author, &[&base])?;
        repo.reference(&format!("refs/heads/{branch}"), new_oid, true, message)?;
        Ok(())
    }

    /// Rename/move a file or an entire directory from `from` to `to` on `branch`, folded
    /// into the branch's single working commit. The object (blob or whole subtree) is
    /// re-inserted at the new path, then removed from the old one — content unchanged.
    pub fn rename_path(
        &self,
        branch: &str,
        from: &str,
        to: &str,
        message: &str,
        author: &str,
    ) -> Result<(), git2::Error> {
        let lock = self.branch_lock(branch);
        let _guard = lock.write().unwrap();
        let repo = self.repo()?;

        let main_oid = repo
            .find_branch("main", BranchType::Local)?
            .get()
            .peel_to_commit()?
            .id();
        let tip = repo
            .find_branch(branch, BranchType::Local)?
            .get()
            .peel_to_commit()?;
        let base_oid = repo
            .merge_base(tip.id(), main_oid)
            .unwrap_or_else(|_| tip.id());
        let base = repo.find_commit(base_oid)?;
        let tip_tree = tip.tree()?;

        let entry = tip_tree
            .get_path(Path::new(from))
            .map_err(|_| git2::Error::from_str(&format!("'{from}' not found")))?;
        let (oid, mode) = match entry.kind() {
            Some(git2::ObjectType::Blob) => (entry.id(), 0o100644),
            Some(git2::ObjectType::Tree) => (entry.id(), 0o040000),
            _ => return Err(git2::Error::from_str(&format!("'{from}' is not movable"))),
        };
        if tip_tree.get_path(Path::new(to)).is_ok() {
            return Err(git2::Error::from_str(&format!("'{to}' already exists")));
        }

        let to_segments: Vec<&str> = to.split('/').collect();
        let with_new = insert_entry_in_tree(&repo, Some(&tip_tree), &to_segments, oid, mode)?;
        let from_segments: Vec<&str> = from.split('/').collect();
        let with_new_tree = repo.find_tree(with_new)?;
        let tree_oid =
            remove_path_in_tree(&repo, &with_new_tree, &from_segments)?.unwrap_or(with_new);
        let new_oid = commit_tree(&repo, tree_oid, message, author, &[&base])?;
        repo.reference(&format!("refs/heads/{branch}"), new_oid, true, message)?;
        Ok(())
    }

    pub fn read_file(&self, branch: &str, file_path: &str) -> Result<Option<Vec<u8>>, git2::Error> {
        let repo = self.repo()?;
        let branch_ref = repo.find_branch(branch, BranchType::Local)?;
        let commit = branch_ref.get().peel_to_commit()?;
        let tree = commit.tree()?;
        match tree.get_path(Path::new(file_path)) {
            Ok(entry) => {
                let blob = repo.find_blob(entry.id())?;
                Ok(Some(blob.content().to_vec()))
            }
            Err(_) => Ok(None),
        }
    }

    /// Author name + unix timestamp of the most recent commit on `branch` that
    /// changed `file_path` — i.e. who last edited the page and when.
    pub fn last_commit_for(
        &self,
        branch: &str,
        file_path: &str,
    ) -> Result<Option<(String, i64)>, git2::Error> {
        let repo = self.repo()?;
        let head = repo
            .find_branch(branch, BranchType::Local)?
            .get()
            .peel_to_commit()?;
        let mut revwalk = repo.revwalk()?;
        revwalk.push(head.id())?;
        revwalk.set_sorting(git2::Sort::TIME)?;
        let path = Path::new(file_path);
        for oid in revwalk {
            let commit = repo.find_commit(oid?)?;
            let here = commit.tree()?.get_path(path).map(|e| e.id()).ok();
            let parent = commit
                .parent(0)
                .ok()
                .and_then(|p| p.tree().ok())
                .and_then(|t| t.get_path(path).map(|e| e.id()).ok());
            let changed = match (here, parent) {
                (Some(a), Some(b)) => a != b,
                (Some(_), None) => true,
                _ => false,
            };
            if changed {
                let name = commit.author().name().unwrap_or("Unknown").to_string();
                return Ok(Some((name, commit.time().seconds())));
            }
        }
        Ok(None)
    }

    pub fn list_files(&self, branch: &str, dir: &str) -> Result<Vec<String>, git2::Error> {
        let repo = self.repo()?;
        let branch_ref = repo.find_branch(branch, BranchType::Local)?;
        let commit = branch_ref.get().peel_to_commit()?;
        let tree = commit.tree()?;

        let subtree = if dir.is_empty() {
            tree
        } else {
            match tree.get_path(Path::new(dir)) {
                Ok(entry) => repo.find_tree(entry.id())?,
                Err(_) => return Ok(Vec::new()),
            }
        };

        let mut files = Vec::new();
        for entry in subtree.iter() {
            if let Some(name) = entry.name() {
                if name.ends_with(".md") {
                    let full = if dir.is_empty() {
                        name.to_string()
                    } else {
                        format!("{dir}/{name}")
                    };
                    files.push(full);
                }
            }
        }
        Ok(files)
    }

    /// List all files recursively under a directory, returning full paths.
    pub fn list_files_recursive(
        &self,
        branch: &str,
        dir: &str,
    ) -> Result<Vec<String>, git2::Error> {
        let repo = self.repo()?;
        let branch_ref = repo.find_branch(branch, BranchType::Local)?;
        let commit = branch_ref.get().peel_to_commit()?;
        let tree = commit.tree()?;

        let subtree = if dir.is_empty() {
            tree
        } else {
            match tree.get_path(Path::new(dir)) {
                Ok(entry) => repo.find_tree(entry.id())?,
                Err(_) => return Ok(Vec::new()),
            }
        };

        let mut files = Vec::new();
        self.walk_tree(&repo, &subtree, dir, &mut files)?;
        Ok(files)
    }

    fn walk_tree(
        &self,
        repo: &Repository,
        tree: &git2::Tree,
        prefix: &str,
        files: &mut Vec<String>,
    ) -> Result<(), git2::Error> {
        for entry in tree.iter() {
            let name = match entry.name() {
                Some(n) => n,
                None => continue,
            };
            // Skip hidden files
            if name.starts_with('.') {
                continue;
            }

            let full_path = if prefix.is_empty() {
                name.to_string()
            } else {
                format!("{prefix}/{name}")
            };

            match entry.kind() {
                Some(git2::ObjectType::Tree) => {
                    let subtree = repo.find_tree(entry.id())?;
                    self.walk_tree(repo, &subtree, &full_path, files)?;
                }
                Some(git2::ObjectType::Blob) if name.ends_with(".md") => {
                    files.push(full_path);
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// List directory paths under `dir` (full paths, e.g. `wiki/research`) that
    /// contain a `.gitkeep` marker, excluding `dir` itself. Git cannot track an
    /// empty directory, so an empty folder is anchored by a `.gitkeep`; this lets
    /// the page tree surface those otherwise-invisible folders.
    pub fn list_marker_dirs(&self, branch: &str, dir: &str) -> Result<Vec<String>, git2::Error> {
        let repo = self.repo()?;
        let branch_ref = repo.find_branch(branch, BranchType::Local)?;
        let commit = branch_ref.get().peel_to_commit()?;
        let tree = commit.tree()?;

        let subtree = if dir.is_empty() {
            tree
        } else {
            match tree.get_path(Path::new(dir)) {
                Ok(entry) => repo.find_tree(entry.id())?,
                Err(_) => return Ok(Vec::new()),
            }
        };

        let mut dirs = Vec::new();
        self.walk_marker_dirs(&repo, &subtree, dir, &mut dirs)?;
        dirs.retain(|d| d != dir);
        Ok(dirs)
    }

    fn walk_marker_dirs(
        &self,
        repo: &Repository,
        tree: &git2::Tree,
        prefix: &str,
        dirs: &mut Vec<String>,
    ) -> Result<(), git2::Error> {
        let mut has_marker = false;
        for entry in tree.iter() {
            let name = match entry.name() {
                Some(n) => n,
                None => continue,
            };
            match entry.kind() {
                Some(git2::ObjectType::Tree) => {
                    let full_path = if prefix.is_empty() {
                        name.to_string()
                    } else {
                        format!("{prefix}/{name}")
                    };
                    let subtree = repo.find_tree(entry.id())?;
                    self.walk_marker_dirs(repo, &subtree, &full_path, dirs)?;
                }
                Some(git2::ObjectType::Blob) if name == ".gitkeep" => has_marker = true,
                _ => {}
            }
        }
        if has_marker && !prefix.is_empty() {
            dirs.push(prefix.to_string());
        }
        Ok(())
    }

    pub fn diff_files(&self, branch: &str, paths: &[String]) -> Result<Vec<FileDiff>, git2::Error> {
        let mut diffs = Vec::new();
        for p in paths {
            let file_path = format!("{p}.md");
            let main_content = self.read_file("main", &file_path)?;
            let branch_content = self.read_file(branch, &file_path)?;
            let old_str = main_content
                .as_ref()
                .map(|b| String::from_utf8_lossy(b).into_owned());
            let new_str = branch_content
                .as_ref()
                .map(|b| String::from_utf8_lossy(b).into_owned());
            let (hunks, additions, deletions) = compute_line_diff(
                old_str.as_deref().unwrap_or(""),
                new_str.as_deref().unwrap_or(""),
            );
            diffs.push(FileDiff {
                path: file_path,
                old_content: old_str,
                new_content: new_str,
                hunks,
                additions,
                deletions,
            });
        }
        Ok(diffs)
    }

    /// Bring `branch` up to date with `main` (the user's "sync" / rebase button, and the
    /// mandatory pre-submit step). Folds `main`'s changes into the branch as a single
    /// commit on top of `main`; on conflict the branch is left **untouched** so the user
    /// can resolve. Single-lock, lock-free body — no nested git locking, no deadlock.
    pub fn rebase_onto_main(&self, branch: &str) -> Result<RebaseOutcome, git2::Error> {
        if branch == "main" {
            return Ok(RebaseOutcome::UpToDate);
        }
        let lock = self.branch_lock(branch);
        let _guard = lock.write().unwrap();
        let repo = self.repo()?;

        let main_commit = repo
            .find_branch("main", BranchType::Local)?
            .get()
            .peel_to_commit()?;
        let branch_commit = repo
            .find_branch(branch, BranchType::Local)?
            .get()
            .peel_to_commit()?;

        if main_commit.id() == branch_commit.id()
            || repo.graph_descendant_of(branch_commit.id(), main_commit.id())?
        {
            return Ok(RebaseOutcome::UpToDate);
        }

        // ours = branch, theirs = main → pull main's changes into the branch.
        let mut idx = repo.merge_commits(&branch_commit, &main_commit, None)?;
        if idx.has_conflicts() {
            return Ok(RebaseOutcome::Conflict(conflict_paths(&mut idx)));
        }
        let tree_oid = idx.write_tree_to(&repo)?;
        // Collapse to a single commit on top of main: the branch becomes
        // "main + this user's net changes".
        let new_oid = commit_tree(
            &repo,
            tree_oid,
            "sync: rebase onto main",
            "cowiki",
            &[&main_commit],
        )?;
        repo.reference(&format!("refs/heads/{branch}"), new_oid, true, "sync")?;
        Ok(RebaseOutcome::Updated)
    }

    /// Freeze the current state of `user_branch` as submission `id`'s reviewable snapshot:
    /// `pr/{id}` (the PR tip that gets reviewed and merged) plus a frozen base ref, both at
    /// commit `S`. The caller must rebase `user_branch` onto `main` first (submit does), so
    /// `S` is already main-based. The live `user_branch` is not touched.
    pub fn create_pr_snapshot(&self, user_branch: &str, id: &str) -> Result<(), git2::Error> {
        let pr = format!("pr/{id}");
        let lock = self.branch_lock(&pr);
        let _guard = lock.write().unwrap();
        let repo = self.repo()?;
        let tip = repo
            .find_branch(user_branch, BranchType::Local)?
            .get()
            .peel_to_commit()?
            .id();
        repo.reference(
            &format!("refs/heads/{pr}"),
            tip,
            true,
            "submit: pr snapshot",
        )?;
        repo.reference(
            &format!("refs/cowiki/base/{id}"),
            tip,
            true,
            "submit: frozen base",
        )?;
        Ok(())
    }

    /// Apply a review-requested change to submission `id` as a single commit `R` on top of
    /// the frozen snapshot `S`, **amended** across rounds (`S` never moves, so `S`→`R` is
    /// always the clean "what review changed" diff).
    pub fn write_review_fix(
        &self,
        id: &str,
        file_path: &str,
        content: &[u8],
        message: &str,
        author: &str,
    ) -> Result<(), git2::Error> {
        let pr = format!("pr/{id}");
        let lock = self.branch_lock(&pr);
        let _guard = lock.write().unwrap();
        let repo = self.repo()?;

        let base_oid = repo.refname_to_id(&format!("refs/cowiki/base/{id}"))?;
        let base = repo.find_commit(base_oid)?;
        let tip = repo
            .find_branch(&pr, BranchType::Local)?
            .get()
            .peel_to_commit()?;

        let blob = repo.blob(content)?;
        let segments: Vec<&str> = file_path.split('/').collect();
        let tree_oid = set_path_in_tree(&repo, Some(&tip.tree()?), &segments, blob)?;
        // Parent is always the frozen base `S`, so this stays one amended commit on top of S.
        let r_oid = commit_tree(&repo, tree_oid, message, author, &[&base])?;
        repo.reference(&format!("refs/heads/{pr}"), r_oid, true, message)?;
        Ok(())
    }

    /// Merge submission `id`'s `pr/{id}` into `main`: a 3-way against the *current* `main`,
    /// squashed into one commit and fast-forwarded. `main` stays **linear** — the commit
    /// has a single parent (`main`), no two-parent merge node. Authored by the submitter.
    /// Conflict with `main` → nothing is written.
    pub fn merge_pr(
        &self,
        id: &str,
        author: &str,
        message: &str,
    ) -> Result<MergeOutcome, git2::Error> {
        let lock = self.branch_lock("main");
        let _guard = lock.write().unwrap();
        // Also hold the PR-branch lock so a concurrent review-fix can't move the tip
        // mid-merge. Lock order is always main → pr (no caller takes pr → main).
        let pr_lock = self.branch_lock(&format!("pr/{id}"));
        let _pr_guard = pr_lock.write().unwrap();
        let repo = self.repo()?;

        let main_commit = repo
            .find_branch("main", BranchType::Local)?
            .get()
            .peel_to_commit()?;
        let pr = format!("pr/{id}");
        let pr_commit = repo
            .find_branch(&pr, BranchType::Local)?
            .get()
            .peel_to_commit()?;

        // ours = main, theirs = pr.
        let mut idx = repo.merge_commits(&main_commit, &pr_commit, None)?;
        if idx.has_conflicts() {
            return Ok(MergeOutcome::Conflict(conflict_paths(&mut idx)));
        }
        let tree_oid = idx.write_tree_to(&repo)?;
        // Single parent = main → linear squash commit.
        let m_oid = commit_tree(&repo, tree_oid, message, author, &[&main_commit])?;
        repo.reference("refs/heads/main", m_oid, true, message)?;
        Ok(MergeOutcome::Merged)
    }

    /// Delete a submission's git refs (`pr/{id}` and its frozen base) — called after a
    /// successful merge or a reject/abandon. Best-effort; missing refs are ignored.
    pub fn cleanup_submission(&self, id: &str) {
        let Ok(repo) = self.repo() else { return };
        for name in [
            format!("refs/heads/pr/{id}"),
            format!("refs/cowiki/base/{id}"),
        ] {
            if let Ok(mut r) = repo.find_reference(&name) {
                let _ = r.delete();
            }
        }
    }

    /// Whole-tree diff of a ref (e.g. `pr/{id}` or a `user/{id}` branch) against `main`,
    /// as per-file `FileDiff`s for the changed `wiki/*.md` pages. Drives the review UI.
    pub fn diff_ref_against_main(&self, ref_name: &str) -> Result<Vec<FileDiff>, git2::Error> {
        let repo = self.repo()?;
        let main_tree = repo
            .find_branch("main", BranchType::Local)?
            .get()
            .peel_to_commit()?
            .tree()?;
        let ref_tree = repo.revparse_single(ref_name)?.peel_to_commit()?.tree()?;

        let diff = repo.diff_tree_to_tree(Some(&main_tree), Some(&ref_tree), None)?;
        let mut paths: Vec<String> = Vec::new();
        diff.foreach(
            &mut |delta, _| {
                if let Some(p) = delta
                    .new_file()
                    .path()
                    .or_else(|| delta.old_file().path())
                    .and_then(|p| p.to_str())
                {
                    if p.starts_with("wiki/") && p.ends_with(".md") {
                        paths.push(p.to_string());
                    }
                }
                true
            },
            None,
            None,
            None,
        )?;
        paths.sort();
        paths.dedup();

        let mut out = Vec::new();
        for path in paths {
            let old = self
                .read_file("main", &path)?
                .map(|b| String::from_utf8_lossy(&b).into_owned());
            let new = self
                .read_file(ref_name, &path)?
                .map(|b| String::from_utf8_lossy(&b).into_owned());
            let (hunks, additions, deletions) =
                compute_line_diff(old.as_deref().unwrap_or(""), new.as_deref().unwrap_or(""));
            out.push(FileDiff {
                path,
                old_content: old,
                new_content: new,
                hunks,
                additions,
                deletions,
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::{MergeOutcome, RebaseOutcome, WikiRepo};
    use git2::{BranchType, Repository};
    use std::sync::{mpsc, Arc};
    use std::time::Duration;

    fn temp_repo(tag: &str) -> (Arc<WikiRepo>, std::path::PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("cowiki-git-test-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&dir);
        let repo = Arc::new(WikiRepo::open_or_init(dir.to_str().unwrap()).unwrap());
        (repo, dir)
    }

    fn read(repo: &WikiRepo, branch: &str, path: &str) -> Option<String> {
        repo.read_file(branch, path)
            .unwrap()
            .map(|b| String::from_utf8_lossy(&b).into_owned())
    }

    fn git(dir: &std::path::Path) -> Repository {
        Repository::open(dir.join("repo")).unwrap()
    }
    fn tip(g: &Repository, branch: &str) -> git2::Oid {
        g.find_branch(branch, BranchType::Local)
            .unwrap()
            .get()
            .peel_to_commit()
            .unwrap()
            .id()
    }

    /// Every edit amends ONE working commit: the branch stays exactly one commit above
    /// main (its single parent is main's tip), accumulating all pages of the task.
    #[test]
    fn edits_keep_a_single_working_commit() {
        let (repo, dir) = temp_repo("single-commit");
        repo.ensure_branch_exists("user/a").unwrap();
        for i in 0..4 {
            repo.write_file(
                "user/a",
                "wiki/p.md",
                format!("v{i}\n").as_bytes(),
                "edit",
                "a",
            )
            .unwrap();
        }
        repo.write_file("user/a", "wiki/q.md", b"q\n", "edit", "a")
            .unwrap();

        let g = git(&dir);
        let main_tip = tip(&g, "main");
        let commit = g
            .find_branch("user/a", BranchType::Local)
            .unwrap()
            .get()
            .peel_to_commit()
            .unwrap();
        assert_eq!(commit.parent_count(), 1, "single working commit");
        assert_eq!(commit.parent_id(0).unwrap(), main_tip, "parented on main");
        assert_eq!(read(&repo, "user/a", "wiki/p.md").as_deref(), Some("v3\n"));
        assert_eq!(read(&repo, "user/a", "wiki/q.md").as_deref(), Some("q\n"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Snapshot freezes S; review fixes are a single commit R on top of S, amended across
    /// rounds — S (the frozen base ref) never moves.
    #[test]
    fn snapshot_then_review_fix_amends_on_frozen_base() {
        let (repo, dir) = temp_repo("review-fix");
        repo.ensure_branch_exists("user/b").unwrap();
        repo.write_file("user/b", "wiki/p.md", b"original\n", "edit", "b")
            .unwrap();
        repo.create_pr_snapshot("user/b", "sub1").unwrap();

        let g = git(&dir);
        let base = g.refname_to_id("refs/cowiki/base/sub1").unwrap();
        assert_eq!(
            tip(&g, "pr/sub1"),
            base,
            "pr tip == frozen base before any fix"
        );

        repo.write_review_fix("sub1", "wiki/p.md", b"fixed\n", "fix", "b")
            .unwrap();
        let r1 = g.find_commit(tip(&g, "pr/sub1")).unwrap();
        assert_eq!(r1.parent_id(0).unwrap(), base, "R is parented on frozen S");
        assert_eq!(
            g.refname_to_id("refs/cowiki/base/sub1").unwrap(),
            base,
            "base ref unchanged"
        );

        repo.write_review_fix("sub1", "wiki/p.md", b"fixed2\n", "fix2", "b")
            .unwrap();
        let r2 = g.find_commit(tip(&g, "pr/sub1")).unwrap();
        assert_eq!(
            r2.parent_id(0).unwrap(),
            base,
            "still one commit on frozen S"
        );
        assert_eq!(
            read(&repo, "pr/sub1", "wiki/p.md").as_deref(),
            Some("fixed2\n")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// merge_pr is linear (single parent = main), authored by the submitter, lands content.
    #[test]
    fn merge_pr_is_linear_and_lands_content() {
        let (repo, dir) = temp_repo("merge-linear");
        repo.ensure_branch_exists("user/c").unwrap();
        repo.write_file("user/c", "wiki/p.md", b"hello\n", "edit", "carol")
            .unwrap();
        repo.create_pr_snapshot("user/c", "subc").unwrap();

        let g = git(&dir);
        let main_before = tip(&g, "main");
        assert_eq!(
            repo.merge_pr("subc", "carol", "approve: p").unwrap(),
            MergeOutcome::Merged
        );
        let m = g
            .find_branch("main", BranchType::Local)
            .unwrap()
            .get()
            .peel_to_commit()
            .unwrap();
        assert_eq!(m.parent_count(), 1, "linear: single parent, no merge node");
        assert_eq!(m.parent_id(0).unwrap(), main_before);
        assert_eq!(m.author().name(), Some("carol"), "authored by submitter");
        assert_eq!(read(&repo, "main", "wiki/p.md").as_deref(), Some("hello\n"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Post-submit edits on the live user branch must NOT change what gets merged — the
    /// snapshot is frozen. (This is the core bug the redesign fixes.)
    #[test]
    fn post_submit_edits_do_not_leak_into_merge() {
        let (repo, dir) = temp_repo("no-leak");
        repo.ensure_branch_exists("user/j").unwrap();
        repo.write_file("user/j", "wiki/p.md", b"reviewed\n", "edit", "j")
            .unwrap();
        repo.create_pr_snapshot("user/j", "subj").unwrap();
        // keep editing the live branch AFTER submit
        repo.write_file("user/j", "wiki/p.md", b"sneaky\n", "edit", "j")
            .unwrap();
        assert_eq!(
            repo.merge_pr("subj", "j", "approve").unwrap(),
            MergeOutcome::Merged
        );
        assert_eq!(
            read(&repo, "main", "wiki/p.md").as_deref(),
            Some("reviewed\n"),
            "merge lands the reviewed snapshot, not the post-submit edit"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// merge_pr detects a real conflict (main changed the same page after snapshot) and
    /// writes nothing.
    #[test]
    fn merge_pr_detects_conflict_and_writes_nothing() {
        let (repo, dir) = temp_repo("merge-conflict");
        // seed main with p = base
        repo.ensure_branch_exists("user/d").unwrap();
        repo.write_file("user/d", "wiki/p.md", b"base\n", "edit", "d")
            .unwrap();
        repo.create_pr_snapshot("user/d", "subd").unwrap();
        repo.merge_pr("subd", "d", "seed").unwrap();

        // user E snapshots p = mine (based on p = base)
        repo.ensure_branch_exists("user/e").unwrap();
        repo.write_file("user/e", "wiki/p.md", b"mine\n", "edit", "e")
            .unwrap();
        repo.create_pr_snapshot("user/e", "sube").unwrap();

        // meanwhile main advances p = theirs via user F
        repo.ensure_branch_exists("user/f").unwrap();
        repo.write_file("user/f", "wiki/p.md", b"theirs\n", "edit", "f")
            .unwrap();
        repo.create_pr_snapshot("user/f", "subf").unwrap();
        repo.merge_pr("subf", "f", "land theirs").unwrap();

        let main_before = tip(&git(&dir), "main");
        match repo.merge_pr("sube", "e", "land mine").unwrap() {
            MergeOutcome::Conflict(paths) => {
                assert!(
                    paths.iter().any(|p| p.contains("p.md")),
                    "conflict names p.md"
                )
            }
            other => panic!("expected conflict, got {other:?}"),
        }
        assert_eq!(
            tip(&git(&dir), "main"),
            main_before,
            "main untouched on conflict"
        );
        assert_eq!(
            read(&repo, "main", "wiki/p.md").as_deref(),
            Some("theirs\n")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// rebase_onto_main: up-to-date, updated (main advanced a different page), conflict.
    #[test]
    fn rebase_onto_main_outcomes() {
        let (repo, dir) = temp_repo("rebase");
        repo.ensure_branch_exists("user/g").unwrap();
        assert_eq!(
            repo.rebase_onto_main("user/g").unwrap(),
            RebaseOutcome::UpToDate,
            "fresh branch == main"
        );

        // main advances a DIFFERENT page
        repo.ensure_branch_exists("user/h").unwrap();
        repo.write_file("user/h", "wiki/other.md", b"other\n", "edit", "h")
            .unwrap();
        repo.create_pr_snapshot("user/h", "subh").unwrap();
        repo.merge_pr("subh", "h", "land other").unwrap();

        repo.write_file("user/g", "wiki/p.md", b"mine\n", "edit", "g")
            .unwrap();
        assert_eq!(
            repo.rebase_onto_main("user/g").unwrap(),
            RebaseOutcome::Updated
        );
        assert_eq!(
            read(&repo, "user/g", "wiki/other.md").as_deref(),
            Some("other\n")
        );
        assert_eq!(
            read(&repo, "user/g", "wiki/p.md").as_deref(),
            Some("mine\n")
        );

        // conflict: main changes p; user/g already changed p differently
        repo.ensure_branch_exists("user/i").unwrap();
        repo.write_file("user/i", "wiki/p.md", b"theirs\n", "edit", "i")
            .unwrap();
        repo.create_pr_snapshot("user/i", "subi").unwrap();
        repo.merge_pr("subi", "i", "land theirs p").unwrap();
        match repo.rebase_onto_main("user/g").unwrap() {
            RebaseOutcome::Conflict(paths) => {
                assert!(paths.iter().any(|p| p.contains("p.md")))
            }
            other => panic!("expected conflict, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Regression for the old `merge_to_main` self-deadlock (it held the "main" lock then
    /// called `write_file("main", …)`, re-acquiring the same non-reentrant lock). The new
    /// merge never nests locks; run it on a worker thread with a timeout so a regression
    /// fails fast instead of hanging the suite.
    #[test]
    fn full_cycle_does_not_deadlock() {
        let (repo, dir) = temp_repo("deadlock");
        repo.ensure_branch_exists("user/x").unwrap();
        repo.write_file("user/x", "wiki/p.md", b"hi\n", "edit", "x")
            .unwrap();
        repo.create_pr_snapshot("user/x", "subx").unwrap();

        let (tx, rx) = mpsc::channel();
        let r2 = Arc::clone(&repo);
        std::thread::spawn(move || {
            let _ = tx.send(r2.merge_pr("subx", "x", "approve"));
        });
        match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(Ok(MergeOutcome::Merged)) => {}
            Ok(other) => panic!("unexpected merge result: {other:?}"),
            Err(_) => panic!("merge_pr deadlocked (timed out after 10s)"),
        }
        assert_eq!(read(&repo, "main", "wiki/p.md").as_deref(), Some("hi\n"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// cleanup_submission removes the pr/{id} branch and the frozen base ref.
    #[test]
    fn cleanup_removes_submission_refs() {
        let (repo, dir) = temp_repo("cleanup");
        repo.ensure_branch_exists("user/k").unwrap();
        repo.write_file("user/k", "wiki/p.md", b"x\n", "edit", "k")
            .unwrap();
        repo.create_pr_snapshot("user/k", "subk").unwrap();
        let g = git(&dir);
        assert!(g.find_branch("pr/subk", BranchType::Local).is_ok());
        repo.cleanup_submission("subk");
        assert!(g.find_branch("pr/subk", BranchType::Local).is_err());
        assert!(g.refname_to_id("refs/cowiki/base/subk").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// delete_path removes a page (and prunes emptied directories), keeps the branch at a
    /// single working commit, and the deletion survives snapshot + merge into main.
    #[test]
    fn delete_page_flows_through_merge() {
        let (repo, dir) = temp_repo("delete");
        // land a nested page on main first
        repo.ensure_branch_exists("user/d1").unwrap();
        repo.write_file("user/d1", "wiki/guides/a.md", b"a\n", "edit", "d")
            .unwrap();
        repo.write_file("user/d1", "wiki/keep.md", b"keep\n", "edit", "d")
            .unwrap();
        repo.create_pr_snapshot("user/d1", "s1").unwrap();
        repo.merge_pr("s1", "d", "land").unwrap();
        let _ = repo.rebase_onto_main("user/d1");

        // delete the nested page on the branch
        repo.delete_path("user/d1", "wiki/guides/a.md", "delete a", "d")
            .unwrap();
        assert!(read(&repo, "user/d1", "wiki/guides/a.md").is_none());
        assert_eq!(
            read(&repo, "user/d1", "wiki/keep.md").as_deref(),
            Some("keep\n")
        );
        // still a single working commit on top of main
        let g = git(&dir);
        let c = g
            .find_branch("user/d1", BranchType::Local)
            .unwrap()
            .get()
            .peel_to_commit()
            .unwrap();
        assert_eq!(c.parent_count(), 1);
        assert_eq!(c.parent_id(0).unwrap(), tip(&g, "main"));

        // deletion lands on main through snapshot + merge
        repo.create_pr_snapshot("user/d1", "s2").unwrap();
        assert_eq!(
            repo.merge_pr("s2", "d", "rm").unwrap(),
            MergeOutcome::Merged
        );
        assert!(read(&repo, "main", "wiki/guides/a.md").is_none());
        assert_eq!(
            read(&repo, "main", "wiki/keep.md").as_deref(),
            Some("keep\n")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// list_marker_dirs surfaces directories anchored only by a `.gitkeep` (empty
    /// folders), excludes the queried dir itself, and ignores `.md`-bearing dirs.
    #[test]
    fn list_marker_dirs_finds_empty_folders() {
        let (repo, dir) = temp_repo("marker-dirs");
        repo.ensure_branch_exists("user/m").unwrap();
        // An empty folder, anchored by a .gitkeep
        repo.write_file("user/m", "wiki/empty/.gitkeep", b"", "e", "m")
            .unwrap();
        // A nested empty folder
        repo.write_file("user/m", "wiki/parent/child/.gitkeep", b"", "e", "m")
            .unwrap();
        // A normal page (no marker)
        repo.write_file("user/m", "wiki/page.md", b"# p\n", "e", "m")
            .unwrap();

        let mut dirs = repo.list_marker_dirs("user/m", "wiki").unwrap();
        dirs.sort();
        assert_eq!(dirs, vec!["wiki/empty", "wiki/parent/child"]);
        // The queried dir itself (whose .gitkeep was seeded at init) is excluded.
        assert!(!dirs.contains(&"wiki".to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// delete_path on a directory removes the whole subtree.
    #[test]
    fn delete_folder_removes_subtree() {
        let (repo, dir) = temp_repo("delete-folder");
        repo.ensure_branch_exists("user/d2").unwrap();
        repo.write_file("user/d2", "wiki/proj/.gitkeep", b"", "e", "d")
            .unwrap();
        repo.write_file("user/d2", "wiki/proj/x.md", b"x\n", "e", "d")
            .unwrap();
        repo.write_file("user/d2", "wiki/other.md", b"o\n", "e", "d")
            .unwrap();
        repo.delete_path("user/d2", "wiki/proj", "rm folder", "d")
            .unwrap();
        assert!(read(&repo, "user/d2", "wiki/proj/.gitkeep").is_none());
        assert!(read(&repo, "user/d2", "wiki/proj/x.md").is_none());
        assert_eq!(
            read(&repo, "user/d2", "wiki/other.md").as_deref(),
            Some("o\n")
        );
        // deleting a missing path errors
        assert!(repo
            .delete_path("user/d2", "wiki/proj", "again", "d")
            .is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// rename_path moves a file and an entire folder subtree, refusing to overwrite.
    #[test]
    fn rename_file_and_folder() {
        let (repo, dir) = temp_repo("rename");
        repo.ensure_branch_exists("user/r").unwrap();
        repo.write_file("user/r", "wiki/old.md", b"body\n", "e", "r")
            .unwrap();
        repo.rename_path("user/r", "wiki/old.md", "wiki/new.md", "mv", "r")
            .unwrap();
        assert!(read(&repo, "user/r", "wiki/old.md").is_none());
        assert_eq!(
            read(&repo, "user/r", "wiki/new.md").as_deref(),
            Some("body\n")
        );

        // folder move keeps children
        repo.write_file("user/r", "wiki/dir/a.md", b"a\n", "e", "r")
            .unwrap();
        repo.write_file("user/r", "wiki/dir/sub/b.md", b"b\n", "e", "r")
            .unwrap();
        repo.rename_path("user/r", "wiki/dir", "wiki/moved", "mv dir", "r")
            .unwrap();
        assert!(read(&repo, "user/r", "wiki/dir/a.md").is_none());
        assert_eq!(
            read(&repo, "user/r", "wiki/moved/a.md").as_deref(),
            Some("a\n")
        );
        assert_eq!(
            read(&repo, "user/r", "wiki/moved/sub/b.md").as_deref(),
            Some("b\n")
        );

        // refuses to overwrite an existing target
        repo.write_file("user/r", "wiki/exists.md", b"x\n", "e", "r")
            .unwrap();
        assert!(repo
            .rename_path("user/r", "wiki/new.md", "wiki/exists.md", "mv", "r")
            .is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// diff_files accepts full repo paths (no hardcoded wiki/ prefix), so pages
    /// from entities/ and concepts/ are diffed against the correct files.
    #[test]
    fn diff_files_uses_full_repo_paths() {
        let (repo, dir) = temp_repo("diff-paths");
        repo.ensure_branch_exists("user/a").unwrap();

        // Write pages in different content directories
        repo.write_file("user/a", "wiki/home.md", b"wiki page\n", "edit", "a")
            .unwrap();
        repo.write_file(
            "user/a",
            "entities/people/alice.md",
            b"alice entity\n",
            "edit",
            "a",
        )
        .unwrap();
        repo.write_file(
            "user/a",
            "concepts/patterns/error-handling.md",
            b"error handling concept\n",
            "edit",
            "a",
        )
        .unwrap();

        // diff_files with full repo paths (no .md extension per design)
        let paths: Vec<String> = vec![
            "wiki/home".into(),
            "entities/people/alice".into(),
            "concepts/patterns/error-handling".into(),
        ];
        let diffs = repo.diff_files("user/a", &paths).unwrap();
        assert_eq!(diffs.len(), 3);

        // Each path in the diff should carry the full .md file path
        let diff_paths: Vec<&str> = diffs.iter().map(|d| d.path.as_str()).collect();
        assert!(diff_paths.contains(&"wiki/home.md"));
        assert!(diff_paths.contains(&"entities/people/alice.md"));
        assert!(diff_paths.contains(&"concepts/patterns/error-handling.md"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// list_pages with dir=entities only returns pages under entities/,
    /// not wiki/ or concepts/.
    #[test]
    fn list_pages_scoped_to_directory() {
        let (repo, dir) = temp_repo("list-scoped");
        repo.ensure_branch_exists("user/b").unwrap();

        repo.write_file("user/b", "wiki/home.md", b"wiki\n", "e", "b")
            .unwrap();
        repo.write_file("user/b", "entities/alice.md", b"alice\n", "e", "b")
            .unwrap();
        repo.write_file("user/b", "entities/people/bob.md", b"bob\n", "e", "b")
            .unwrap();

        // list_pages_recursive under entities/
        let entities_files =
            crate::wiki_fs::list_pages_recursive(&repo, "user/b", "entities").unwrap();
        assert_eq!(entities_files.len(), 2);
        assert!(entities_files.iter().any(|f| f == "entities/alice.md"));
        assert!(entities_files.iter().any(|f| f == "entities/people/bob.md"));

        // list_pages_recursive under wiki/
        let wiki_files = crate::wiki_fs::list_pages_recursive(&repo, "user/b", "wiki").unwrap();
        assert_eq!(wiki_files.len(), 1);
        assert!(wiki_files.iter().any(|f| f == "wiki/home.md"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// write_file + read_file work with multi-directory paths — no hardcoded
    /// assumptions about the content root.
    #[test]
    fn read_write_multi_directory() {
        let (repo, dir) = temp_repo("multi-dir-rw");
        repo.ensure_branch_exists("user/c").unwrap();

        let test_cases = vec![
            ("wiki/team-home.md", "team home content"),
            ("entities/people/alice.md", "alice content"),
            (
                "concepts/patterns/error-handling.md",
                "error handling content",
            ),
            ("wiki/research/ai-safety.md", "nested wiki page"),
            ("entities/orgs/acme.md", "org entity"),
        ];

        for (file_path, content) in &test_cases {
            repo.write_file("user/c", file_path, content.as_bytes(), "add", "c")
                .unwrap();
        }

        for (file_path, expected) in &test_cases {
            let got = repo
                .read_file("user/c", file_path)
                .unwrap()
                .map(|b| String::from_utf8_lossy(&b).into_owned());
            assert_eq!(
                got.as_deref(),
                Some(*expected),
                "read_file({file_path}) should match written content"
            );
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}
