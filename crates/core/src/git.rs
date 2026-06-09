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

        // Write file to working directory
        let full_path = self.path.join(file_path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).ok();
        }
        fs::write(&full_path, content)
            .map_err(|e| git2::Error::from_str(&format!("write failed: {e}")))?;

        // Get the branch's current commit
        let branch_ref = repo.find_branch(branch, BranchType::Local)?;
        let parent_commit = branch_ref.get().peel_to_commit()?;

        // Build a new tree from the parent tree + the new file
        let blob_oid = repo.blob(content)?;
        let mut builder = repo.treebuilder(Some(&parent_commit.tree()?))?;

        // Handle nested paths by building subtrees
        let parts: Vec<&str> = file_path.split('/').collect();
        if parts.len() == 1 {
            builder.insert(parts[0], blob_oid, 0o100644)?;
        } else {
            // For nested paths, we need to rebuild the tree hierarchy
            // Simplified: use index-based approach
            let mut index = repo.index()?;
            // Reset index to parent tree
            index.read_tree(&parent_commit.tree()?)?;
            index.add_frombuffer(
                &git2::IndexEntry {
                    ctime: git2::IndexTime::new(0, 0),
                    mtime: git2::IndexTime::new(0, 0),
                    dev: 0,
                    ino: 0,
                    mode: 0o100644,
                    uid: 0,
                    gid: 0,
                    file_size: content.len() as u32,
                    id: blob_oid,
                    flags: 0,
                    flags_extended: 0,
                    path: file_path.as_bytes().to_vec(),
                },
                content,
            )?;
            let tree_oid = index.write_tree()?;
            let tree = repo.find_tree(tree_oid)?;
            let sig = Signature::now(author, &format!("{author}@cowiki"))?;
            repo.commit(
                Some(&format!("refs/heads/{branch}")),
                &sig,
                &sig,
                message,
                &tree,
                &[&parent_commit],
            )?;
            return Ok(());
        }

        let tree_oid = builder.write()?;
        let tree = repo.find_tree(tree_oid)?;
        let sig = Signature::now(author, &format!("{author}@cowiki"))?;
        repo.commit(
            Some(&format!("refs/heads/{branch}")),
            &sig,
            &sig,
            message,
            &tree,
            &[&parent_commit],
        )?;
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

    pub fn merge_to_main(
        &self,
        branch: &str,
        file_paths: &[String],
        author: &str,
        message: &str,
    ) -> Result<(), git2::Error> {
        let lock = self.branch_lock("main");
        let _guard = lock.write().unwrap();
        for path in file_paths {
            if let Some(content) = self.read_file(branch, path)? {
                self.write_file("main", path, &content, message, author)?;
            }
        }
        Ok(())
    }
}
