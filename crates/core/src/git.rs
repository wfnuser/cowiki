use git2::{BranchType, Repository, Signature};
use std::fs;
use std::path::{Path, PathBuf};

pub struct WikiRepo {
    path: PathBuf,
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

impl WikiRepo {
    pub fn open_or_init(data_dir: &str) -> Result<Self, git2::Error> {
        let path = PathBuf::from(data_dir).join("repo");
        if path.exists() {
            Repository::open(&path)?;
        } else {
            let repo = Repository::init(&path)?;
            fs::create_dir_all(path.join("wiki")).ok();
            fs::create_dir_all(path.join("sources")).ok();

            // Create .gitkeep files so directories are tracked
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
        Ok(Self { path })
    }

    fn repo(&self) -> Result<Repository, git2::Error> {
        Repository::open(&self.path)
    }

    pub fn ensure_user_branch(&self, user_id: &str) -> Result<String, git2::Error> {
        let branch_name = format!("user/{user_id}");
        let repo = self.repo()?;

        if repo.find_branch(&branch_name, BranchType::Local).is_ok() {
            return Ok(branch_name);
        }

        // Find main or master branch
        let main = repo
            .find_branch("main", BranchType::Local)
            .or_else(|_| repo.find_branch("master", BranchType::Local))?;
        let commit = main.get().peel_to_commit()?;
        repo.branch(&branch_name, &commit, false)?;
        Ok(branch_name)
    }

    pub fn write_file(
        &self,
        branch: &str,
        file_path: &str,
        content: &[u8],
        message: &str,
        author: &str,
    ) -> Result<(), git2::Error> {
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

    pub fn read_file(
        &self,
        branch: &str,
        file_path: &str,
    ) -> Result<Option<Vec<u8>>, git2::Error> {
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

    pub fn diff_files(
        &self,
        branch: &str,
        slugs: &[String],
    ) -> Result<Vec<FileDiff>, git2::Error> {
        let mut diffs = Vec::new();
        for slug in slugs {
            let path = format!("wiki/{slug}.md");
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
