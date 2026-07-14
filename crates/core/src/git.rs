use git2::{BranchType, IndexEntry, IndexTime, Repository, Signature};
use std::collections::{BTreeMap, HashMap};
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
}

impl FileDiff {
    pub fn is_new(&self) -> bool {
        self.old_content.is_none() && self.new_content.is_some()
    }
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

    /// Get the "default" repo for backward compatibility.
    /// TODO: Remove once all routes use workspace-scoped repos.
    pub fn default_repo(&self) -> Result<Arc<WikiRepo>, git2::Error> {
        self.get("_default")
    }
}

impl WikiRepo {
    pub fn open_or_init(data_dir: &str) -> Result<Self, git2::Error> {
        let path = PathBuf::from(data_dir).join("repo");
        if path.exists() {
            Repository::open(&path)?;
        } else {
            let repo = Repository::init(&path)?;
            let root_index = crate::okf::root_index();
            fs::write(path.join("index.md"), root_index).map_err(|error| {
                git2::Error::from_str(&format!("write OKF root index failed: {error}"))
            })?;

            let sig = Signature::now("cowiki", "cowiki@local")?;
            let mut index = repo.index()?;
            index.add_path(Path::new("index.md"))?;
            index.write()?;
            let tree_id = index.write_tree()?;
            let tree = repo.find_tree(tree_id)?;
            repo.commit(Some("HEAD"), &sig, &sig, "init: empty wiki", &tree, &[])?;
        }

        // Ensure main branch exists (rename master → main if needed)
        rename_master_to_main(&path);

        let repo = Repository::open(&path)?;
        let branch_names = repo
            .branches(Some(BranchType::Local))?
            .filter_map(|branch| {
                branch
                    .ok()
                    .and_then(|(branch, _)| branch.name().ok().flatten().map(str::to_string))
            })
            .collect::<Vec<_>>();
        for branch in branch_names {
            migrate_branch_to_okf(&repo, &branch)?;
        }

        Ok(Self {
            path,
            write_locks: RwLock::new(HashMap::new()),
        })
    }

    fn repo(&self) -> Result<Repository, git2::Error> {
        Repository::open(&self.path)
    }

    pub fn commit_count(&self, branch: &str) -> Result<usize, git2::Error> {
        let repo = self.repo()?;
        let branch = repo.find_branch(branch, BranchType::Local)?;
        let mut walk = repo.revwalk()?;
        walk.push(branch.get().peel_to_commit()?.id())?;
        Ok(walk.count())
    }

    pub fn validate_okf_branch(
        &self,
        branch: &str,
    ) -> Result<crate::okf::BundleValidation, git2::Error> {
        let repo = self.repo()?;
        let branch = repo.find_branch(branch, BranchType::Local)?;
        let commit = branch.get().peel_to_commit()?;
        let mut blobs = Vec::new();
        collect_blobs(&repo, &commit.tree()?, "", &mut blobs)?;
        Ok(crate::okf::validate_bundle(blobs.into_iter().map(
            |(path, content)| crate::okf::BundleFile { path, content },
        )))
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
            let path =
                crate::okf::concept_path(slug).map_err(|error| git2::Error::from_str(&error))?;
            let main_content = self.read_file("main", &path)?;
            let branch_content = self.read_file(branch, &path)?;
            diffs.push(FileDiff {
                path,
                old_content: main_content.map(|b| String::from_utf8_lossy(&b).into_owned()),
                new_content: branch_content.map(|b| String::from_utf8_lossy(&b).into_owned()),
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

fn migrate_branch_to_okf(repo: &Repository, branch: &str) -> Result<(), git2::Error> {
    let branch_ref = repo.find_branch(branch, BranchType::Local)?;
    let parent = branch_ref.get().peel_to_commit()?;
    let mut blobs = Vec::new();
    collect_blobs(repo, &parent.tree()?, "", &mut blobs)?;

    let has_legacy_layout = blobs
        .iter()
        .any(|(path, _)| path.starts_with("wiki/") || path.starts_with("sources/"));
    let has_root_index = blobs.iter().any(|(path, _)| path == "index.md");
    let has_invalid_concept = blobs.iter().any(|(path, content)| {
        crate::okf::DocumentKind::from_path(path) == crate::okf::DocumentKind::Concept
            && !crate::okf::validate_document(path, content).is_empty()
    });
    let has_nonconforming_reserved = blobs.iter().any(|(path, content)| {
        matches!(
            crate::okf::DocumentKind::from_path(path),
            crate::okf::DocumentKind::Index | crate::okf::DocumentKind::Log
        ) && !crate::okf::validate_document(path, content).is_empty()
    });
    if !has_legacy_layout && has_root_index && !has_invalid_concept && !has_nonconforming_reserved {
        return Ok(());
    }

    let mut migrated: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for (old_path, content) in blobs {
        let new_path = crate::okf::migrate_legacy_path(&old_path);
        // If both old and canonical forms exist, the already-canonical file is authoritative.
        if old_path != new_path && migrated.contains_key(&new_path) {
            continue;
        }
        let new_content = if crate::okf::DocumentKind::from_path(&new_path)
            == crate::okf::DocumentKind::Concept
            && std::str::from_utf8(&content).is_err()
        {
            let legacy_path = format!(".cowiki/legacy/{new_path}.bin");
            migrated.insert(legacy_path.clone(), content);
            let notice = format!(
                "Legacy non-UTF-8 bytes were preserved at [legacy copy](/{legacy_path}).\n"
            );
            if new_path.starts_with(&format!("{}/", crate::okf::RAW_SOURCES_DIR)) {
                let filename = Path::new(&new_path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("source.md");
                crate::okf::source_document(filename, &notice)
                    .map(String::into_bytes)
                    .map_err(|error| git2::Error::from_str(&error))?
            } else {
                let fallback = Path::new(&new_path)
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .unwrap_or("Untitled");
                crate::okf::normalize_concept_document(&notice, fallback)
                    .map(String::into_bytes)
                    .map_err(|error| git2::Error::from_str(&error))?
            }
        } else if crate::okf::DocumentKind::from_path(&new_path) == crate::okf::DocumentKind::Log
            && !crate::okf::validate_document(&new_path, &content).is_empty()
        {
            let legacy_path = format!(".cowiki/legacy/{new_path}.txt");
            migrated.insert(legacy_path.clone(), content);
            crate::okf::replacement_log(&legacy_path).into_bytes()
        } else if new_path.starts_with(&format!("{}/", crate::okf::RAW_SOURCES_DIR)) {
            if crate::okf::validate_document(&new_path, &content).is_empty() {
                content
            } else {
                let filename = Path::new(&new_path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("source.md");
                crate::okf::source_document(filename, &String::from_utf8_lossy(&content))
                    .map(String::into_bytes)
                    .map_err(|error| git2::Error::from_str(&error))?
            }
        } else {
            match crate::okf::DocumentKind::from_path(&new_path) {
                crate::okf::DocumentKind::Concept => {
                    let fallback = Path::new(&new_path)
                        .file_stem()
                        .and_then(|name| name.to_str())
                        .unwrap_or("Untitled");
                    crate::okf::normalize_concept_document(
                        &String::from_utf8_lossy(&content),
                        fallback,
                    )
                    .unwrap_or_else(|_| {
                        let mut repaired = crate::okf::normalize_concept_document("", fallback)
                            .expect("empty OKF document normalization cannot fail");
                        repaired.push_str(&String::from_utf8_lossy(&content));
                        repaired
                    })
                    .into_bytes()
                }
                crate::okf::DocumentKind::Index => crate::okf::normalize_index_document(
                    &new_path,
                    &String::from_utf8_lossy(&content),
                )
                .unwrap_or_else(|_| {
                    let title = Path::new(&new_path)
                        .parent()
                        .and_then(Path::file_name)
                        .and_then(|name| name.to_str())
                        .unwrap_or("Knowledge");
                    let preserved = format!("# {title}\n\n{}", String::from_utf8_lossy(&content));
                    crate::okf::normalize_index_document(&new_path, &preserved)
                        .expect("generated index normalization cannot fail")
                })
                .into_bytes(),
                _ => content,
            }
        };
        if old_path == new_path {
            migrated.insert(new_path, new_content);
        } else {
            migrated.entry(new_path).or_insert(new_content);
        }
    }
    migrated
        .entry("index.md".into())
        .or_insert_with(|| crate::okf::root_index().into_bytes());

    let mut index = repo.index()?;
    index.clear()?;
    for (path, content) in migrated {
        index.add_frombuffer(&index_entry(&path, content.len()), &content)?;
    }
    let tree_id = index.write_tree_to(repo)?;
    if tree_id == parent.tree_id() {
        return Ok(());
    }
    let tree = repo.find_tree(tree_id)?;
    let sig = Signature::now("cowiki", "cowiki@local")?;
    repo.commit(
        Some(&format!("refs/heads/{branch}")),
        &sig,
        &sig,
        "migrate: align Space with OKF v0.1",
        &tree,
        &[&parent],
    )?;
    Ok(())
}

fn collect_blobs(
    repo: &Repository,
    tree: &git2::Tree<'_>,
    prefix: &str,
    output: &mut Vec<(String, Vec<u8>)>,
) -> Result<(), git2::Error> {
    for entry in tree.iter() {
        let Some(name) = entry.name() else { continue };
        let path = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}/{name}")
        };
        match entry.kind() {
            Some(git2::ObjectType::Tree) => {
                collect_blobs(repo, &repo.find_tree(entry.id())?, &path, output)?;
            }
            Some(git2::ObjectType::Blob) => {
                output.push((path, repo.find_blob(entry.id())?.content().to_vec()));
            }
            _ => {}
        }
    }
    Ok(())
}

fn index_entry(path: &str, size: usize) -> IndexEntry {
    IndexEntry {
        ctime: IndexTime::new(0, 0),
        mtime: IndexTime::new(0, 0),
        dev: 0,
        ino: 0,
        mode: 0o100644,
        uid: 0,
        gid: 0,
        file_size: size as u32,
        id: git2::Oid::zero(),
        flags: 0,
        flags_extended: 0,
        path: path.as_bytes().to_vec(),
    }
}
