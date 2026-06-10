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

/// Outcome of merging a user branch's pages into `main`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeOutcome {
    /// All requested pages merged cleanly; `main` advanced by one commit.
    Merged,
    /// One or more requested pages conflict with `main`. Nothing was written;
    /// carries the conflicting `wiki/<slug>.md` paths.
    Conflict(Vec<String>),
}

/// Outcome of bringing a user branch up to date with `main`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RebaseOutcome {
    /// Branch already contains all of `main`; nothing to do.
    UpToDate,
    /// `main` was merged into the branch cleanly; the branch advanced.
    Updated,
    /// Branch conflicts with `main`. The branch was left **untouched**;
    /// carries the conflicting paths for the UI to surface.
    Conflict(Vec<String>),
}

/// Build a git signature from a user display name. libgit2 rejects names containing
/// angle brackets (and control chars), so we strip those and fall back to a service
/// identity when nothing usable remains — a real user name must never 500 a write.
/// The email is a fixed placeholder (cowiki has no per-user git mailbox).
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
        // Refuse to drop a whole subtree by overwriting it with a file.
        if existing.as_ref().and_then(|e| e.kind()) == Some(git2::ObjectType::Tree) {
            return Err(git2::Error::from_str(&format!(
                "cannot write file over existing directory '{}'",
                segments[0]
            )));
        }
        builder.insert(segments[0], blob, 0o100644)?;
    } else {
        // Refuse to descend through a file as if it were a directory.
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

/// Commit `tree_oid` as the new tip of `branch` (`refs/heads/<branch>`), authored by
/// `author`. Lock-free: callers already holding the branch write lock use it directly.
fn commit_tree(
    repo: &Repository,
    branch: &str,
    tree_oid: git2::Oid,
    message: &str,
    author: &str,
    parents: &[&git2::Commit],
) -> Result<(), git2::Error> {
    let tree = repo.find_tree(tree_oid)?;
    let sig = signature(author)?;
    repo.commit(
        Some(&format!("refs/heads/{branch}")),
        &sig,
        &sig,
        message,
        &tree,
        parents,
    )?;
    Ok(())
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

            fs::write(path.join("wiki/.gitkeep"), "").ok();
            fs::write(path.join("sources/.gitkeep"), "").ok();

            let sig = Signature::now("cowiki", "cowiki@local")?;
            let mut index = repo.index()?;
            index.add_path(Path::new("wiki/.gitkeep"))?;
            index.add_path(Path::new("sources/.gitkeep"))?;
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

    /// Commit a single file onto `branch`. The new tree is built entirely in memory,
    /// so this never touches the working directory or the shared on-disk index —
    /// avoiding the cross-branch nested-path index race the old implementation had.
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
        let parent_commit = repo
            .find_branch(branch, BranchType::Local)?
            .get()
            .peel_to_commit()?;

        let blob_oid = repo.blob(content)?;
        let segments: Vec<&str> = file_path.split('/').collect();
        let tree_oid = set_path_in_tree(&repo, Some(&parent_commit.tree()?), &segments, blob_oid)?;
        commit_tree(&repo, branch, tree_oid, message, author, &[&parent_commit])
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

    pub fn diff_files(&self, branch: &str, slugs: &[String]) -> Result<Vec<FileDiff>, git2::Error> {
        let mut diffs = Vec::new();
        for slug in slugs {
            let path = format!("wiki/{slug}.md");
            let main_content = self.read_file("main", &path)?;
            let branch_content = self.read_file(branch, &path)?;
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
                path,
                old_content: old_str,
                new_content: new_str,
                hunks,
                additions,
                deletions,
            });
        }
        Ok(diffs)
    }

    /// Merge the given `wiki/<slug>.md` pages from `branch` into `main` with a real
    /// 3-way merge (merge-base vs main vs branch), recorded as a 2-parent merge commit
    /// authored by `author` (the original submitter). Only the requested pages are
    /// applied — the rest of `branch` (e.g. unsubmitted drafts) is ignored. If any
    /// requested page conflicts with `main`, nothing is written and the conflicting
    /// paths are returned via `Conflict`.
    ///
    /// The source-branch tip is read without holding the branch lock: git objects are
    /// immutable, so the worst a concurrent edit can do is make the merge see a
    /// slightly older tip — never a torn read or a bad write to `main`.
    pub fn merge_to_main(
        &self,
        branch: &str,
        file_paths: &[String],
        author: &str,
        message: &str,
    ) -> Result<MergeOutcome, git2::Error> {
        let lock = self.branch_lock("main");
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

        // In-memory 3-way merge of the two tips (libgit2 finds the merge base).
        let merged_index = repo.merge_commits(&main_commit, &branch_commit, None)?;

        // For each requested page, take the merged blob (stage 0). A requested page
        // with no stage-0 entry that still exists on the branch is a real conflict.
        // (Note: a branch-side *deletion* that conflicts with a main-side edit is not
        // distinguished here, but cowiki has no page-delete path and a clean rebase
        // always precedes merge, so it is unreachable today.)
        let mut updates: Vec<(String, git2::Oid)> = Vec::new();
        let mut conflicts: Vec<String> = Vec::new();
        for path in file_paths {
            match merged_index.get_path(Path::new(path), 0) {
                Some(entry) => updates.push((path.clone(), entry.id)),
                None => {
                    if self.read_file(branch, path)?.is_some() {
                        conflicts.push(path.clone());
                    }
                }
            }
        }

        if !conflicts.is_empty() {
            return Ok(MergeOutcome::Conflict(conflicts));
        }
        if updates.is_empty() {
            return Ok(MergeOutcome::Merged);
        }

        // Apply only the requested pages onto main's current tree, in memory.
        let mut tree_oid = main_commit.tree()?.id();
        for (path, blob) in &updates {
            let cur = repo.find_tree(tree_oid)?;
            let segments: Vec<&str> = path.split('/').collect();
            tree_oid = set_path_in_tree(&repo, Some(&cur), &segments, *blob)?;
        }

        // Commit as a 2-parent merge of main + the branch. The branch parent is what
        // makes `main` an ancestor of every contributing branch, so the merge base
        // advances and a later edit to the *same* page no longer re-diffs against a
        // stale base (which would otherwise surface as a bogus add/add conflict and
        // block all repeat edits). Only the requested pages are in the committed
        // tree, so unsubmitted drafts never reach main's content even though the
        // branch is reachable in history.
        commit_tree(
            &repo,
            "main",
            tree_oid,
            message,
            author,
            &[&main_commit, &branch_commit],
        )?;
        Ok(MergeOutcome::Merged)
    }

    /// Bring `branch` up to date with `main` by merging `main` into it (a "catch-up").
    /// We squash at merge-to-main time, so the branch's internal history need not stay
    /// linear — a merge-from-main is simpler and more robust than a true rebase/replay.
    ///
    /// - Clean → the branch advances by one merge commit (`Updated`).
    /// - Branch already contains `main` → `UpToDate`.
    /// - Conflicts → the branch is left **untouched** and the conflicting paths are
    ///   returned, so the caller/UI can ask the author to resolve them.
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
        let mut merged_index = repo.merge_commits(&branch_commit, &main_commit, None)?;
        if merged_index.has_conflicts() {
            let conflicts: Vec<String> = merged_index
                .conflicts()?
                .filter_map(|c| c.ok())
                .filter_map(|c| {
                    c.our
                        .or(c.their)
                        .or(c.ancestor)
                        .and_then(|e| String::from_utf8(e.path).ok())
                })
                .collect();
            return Ok(RebaseOutcome::Conflict(conflicts));
        }

        let tree_oid = merged_index.write_tree_to(&repo)?;
        commit_tree(
            &repo,
            branch,
            tree_oid,
            "sync: merge main into user branch",
            "cowiki",
            &[&branch_commit, &main_commit],
        )?;
        Ok(RebaseOutcome::Updated)
    }
}

#[cfg(test)]
mod tests {
    use super::{MergeOutcome, RebaseOutcome, WikiRepo};
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

    /// Regression for the `merge_to_main` self-deadlock: it held the "main" write
    /// lock then called `write_file("main", …)`, re-acquiring the same non-reentrant
    /// lock. Runs the merge on a worker thread with a timeout so this FAILS (rather
    /// than hanging the whole suite) if it ever regresses.
    #[test]
    fn merge_to_main_does_not_deadlock_and_applies_content() {
        let (repo, dir) = temp_repo("merge-deadlock");
        repo.ensure_branch_exists("user/test").unwrap();
        repo.write_file("user/test", "wiki/page.md", b"# Hello\n", "edit", "tester")
            .unwrap();

        let (tx, rx) = mpsc::channel();
        let r2 = Arc::clone(&repo);
        std::thread::spawn(move || {
            let res = r2.merge_to_main(
                "user/test",
                &["wiki/page.md".to_string()],
                "tester",
                "approve",
            );
            let _ = tx.send(res);
        });

        match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(Ok(MergeOutcome::Merged)) => {}
            Ok(other) => panic!("unexpected merge result: {other:?}"),
            Err(_) => panic!("merge_to_main deadlocked (timed out after 10s)"),
        }

        assert_eq!(
            read(&repo, "main", "wiki/page.md").as_deref(),
            Some("# Hello\n")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Nested paths must commit without touching the shared on-disk index.
    #[test]
    fn write_and_merge_nested_path() {
        let (repo, dir) = temp_repo("nested");
        repo.ensure_branch_exists("user/n").unwrap();
        repo.write_file("user/n", "wiki/a/b.md", b"deep\n", "edit", "n")
            .unwrap();
        assert_eq!(
            repo.merge_to_main("user/n", &["wiki/a/b.md".into()], "n", "m")
                .unwrap(),
            MergeOutcome::Merged
        );
        assert_eq!(
            read(&repo, "main", "wiki/a/b.md").as_deref(),
            Some("deep\n")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Merge must be authored by the submitter, and only apply the requested pages
    /// (an unsubmitted draft on the same branch must NOT leak into main).
    #[test]
    fn merge_only_applies_selected_pages_and_preserves_author() {
        let (repo, dir) = temp_repo("selective");
        repo.ensure_branch_exists("user/s").unwrap();
        repo.write_file("user/s", "wiki/submitted.md", b"yes\n", "e", "alice")
            .unwrap();
        repo.write_file("user/s", "wiki/draft.md", b"no\n", "e", "alice")
            .unwrap();

        let out = repo
            .merge_to_main("user/s", &["wiki/submitted.md".into()], "alice", "m")
            .unwrap();
        assert_eq!(out, MergeOutcome::Merged);
        assert_eq!(
            read(&repo, "main", "wiki/submitted.md").as_deref(),
            Some("yes\n")
        );
        assert_eq!(
            read(&repo, "main", "wiki/draft.md"),
            None,
            "draft must not leak"
        );

        // Author of the new main commit is the submitter, not a reviewer.
        let inner = repo.repo().unwrap();
        let head = inner
            .find_branch("main", git2::BranchType::Local)
            .unwrap()
            .get()
            .peel_to_commit()
            .unwrap();
        assert_eq!(head.author().name(), Some("alice"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Non-overlapping edits across branches merge cleanly even when main moved.
    #[test]
    fn merge_handles_main_advancing_on_other_pages() {
        let (repo, dir) = temp_repo("stale-ok");
        // user A adds page-a; merge it (main now has page-a).
        repo.ensure_branch_exists("user/a").unwrap();
        repo.write_file("user/a", "wiki/page-a.md", b"A\n", "e", "a")
            .unwrap();
        repo.merge_to_main("user/a", &["wiki/page-a.md".into()], "a", "m")
            .unwrap();

        // user B (branched before A merged) adds a different page-b.
        repo.ensure_branch_exists("user/b").unwrap();
        repo.write_file("user/b", "wiki/page-b.md", b"B\n", "e", "b")
            .unwrap();
        let out = repo
            .merge_to_main("user/b", &["wiki/page-b.md".into()], "b", "m")
            .unwrap();
        assert_eq!(out, MergeOutcome::Merged);
        // Both pages coexist on main — B's stale base didn't clobber A's page.
        assert_eq!(
            read(&repo, "main", "wiki/page-a.md").as_deref(),
            Some("A\n")
        );
        assert_eq!(
            read(&repo, "main", "wiki/page-b.md").as_deref(),
            Some("B\n")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Same page edited differently on main and branch → real conflict, nothing written.
    #[test]
    fn merge_detects_conflict_and_writes_nothing() {
        let (repo, dir) = temp_repo("conflict");
        // Seed page on main via user/seed.
        repo.ensure_branch_exists("user/seed").unwrap();
        repo.write_file("user/seed", "wiki/p.md", b"line1\nline2\n", "e", "s")
            .unwrap();
        repo.merge_to_main("user/seed", &["wiki/p.md".into()], "s", "m")
            .unwrap();

        // user/x branches from main, edits line2.
        repo.ensure_branch_exists("user/x").unwrap();
        repo.write_file("user/x", "wiki/p.md", b"line1\nX\n", "e", "x")
            .unwrap();
        // main edits the same line differently (via another branch merged in).
        repo.ensure_branch_exists("user/y").unwrap();
        repo.write_file("user/y", "wiki/p.md", b"line1\nY\n", "e", "y")
            .unwrap();
        repo.merge_to_main("user/y", &["wiki/p.md".into()], "y", "m")
            .unwrap();

        // Now user/x vs main conflict on the same line.
        let out = repo
            .merge_to_main("user/x", &["wiki/p.md".into()], "x", "m")
            .unwrap();
        assert_eq!(out, MergeOutcome::Conflict(vec!["wiki/p.md".to_string()]));
        // main keeps Y; nothing from x was written.
        assert_eq!(
            read(&repo, "main", "wiki/p.md").as_deref(),
            Some("line1\nY\n")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rebase_up_to_date_then_updated_then_conflict() {
        let (repo, dir) = temp_repo("rebase");
        // Fresh branch off main → up to date.
        repo.ensure_branch_exists("user/r").unwrap();
        assert_eq!(
            repo.rebase_onto_main("user/r").unwrap(),
            RebaseOutcome::UpToDate
        );

        // main advances on page-m (different page) → clean catch-up.
        repo.ensure_branch_exists("user/m").unwrap();
        repo.write_file("user/m", "wiki/page-m.md", b"M\n", "e", "m")
            .unwrap();
        repo.merge_to_main("user/m", &["wiki/page-m.md".into()], "m", "x")
            .unwrap();
        repo.write_file("user/r", "wiki/page-r.md", b"R\n", "e", "r")
            .unwrap();
        assert_eq!(
            repo.rebase_onto_main("user/r").unwrap(),
            RebaseOutcome::Updated
        );
        // After catch-up, the branch sees main's page-m AND keeps its own page-r.
        assert_eq!(
            read(&repo, "user/r", "wiki/page-m.md").as_deref(),
            Some("M\n")
        );
        assert_eq!(
            read(&repo, "user/r", "wiki/page-r.md").as_deref(),
            Some("R\n")
        );

        // Now create a real conflict: main and branch edit the same page/line.
        repo.write_file("user/r", "wiki/page-m.md", b"R-edit\n", "e", "r")
            .unwrap();
        repo.ensure_branch_exists("user/z").unwrap();
        repo.rebase_onto_main("user/z").unwrap();
        repo.write_file("user/z", "wiki/page-m.md", b"Z-edit\n", "e", "z")
            .unwrap();
        repo.merge_to_main("user/z", &["wiki/page-m.md".into()], "z", "x")
            .unwrap();
        match repo.rebase_onto_main("user/r").unwrap() {
            RebaseOutcome::Conflict(paths) => {
                assert!(paths.iter().any(|p| p == "wiki/page-m.md"))
            }
            other => panic!("expected conflict, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Regression for the merge-base drift bug: repeated edits to the SAME page on the
    /// SAME long-lived branch must merge cleanly. The 2-parent merge makes main an
    /// ancestor of the branch, advancing the base; previously the 2nd edit was a bogus
    /// add/add conflict that blocked every follow-up edit.
    #[test]
    fn repeat_edits_to_same_page_on_same_branch_merge_cleanly() {
        let (repo, dir) = temp_repo("repeat-edit");
        repo.ensure_branch_exists("user/ed").unwrap();

        repo.write_file("user/ed", "wiki/p.md", b"A1\n", "e", "ed")
            .unwrap();
        assert_eq!(
            repo.rebase_onto_main("user/ed").unwrap(),
            RebaseOutcome::UpToDate
        );
        assert_eq!(
            repo.merge_to_main("user/ed", &["wiki/p.md".into()], "ed", "m1")
                .unwrap(),
            MergeOutcome::Merged
        );
        assert_eq!(read(&repo, "main", "wiki/p.md").as_deref(), Some("A1\n"));

        // 2nd edit to the same page (the flow rebases then merges — both must be clean).
        repo.write_file("user/ed", "wiki/p.md", b"A1\nA2\n", "e", "ed")
            .unwrap();
        assert_eq!(
            repo.rebase_onto_main("user/ed").unwrap(),
            RebaseOutcome::Updated
        );
        assert_eq!(
            repo.merge_to_main("user/ed", &["wiki/p.md".into()], "ed", "m2")
                .unwrap(),
            MergeOutcome::Merged
        );
        assert_eq!(
            read(&repo, "main", "wiki/p.md").as_deref(),
            Some("A1\nA2\n")
        );

        // 3rd edit, to be sure the base keeps advancing.
        repo.write_file("user/ed", "wiki/p.md", b"A1\nA2\nA3\n", "e", "ed")
            .unwrap();
        repo.rebase_onto_main("user/ed").unwrap();
        assert_eq!(
            repo.merge_to_main("user/ed", &["wiki/p.md".into()], "ed", "m3")
                .unwrap(),
            MergeOutcome::Merged
        );
        assert_eq!(
            read(&repo, "main", "wiki/p.md").as_deref(),
            Some("A1\nA2\nA3\n")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A display name with angle brackets / control chars must be sanitized, never
    /// rejected by libgit2 (which would 500 every write/merge).
    #[test]
    fn writes_succeed_with_angle_bracket_author_name() {
        let (repo, dir) = temp_repo("weird-name");
        repo.ensure_branch_exists("user/w").unwrap();
        repo.write_file("user/w", "wiki/p.md", b"x\n", "edit", "Alice <a>")
            .unwrap();
        assert_eq!(read(&repo, "user/w", "wiki/p.md").as_deref(), Some("x\n"));
        let inner = repo.repo().unwrap();
        let c = inner
            .find_branch("user/w", git2::BranchType::Local)
            .unwrap()
            .get()
            .peel_to_commit()
            .unwrap();
        assert_eq!(c.author().name(), Some("Alice a"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `set_path_in_tree` must refuse to overwrite a directory with a file rather than
    /// silently dropping the whole subtree.
    #[test]
    fn writing_file_over_directory_errors_instead_of_clobbering() {
        let (repo, dir) = temp_repo("collide");
        repo.ensure_branch_exists("user/c").unwrap();
        repo.write_file("user/c", "wiki/d/inner.md", b"keep\n", "e", "c")
            .unwrap();
        assert!(
            repo.write_file("user/c", "wiki/d", b"oops\n", "e", "c")
                .is_err(),
            "writing a file over an existing directory must error"
        );
        // The subtree is untouched.
        assert_eq!(
            read(&repo, "user/c", "wiki/d/inner.md").as_deref(),
            Some("keep\n")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
