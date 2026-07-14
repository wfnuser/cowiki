use git2::build::CheckoutBuilder;
use git2::{BranchType, IndexEntry, IndexTime, ObjectType, Repository, Signature};
use std::collections::{BTreeMap, BTreeSet, HashMap};
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
        let mut branch_names = repo
            .branches(Some(BranchType::Local))?
            .filter_map(|branch| {
                branch
                    .ok()
                    .and_then(|(branch, _)| branch.name().ok().flatten().map(str::to_string))
            })
            .collect::<Vec<_>>();
        let head_branch = repo
            .head()
            .ok()
            .and_then(|head| head.shorthand().map(str::to_string));
        // Migrate the checked-out branch last. If any detached branch cannot
        // migrate, HEAD and its working tree remain exactly as the user left
        // them instead of exposing a half-upgraded repository.
        branch_names.sort_by_key(|branch| head_branch.as_deref() == Some(branch.as_str()));
        let worktree_clean = repo.statuses(None)?.is_empty();
        if !worktree_clean {
            if let Some(head_branch) = head_branch.as_deref() {
                if branch_needs_okf_migration(&repo, head_branch)? {
                    return Err(git2::Error::from_str(
                        "cannot migrate OKF layout while the checked-out branch has uncommitted changes",
                    ));
                }
            }
        }
        let mut migrated_head = false;
        for branch in branch_names {
            let migrated = migrate_branch_to_okf(&repo, &branch)?;
            migrated_head |= migrated && head_branch.as_deref() == Some(branch.as_str());
        }
        if migrated_head {
            // The preflight above guarantees the working tree was clean. A forced
            // checkout is required because the branch ref moved without a normal
            // checkout transition, so libgit2's safe mode otherwise leaves the
            // old paths in place.
            repo.checkout_head(Some(CheckoutBuilder::new().force().remove_untracked(true)))?;
            let head_tree = repo.head()?.peel_to_tree()?;
            let mut index = repo.index()?;
            index.read_tree(&head_tree)?;
            index.write()?;
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
        let branch_ref = repo.find_branch(branch, BranchType::Local)?;
        let parent_commit = branch_ref.get().peel_to_commit()?;
        let mut blobs = Vec::new();
        collect_blobs(&repo, &parent_commit.tree()?, "", &mut blobs)?;
        let original = blobs.into_iter().collect::<BTreeMap<_, _>>();
        let mut files = original.clone();
        files.insert(file_path.to_string(), content.to_vec());
        refresh_all_progressive_indexes(&mut files)?;

        let parent_tree = parent_commit.tree()?;
        let mut index = repo.index()?;
        index.read_tree(&parent_tree)?;
        for (path, content) in files
            .iter()
            .filter(|(path, content)| original.get(*path) != Some(*content))
        {
            index.add_frombuffer(&index_entry(path, content.len()), content)?;
        }
        let tree_oid = index.write_tree_to(&repo)?;
        if tree_oid == parent_commit.tree_id() {
            return Ok(());
        }
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

        let is_checked_out = repo
            .head()
            .ok()
            .and_then(|head| head.shorthand().map(str::to_string))
            .as_deref()
            == Some(branch);
        if is_checked_out {
            for (path, content) in files
                .iter()
                .filter(|(path, content)| original.get(*path) != Some(*content))
            {
                let full_path = self.path.join(path);
                let write_result = full_path
                    .parent()
                    .map(fs::create_dir_all)
                    .transpose()
                    .and_then(|_| fs::write(&full_path, content));
                if let Err(error) = write_result {
                    tracing::warn!(path, %error, "Git commit succeeded but worktree mirror failed");
                }
            }
            if let Err(error) = index.read_tree(&tree).and_then(|_| index.write()) {
                tracing::warn!(%error, "Git commit succeeded but index mirror failed");
            }
        }
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
                crate::okf::document_path(slug).map_err(|error| git2::Error::from_str(&error))?;
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
        for path in file_paths {
            if let Some(content) = self.read_file(branch, path)? {
                self.write_file("main", path, &content, message, author)?;
            }
        }
        Ok(())
    }
}

fn progressive_index_entries(
    files: &BTreeMap<String, Vec<u8>>,
    directory: &str,
) -> Vec<crate::okf::IndexEntry> {
    let prefix = if directory.is_empty() {
        String::new()
    } else {
        format!("{directory}/")
    };
    let mut concepts = BTreeMap::new();
    let mut subdirectories = BTreeSet::new();
    for path in files.keys() {
        let kind = crate::okf::DocumentKind::from_path(path);
        if kind == crate::okf::DocumentKind::Other {
            continue;
        }
        let Some(relative) = path.strip_prefix(&prefix) else {
            continue;
        };
        if relative.starts_with('.') {
            continue;
        }
        if let Some((child, _)) = relative.split_once('/') {
            if !child.starts_with('.') {
                subdirectories.insert(child.to_string());
            }
            continue;
        }
        if kind == crate::okf::DocumentKind::Concept {
            concepts.insert(relative.to_string(), path.to_string());
        }
    }

    let mut entries = Vec::new();
    for child in subdirectories {
        let child_index = if directory.is_empty() {
            format!("{child}/index.md")
        } else {
            format!("{directory}/{child}/index.md")
        };
        let title = files
            .get(&child_index)
            .map(|content| crate::okf::index_title(&child_index, &String::from_utf8_lossy(content)))
            .unwrap_or_else(|| humanize_segment(&child));
        entries.push(crate::okf::IndexEntry {
            title,
            target: format!("{child}/"),
            description: None,
        });
    }
    for (relative, path) in concepts {
        let content = files.get(&path).expect("indexed concept must exist");
        let (title, description) = crate::okf::display_metadata(&String::from_utf8_lossy(content));
        let fallback = Path::new(&relative)
            .file_stem()
            .and_then(|name| name.to_str())
            .map(humanize_segment)
            .unwrap_or_else(|| "Untitled".into());
        entries.push(crate::okf::IndexEntry {
            title: title.unwrap_or(fallback),
            target: relative,
            description,
        });
    }
    entries
}

fn humanize_segment(segment: &str) -> String {
    let words = segment.replace(['-', '_'], " ");
    let mut chars = words.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => "Knowledge".into(),
    }
}

fn migrate_branch_to_okf(repo: &Repository, branch: &str) -> Result<bool, git2::Error> {
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
        return Ok(false);
    }

    let originals = blobs.iter().cloned().collect::<BTreeMap<_, _>>();
    let mut migrated: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut origins: BTreeMap<String, String> = BTreeMap::new();
    let mut collision_archives = Vec::new();
    for (old_path, content) in blobs {
        let new_path = crate::okf::migrate_legacy_path(&old_path);
        if new_path != "index.md"
            && crate::okf::DocumentKind::from_path(&new_path) == crate::okf::DocumentKind::Index
            && content.starts_with(b"---\n")
        {
            let archive_path = unique_migration_path(
                &format!(".cowiki/legacy/indexes/{new_path}.legacy"),
                &migrated,
                &originals,
            );
            migrated.insert(archive_path, content.clone());
        }
        let new_content = if crate::okf::DocumentKind::from_path(&new_path)
            == crate::okf::DocumentKind::Concept
            && std::str::from_utf8(&content).is_err()
        {
            let legacy_path = unique_migration_path(
                &format!(".cowiki/legacy/{new_path}.bin"),
                &migrated,
                &originals,
            );
            migrated.insert(legacy_path.clone(), content);
            let notice = format!(
                "Legacy non-UTF-8 bytes were preserved at [legacy copy](/{legacy_path}).\n"
            );
            if new_path.starts_with(&format!("{}/", crate::okf::RAW_SOURCES_DIR)) {
                let filename = Path::new(&old_path)
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
            let legacy_path = unique_migration_path(
                &format!(".cowiki/legacy/{new_path}.txt"),
                &migrated,
                &originals,
            );
            migrated.insert(legacy_path.clone(), content);
            crate::okf::replacement_log(&legacy_path).into_bytes()
        } else if new_path.starts_with(&format!("{}/", crate::okf::RAW_SOURCES_DIR)) {
            if crate::okf::validate_document(&new_path, &content).is_empty() {
                content
            } else {
                let filename = Path::new(&old_path)
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
                    if crate::okf::validate_document(&new_path, &content).is_empty() {
                        content
                    } else {
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

        if let Some(existing_origin) = origins.get(&new_path).cloned() {
            let canonical_is_existing = existing_origin == new_path;
            let canonical_is_current = old_path == new_path;
            if canonical_is_existing {
                let archived = content_for_archive(&originals, &old_path);
                collision_archives.push((old_path, archived));
                continue;
            }
            if canonical_is_current {
                collision_archives.push((
                    existing_origin.clone(),
                    content_for_archive(&originals, &existing_origin),
                ));
                migrated.insert(new_path.clone(), new_content);
                origins.insert(new_path, old_path);
                continue;
            }
            let archived = content_for_archive(&originals, &old_path);
            collision_archives.push((old_path, archived));
            continue;
        }
        migrated.insert(new_path.clone(), new_content);
        origins.insert(new_path, old_path);
    }
    migrated
        .entry("index.md".into())
        .or_insert_with(|| crate::okf::root_index().into_bytes());
    for (old_path, content) in collision_archives {
        let oid = git2::Oid::hash_object(ObjectType::Blob, &content)?;
        let mut archive_path = format!(".cowiki/legacy/collisions/{old_path}.{oid}.legacy");
        let mut suffix = 1;
        while migrated.contains_key(&archive_path) || originals.contains_key(&archive_path) {
            archive_path = format!(".cowiki/legacy/collisions/{old_path}.{oid}.{suffix}.legacy");
            suffix += 1;
        }
        migrated.insert(archive_path, content);
    }
    refresh_all_progressive_indexes(&mut migrated)?;

    let mut index = repo.index()?;
    index.clear()?;
    for (path, content) in migrated {
        index.add_frombuffer(&index_entry(&path, content.len()), &content)?;
    }
    let tree_id = index.write_tree_to(repo)?;
    if tree_id == parent.tree_id() {
        return Ok(false);
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
    Ok(true)
}

fn refresh_all_progressive_indexes(
    files: &mut BTreeMap<String, Vec<u8>>,
) -> Result<(), git2::Error> {
    let mut directories = BTreeSet::new();
    let paths = files.keys().cloned().collect::<Vec<_>>();
    for path in paths {
        if path.split('/').any(|segment| segment.starts_with('.')) {
            continue;
        }
        let kind = crate::okf::DocumentKind::from_path(&path);
        if !matches!(
            kind,
            crate::okf::DocumentKind::Concept | crate::okf::DocumentKind::Index
        ) {
            continue;
        }
        let mut current = Path::new(&path).parent();
        loop {
            let directory = current
                .filter(|path| !path.as_os_str().is_empty())
                .map(|path| path.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            directories.insert(directory);
            let Some(path) = current.filter(|path| !path.as_os_str().is_empty()) else {
                break;
            };
            current = path.parent();
        }
    }

    let mut directories = directories.into_iter().collect::<Vec<_>>();
    directories.sort_by_key(|directory| {
        std::cmp::Reverse(
            directory
                .split('/')
                .filter(|segment| !segment.is_empty())
                .count(),
        )
    });
    for directory in directories {
        let index_path = if directory.is_empty() {
            "index.md".to_string()
        } else {
            format!("{directory}/index.md")
        };
        let existing = files.get(&index_path).cloned().unwrap_or_else(|| {
            if directory.is_empty() {
                crate::okf::root_index().into_bytes()
            } else {
                let title = humanize_segment(
                    Path::new(&directory)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("Knowledge"),
                );
                crate::okf::folder_index(&title).into_bytes()
            }
        });
        let entries = progressive_index_entries(files, &directory);
        let updated = crate::okf::update_index_entries(
            &index_path,
            &String::from_utf8_lossy(&existing),
            &entries,
        )
        .map_err(|error| git2::Error::from_str(&error))?;
        files.insert(index_path, updated.into_bytes());
    }
    Ok(())
}

fn content_for_archive(originals: &BTreeMap<String, Vec<u8>>, path: &str) -> Vec<u8> {
    originals
        .get(path)
        .cloned()
        .expect("migration origin must exist in the parent tree")
}

fn unique_migration_path(
    preferred: &str,
    migrated: &BTreeMap<String, Vec<u8>>,
    originals: &BTreeMap<String, Vec<u8>>,
) -> String {
    if !migrated.contains_key(preferred) && !originals.contains_key(preferred) {
        return preferred.to_string();
    }
    let mut suffix = 1;
    loop {
        let candidate = format!("{preferred}.{suffix}");
        if !migrated.contains_key(&candidate) && !originals.contains_key(&candidate) {
            return candidate;
        }
        suffix += 1;
    }
}

fn branch_needs_okf_migration(repo: &Repository, branch: &str) -> Result<bool, git2::Error> {
    let branch_ref = repo.find_branch(branch, BranchType::Local)?;
    let parent = branch_ref.get().peel_to_commit()?;
    let mut blobs = Vec::new();
    collect_blobs(repo, &parent.tree()?, "", &mut blobs)?;
    let has_legacy_layout = blobs
        .iter()
        .any(|(path, _)| path.starts_with("wiki/") || path.starts_with("sources/"));
    let has_root_index = blobs.iter().any(|(path, _)| path == "index.md");
    let has_invalid = blobs.iter().any(|(path, content)| {
        matches!(
            crate::okf::DocumentKind::from_path(path),
            crate::okf::DocumentKind::Concept
                | crate::okf::DocumentKind::Index
                | crate::okf::DocumentKind::Log
        ) && !crate::okf::validate_document(path, content).is_empty()
    });
    Ok(has_legacy_layout || !has_root_index || has_invalid)
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
