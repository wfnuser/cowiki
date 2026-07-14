use git2::{IndexEntry, IndexTime, Oid, Repository, Signature, StatusOptions};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use similar::{ChangeTag, TextDiff};
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;

use crate::knowledge_index::{self, SearchHit};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Space {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub visibility: String,
    pub role: String,
    pub local_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SubmitResult {
    pub committed: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PageMeta {
    pub slug: String,
    pub path: String,
    pub title: String,
    pub summary: String,
    pub branch: String,
    pub kind: String,
    pub children: Vec<PageMeta>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PageFull {
    #[serde(flatten)]
    pub meta: PageMeta,
    pub body: String,
    pub edited_by: Option<String>,
    pub edited_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SourceItem {
    pub filename: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SourceContent {
    pub filename: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SearchResponse {
    pub keyword: Vec<SearchHit>,
    pub semantic: Vec<SearchHit>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: String,
    pub old_line: Option<usize>,
    pub new_line: Option<usize>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiffHunk {
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileDiff {
    pub path: String,
    pub old_content: Option<String>,
    pub new_content: Option<String>,
    pub hunks: Vec<DiffHunk>,
    pub additions: usize,
    pub deletions: usize,
}

pub struct LocalEngine {
    db: Mutex<Connection>,
}

impl LocalEngine {
    pub fn open(metadata_dir: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(metadata_dir).map_err(|e| e.to_string())?;
        let connection =
            Connection::open(metadata_dir.join("local.db")).map_err(|e| e.to_string())?;
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS spaces (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    slug TEXT NOT NULL UNIQUE,
                    local_path TEXT NOT NULL UNIQUE,
                    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                );",
            )
            .map_err(|e| e.to_string())?;
        knowledge_index::initialize(&connection)?;
        let engine = Self {
            db: Mutex::new(connection),
        };
        engine.rebuild_all_search_indexes()?;
        Ok(engine)
    }

    pub fn list_spaces(&self) -> Result<Vec<Space>, String> {
        let db = self
            .db
            .lock()
            .map_err(|_| "local database lock poisoned".to_string())?;
        let mut statement = db
            .prepare("SELECT id, name, slug, local_path FROM spaces ORDER BY created_at, name")
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([], |row| {
                Ok(Space {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    slug: row.get(2)?,
                    visibility: "private".to_string(),
                    role: "owner".to_string(),
                    local_path: PathBuf::from(row.get::<_, String>(3)?),
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    /// Import repositories created by the previous embedded local runtime.
    /// This is intentionally idempotent so every startup can safely repair a
    /// missing local index without changing the Markdown/Git repositories.
    pub fn import_legacy_spaces(&self, cowiki_home: &Path) -> Result<usize, String> {
        if !cowiki_home.is_dir() {
            return Ok(0);
        }
        let mut imported = 0;
        for entry in std::fs::read_dir(cowiki_home).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let repo = entry.path().join("repo");
            if !repo.join(".git").exists() {
                continue;
            }
            let repo = repo.canonicalize().map_err(|e| e.to_string())?;
            let slug = entry.file_name().to_string_lossy().to_string();
            if self.space_by_path(&repo)?.is_some() {
                continue;
            }
            let name = if slug.starts_with("personal-") {
                "My Space".to_string()
            } else if slug.starts_with("general-") {
                "General".to_string()
            } else {
                slug.replace(['-', '_'], " ")
            };
            let space = self.insert_space(&name, &slug, &repo)?;
            self.rebuild_search_index(&space.slug)?;
            imported += 1;
        }
        Ok(imported)
    }

    pub fn add_space(&self, name: &str, slug: &str, folder: &Path) -> Result<Space, String> {
        validate_slug(slug)?;
        if !folder.is_dir() {
            return Err("selected Space must be an existing directory".to_string());
        }
        let local_path = folder.canonicalize().map_err(|e| e.to_string())?;
        if let Some(existing) = self.space_by_path(&local_path)? {
            return Ok(existing);
        }
        Repository::open(&local_path)
            .or_else(|_| Repository::init(&local_path))
            .map_err(|e| format!("cannot initialize local Git repository: {e}"))?;

        let space = self.insert_space(name, slug, &local_path)?;
        self.rebuild_search_index(&space.slug)?;
        Ok(space)
    }

    fn insert_space(&self, name: &str, slug: &str, local_path: &Path) -> Result<Space, String> {
        let space = Space {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.trim().to_string(),
            slug: slug.to_string(),
            visibility: "private".to_string(),
            role: "owner".to_string(),
            local_path: local_path.to_path_buf(),
        };
        let db = self
            .db
            .lock()
            .map_err(|_| "local database lock poisoned".to_string())?;
        db.execute(
            "INSERT INTO spaces (id, name, slug, local_path) VALUES (?1, ?2, ?3, ?4)",
            params![
                space.id,
                space.name,
                space.slug,
                space.local_path.to_string_lossy().as_ref()
            ],
        )
        .map_err(|e| format!("cannot add local Space: {e}"))?;
        Ok(space)
    }

    fn space_by_path(&self, path: &Path) -> Result<Option<Space>, String> {
        Ok(self
            .list_spaces()?
            .into_iter()
            .find(|space| space.local_path == path))
    }

    pub fn find_space(&self, slug: &str) -> Result<Space, String> {
        self.list_spaces()?
            .into_iter()
            .find(|space| space.slug == slug)
            .ok_or_else(|| format!("Space '{slug}' is not registered on this device"))
    }

    pub fn find_space_by_path(&self, path: &Path) -> Result<Space, String> {
        let canonical = path.canonicalize().map_err(|e| e.to_string())?;
        self.space_by_path(&canonical)?
            .ok_or_else(|| "directory is not an opened CoWiki Space".to_string())
    }

    #[cfg(test)]
    pub fn write_page(
        &self,
        space_slug: &str,
        dir: &str,
        page_slug: &str,
        content: &str,
    ) -> Result<(), String> {
        self.write_page_checked(space_slug, dir, page_slug, content, None, false)
    }

    pub fn write_page_checked(
        &self,
        space_slug: &str,
        dir: &str,
        page_slug: &str,
        content: &str,
        expected_content: Option<&str>,
        create_only: bool,
    ) -> Result<(), String> {
        let space = self.find_space(space_slug)?;
        let path = content_path(&space.local_path, dir, page_slug, true)?;
        if let Some(expected) = expected_content {
            let current = std::fs::read_to_string(&path)
                .map_err(|error| format!("cannot verify the current page: {error}"))?;
            if current != expected {
                return Err(
                    "This page changed outside CoWiki. Your text is still open; review the latest file before saving again."
                        .to_string(),
                );
            }
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let temporary =
            path.with_extension(format!("md.cowiki-{}.tmp", uuid::Uuid::new_v4().simple()));
        std::fs::write(&temporary, content).map_err(|e| e.to_string())?;
        if create_only {
            // `exists` followed by `rename` is a TOCTOU overwrite on macOS.
            // A hard link publishes the completed temp file atomically and
            // fails if an Agent created the target after the editor noticed
            // the deletion. Both paths live in the same directory/filesystem.
            if let Err(error) = std::fs::hard_link(&temporary, &path) {
                let _ = std::fs::remove_file(&temporary);
                return if error.kind() == std::io::ErrorKind::AlreadyExists {
                    Err("a page with this name already exists".to_string())
                } else {
                    Err(error.to_string())
                };
            }
            std::fs::remove_file(&temporary).map_err(|e| e.to_string())?;
        } else {
            std::fs::rename(&temporary, &path).map_err(|e| e.to_string())?;
        }
        let mut db = self
            .db
            .lock()
            .map_err(|_| "local database lock poisoned".to_string())?;
        knowledge_index::index_file(&mut db, &space.id, &space.local_path, &path)
    }

    pub fn create_folder(
        &self,
        space_slug: &str,
        name: &str,
        parent: Option<&str>,
    ) -> Result<(), String> {
        let space = self.find_space(space_slug)?;
        let clean_name = safe_component(name, "folder name")?;
        let parent = parent.unwrap_or("wiki");
        let parent_path = ui_path(&space.local_path, parent)?;
        let folder = parent_path.join(clean_name);
        ensure_inside(&space.local_path, &folder)?;
        std::fs::create_dir(&folder).map_err(|error| format!("cannot create folder: {error}"))
    }

    pub fn ingest(
        &self,
        space_slug: &str,
        source_type: &str,
        content: &str,
        filename: Option<&str>,
    ) -> Result<SourceItem, String> {
        let space = self.find_space(space_slug)?;
        let sources = section_root(&space.local_path, "sources")?;
        std::fs::create_dir_all(&sources).map_err(|error| error.to_string())?;

        let fallback = match source_type {
            "url" => url::Url::parse(content.trim())
                .ok()
                .and_then(|value| value.host_str().map(ToOwned::to_owned))
                .unwrap_or_else(|| "web-source".to_string()),
            _ => "source".to_string(),
        };
        let requested = filename.unwrap_or(&fallback);
        let stem = slugify_filename(requested);
        let mut candidate = sources.join(format!("{stem}.md"));
        if candidate.exists() {
            candidate = sources.join(format!(
                "{stem}-{}.md",
                &uuid::Uuid::new_v4().simple().to_string()[..8]
            ));
        }
        ensure_inside(&space.local_path, &candidate)?;

        let title = if source_type == "url" {
            content.trim()
        } else {
            requested.trim()
        };
        let body = format!(
            "---\ntitle: {}\ntype: Source\n---\n\n{}\n",
            yaml_string(title),
            content.trim()
        );
        std::fs::write(&candidate, body).map_err(|error| error.to_string())?;
        let mut db = self
            .db
            .lock()
            .map_err(|_| "local database lock poisoned".to_string())?;
        knowledge_index::index_file(&mut db, &space.id, &space.local_path, &candidate)?;
        Ok(SourceItem {
            filename: candidate
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
        })
    }

    pub fn rename_path(&self, space_slug: &str, from: &str, to: &str) -> Result<(), String> {
        let space = self.find_space(space_slug)?;
        let from = ui_path(&space.local_path, from)?;
        let to = ui_path(&space.local_path, to)?;
        ensure_inside(&space.local_path, &from)?;
        ensure_inside(&space.local_path, &to)?;
        if !from.exists() {
            return Err("the item to rename no longer exists".to_string());
        }
        if to.exists() {
            return Err("an item with that name already exists".to_string());
        }
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::rename(&from, &to).map_err(|error| format!("cannot rename item: {error}"))?;
        self.rebuild_search_index(space_slug)
    }

    pub fn delete_path(&self, space_slug: &str, value: &str) -> Result<(), String> {
        let space = self.find_space(space_slug)?;
        let path = ui_path(&space.local_path, value)?;
        ensure_inside(&space.local_path, &path)?;
        if path == space.local_path {
            return Err("cannot delete the Space root".to_string());
        }
        if path.is_dir() {
            std::fs::remove_dir_all(&path)
                .map_err(|error| format!("cannot delete folder: {error}"))?;
        } else if path.is_file() {
            std::fs::remove_file(&path).map_err(|error| format!("cannot delete file: {error}"))?;
        } else {
            return Err("the item to delete no longer exists".to_string());
        }
        self.rebuild_search_index(space_slug)
    }

    pub fn list_pages(&self, space_slug: &str, dir: &str) -> Result<Vec<PageMeta>, String> {
        let space = self.find_space(space_slug)?;
        let sections = if dir == "all" {
            vec!["wiki", "entities", "concepts"]
        } else {
            vec![dir]
        };
        sections
            .into_iter()
            .map(|section| {
                let root = section_root(&space.local_path, section)?;
                Ok(PageMeta {
                    slug: section.to_string(),
                    path: section.to_string(),
                    title: title_case(section),
                    summary: String::new(),
                    branch: "local".to_string(),
                    kind: "folder".to_string(),
                    children: read_page_tree(&root, section, &root)?,
                })
            })
            .collect()
    }

    pub fn get_page(&self, space_slug: &str, dir: &str, slug: &str) -> Result<PageFull, String> {
        let space = self.find_space(space_slug)?;
        let path = content_path(&space.local_path, dir, slug, true)?;
        let body = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let title = markdown_title(&body).unwrap_or_else(|| {
            path.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        });
        Ok(PageFull {
            meta: PageMeta {
                slug: slug.to_string(),
                path: format!("{dir}/{slug}.md"),
                title,
                summary: String::new(),
                branch: "local".to_string(),
                kind: "page".to_string(),
                children: vec![],
            },
            body,
            edited_by: None,
            edited_at: None,
        })
    }

    pub fn list_sources(&self, space_slug: &str) -> Result<Vec<SourceItem>, String> {
        let space = self.find_space(space_slug)?;
        let root = section_root(&space.local_path, "sources")?;
        if !root.is_dir() {
            return Ok(vec![]);
        }
        let mut sources = Vec::new();
        for entry in walkdir::WalkDir::new(root)
            .into_iter()
            .filter_map(Result::ok)
        {
            if entry.file_type().is_file()
                && !entry
                    .path()
                    .components()
                    .any(|component| component.as_os_str().to_string_lossy().starts_with('.'))
            {
                sources.push(SourceItem {
                    filename: entry
                        .path()
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                });
            }
        }
        Ok(sources)
    }

    pub fn get_source(&self, space_slug: &str, filename: &str) -> Result<SourceContent, String> {
        let space = self.find_space(space_slug)?;
        let path = content_path(&space.local_path, "sources", filename, false)?;
        Ok(SourceContent {
            filename: filename.to_string(),
            content: std::fs::read_to_string(path).map_err(|e| e.to_string())?,
        })
    }

    pub fn rebuild_search_index(&self, space_slug: &str) -> Result<(), String> {
        let space = self.find_space(space_slug)?;
        let mut db = self
            .db
            .lock()
            .map_err(|_| "local database lock poisoned".to_string())?;
        knowledge_index::rebuild_space(&mut db, &space.id, &space.local_path)
    }

    pub fn rebuild_all_search_indexes(&self) -> Result<(), String> {
        for space in self.list_spaces()? {
            if space.local_path.is_dir() {
                let mut db = self
                    .db
                    .lock()
                    .map_err(|_| "local database lock poisoned".to_string())?;
                knowledge_index::rebuild_space(&mut db, &space.id, &space.local_path)?;
            }
        }
        Ok(())
    }

    pub fn search_pages(
        &self,
        space_slug: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchHit>, String> {
        let space = self.find_space(space_slug)?;
        let mut db = self
            .db
            .lock()
            .map_err(|_| "local database lock poisoned".to_string())?;
        knowledge_index::refresh_space(&mut db, &space.id, &space.local_path)?;
        knowledge_index::search(&db, &space.id, query, limit)
    }

    pub fn search(
        &self,
        space_slug: &str,
        query: &str,
        limit: usize,
    ) -> Result<SearchResponse, String> {
        let space = self.find_space(space_slug)?;
        let keyword = self
            .search_pages(space_slug, query, limit)?
            .into_iter()
            .filter_map(|hit| ui_search_hit(&space.local_path, hit))
            .collect();
        Ok(SearchResponse {
            keyword,
            semantic: Vec::new(),
        })
    }

    pub fn list_backlinks(
        &self,
        space_slug: &str,
        target_path: &str,
    ) -> Result<Vec<SearchHit>, String> {
        let space = self.find_space(space_slug)?;
        let mut db = self
            .db
            .lock()
            .map_err(|_| "local database lock poisoned".to_string())?;
        knowledge_index::refresh_space(&mut db, &space.id, &space.local_path)?;
        knowledge_index::backlinks(&db, &space.id, target_path)
    }

    pub fn indexed_page_count(&self, space_slug: &str) -> Result<usize, String> {
        let space = self.find_space(space_slug)?;
        let db = self
            .db
            .lock()
            .map_err(|_| "local database lock poisoned".to_string())?;
        knowledge_index::page_count(&db, &space.id)
    }

    pub fn get_page_by_path(
        &self,
        space_slug: &str,
        relative_path: &str,
    ) -> Result<PageFull, String> {
        let space = self.find_space(space_slug)?;
        let relative = Path::new(relative_path);
        if relative.as_os_str().is_empty()
            || relative
                .extension()
                .is_none_or(|extension| !extension.eq_ignore_ascii_case("md"))
            || relative.components().any(|component| {
                !matches!(component, Component::Normal(_))
                    || component.as_os_str().to_string_lossy().starts_with('.')
            })
        {
            return Err("invalid Markdown path".to_string());
        }
        let path = space.local_path.join(relative);
        let body = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let title = markdown_title(&body).unwrap_or_else(|| {
            path.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        });
        let normalized = relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        let slug = normalized
            .strip_suffix(".md")
            .unwrap_or(&normalized)
            .to_string();
        Ok(PageFull {
            meta: PageMeta {
                slug,
                path: normalized,
                title,
                summary: String::new(),
                branch: "local".to_string(),
                kind: "page".to_string(),
                children: Vec::new(),
            },
            body,
            edited_by: None,
            edited_at: None,
        })
    }

    #[cfg(test)]
    pub fn commit_count(&self, space_slug: &str) -> Result<usize, String> {
        let repo = self.repo(space_slug)?;
        let mut walk = repo.revwalk().map_err(|e| e.to_string())?;
        if walk.push_head().is_err() {
            return Ok(0);
        }
        Ok(walk.filter_map(Result::ok).count())
    }

    #[cfg(test)]
    pub fn has_uncommitted_changes(&self, space_slug: &str) -> Result<bool, String> {
        let repo = self.repo(space_slug)?;
        let mut options = StatusOptions::new();
        options.include_untracked(true).recurse_untracked_dirs(true);
        let has_changes = !repo
            .statuses(Some(&mut options))
            .map_err(|e| e.to_string())?
            .is_empty();
        Ok(has_changes)
    }

    pub fn submit(&self, space_slug: &str, paths: &[String]) -> Result<SubmitResult, String> {
        let repo = self.repo(space_slug)?;
        let mut index = repo.index().map_err(|e| e.to_string())?;
        if let Ok(head) = repo.head().and_then(|head| head.peel_to_commit()) {
            let tree = head.tree().map_err(|error| error.to_string())?;
            index.read_tree(&tree).map_err(|error| error.to_string())?;
        } else {
            index.clear().map_err(|error| error.to_string())?;
        }
        for value in paths {
            let path = safe_repo_path(value)?;
            if repo.workdir().is_some_and(|root| root.join(path).is_file()) {
                index.add_path(path).map_err(|error| error.to_string())?;
            } else {
                let _ = index.remove_path(path);
            }
        }
        index.write().map_err(|e| e.to_string())?;
        let tree_id = index.write_tree().map_err(|e| e.to_string())?;

        let parent = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
        if parent
            .as_ref()
            .is_some_and(|commit| commit.tree_id() == tree_id)
        {
            return Ok(SubmitResult { committed: false });
        }

        let tree = repo.find_tree(tree_id).map_err(|e| e.to_string())?;
        let signature =
            Signature::now("CoWiki Local", "local@cowiki.app").map_err(|e| e.to_string())?;
        let parents: Vec<&git2::Commit<'_>> = parent.iter().collect();
        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            "Update local Space",
            &tree,
            &parents,
        )
        .map_err(|e| e.to_string())?;
        Ok(SubmitResult { committed: true })
    }

    pub fn keep_working_diff(
        &self,
        space_slug: &str,
        expected: &[FileDiff],
    ) -> Result<SubmitResult, String> {
        let current = self.working_diff(space_slug)?;
        if current != expected {
            return Err(
                "Local files changed after this Review opened. Refresh Reviews before keeping changes."
                    .to_string(),
            );
        }
        let repo = self.repo(space_slug)?;
        let mut index = repo.index().map_err(|error| error.to_string())?;
        reset_index_to_head(&repo, &mut index)?;
        for diff in expected {
            let path = safe_repo_path(&diff.path)?;
            if let Some(content) = &diff.new_content {
                let entry = IndexEntry {
                    ctime: IndexTime::new(0, 0),
                    mtime: IndexTime::new(0, 0),
                    dev: 0,
                    ino: 0,
                    mode: 0o100644,
                    uid: 0,
                    gid: 0,
                    file_size: 0,
                    id: Oid::zero(),
                    flags: 0,
                    flags_extended: 0,
                    path: diff.path.as_bytes().to_vec(),
                };
                index
                    .add_frombuffer(&entry, content.as_bytes())
                    .map_err(|error| error.to_string())?;
            } else {
                let _ = index.remove_path(path);
            }
        }
        commit_index(&repo, &mut index)
    }

    pub fn working_diff(&self, space_slug: &str) -> Result<Vec<FileDiff>, String> {
        let space = self.find_space(space_slug)?;
        let repo = Repository::open(&space.local_path).map_err(|error| error.to_string())?;
        let mut options = StatusOptions::new();
        options
            .include_untracked(true)
            .recurse_untracked_dirs(true)
            .include_ignored(false);
        let statuses = repo
            .statuses(Some(&mut options))
            .map_err(|error| error.to_string())?;
        let mut result = Vec::new();
        for entry in statuses.iter() {
            let Some(relative) = entry.path() else {
                continue;
            };
            if Path::new(relative)
                .components()
                .any(|component| component.as_os_str().to_string_lossy().starts_with('.'))
            {
                continue;
            }
            let old_content = head_text(&repo, relative)?;
            let new_path = space.local_path.join(relative);
            let new_content = if new_path.is_file() {
                std::fs::read_to_string(&new_path).ok()
            } else {
                None
            };
            if old_content.is_none() && new_content.is_none() {
                continue;
            }
            result.push(text_file_diff(relative, old_content, new_content));
        }
        result.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(result)
    }

    fn repo(&self, space_slug: &str) -> Result<Repository, String> {
        let space = self.find_space(space_slug)?;
        Repository::open(&space.local_path).map_err(|e| e.to_string())
    }
}

fn reset_index_to_head(repo: &Repository, index: &mut git2::Index) -> Result<(), String> {
    if let Ok(head) = repo.head().and_then(|head| head.peel_to_commit()) {
        let tree = head.tree().map_err(|error| error.to_string())?;
        index.read_tree(&tree).map_err(|error| error.to_string())
    } else {
        index.clear().map_err(|error| error.to_string())
    }
}

fn commit_index(repo: &Repository, index: &mut git2::Index) -> Result<SubmitResult, String> {
    index.write().map_err(|error| error.to_string())?;
    let tree_id = index.write_tree().map_err(|error| error.to_string())?;
    let parent = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
    if parent
        .as_ref()
        .is_some_and(|commit| commit.tree_id() == tree_id)
    {
        return Ok(SubmitResult { committed: false });
    }
    let tree = repo.find_tree(tree_id).map_err(|error| error.to_string())?;
    let signature =
        Signature::now("CoWiki Local", "local@cowiki.app").map_err(|error| error.to_string())?;
    let parents: Vec<&git2::Commit<'_>> = parent.iter().collect();
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        "Update local Space",
        &tree,
        &parents,
    )
    .map_err(|error| error.to_string())?;
    Ok(SubmitResult { committed: true })
}

fn ui_search_hit(root: &Path, mut hit: SearchHit) -> Option<SearchHit> {
    let without_extension = hit.path.strip_suffix(".md").unwrap_or(&hit.path);
    let (section, slug) = if let Some(slug) = without_extension.strip_prefix("entities/") {
        ("entities", slug)
    } else if let Some(slug) = without_extension.strip_prefix("concepts/") {
        ("concepts", slug)
    } else if without_extension == "sources" || without_extension.starts_with("sources/") {
        return None;
    } else if root.join("wiki").is_dir() {
        ("wiki", without_extension.strip_prefix("wiki/")?)
    } else {
        ("wiki", without_extension)
    };
    hit.slug = slug.to_string();
    hit.path = format!("{section}/{slug}.md");
    Some(hit)
}

fn safe_repo_path(value: &str) -> Result<&Path, String> {
    let path = Path::new(value);
    if path.as_os_str().is_empty()
        || path.components().any(|component| {
            !matches!(component, Component::Normal(_))
                || component.as_os_str().to_string_lossy().starts_with('.')
        })
    {
        return Err("invalid reviewed path".to_string());
    }
    Ok(path)
}

fn head_text(repo: &Repository, relative: &str) -> Result<Option<String>, String> {
    let Ok(head) = repo.head() else {
        return Ok(None);
    };
    let commit = head.peel_to_commit().map_err(|error| error.to_string())?;
    let tree = commit.tree().map_err(|error| error.to_string())?;
    let Ok(entry) = tree.get_path(Path::new(relative)) else {
        return Ok(None);
    };
    let blob = repo
        .find_blob(entry.id())
        .map_err(|error| error.to_string())?;
    Ok(std::str::from_utf8(blob.content())
        .ok()
        .map(ToOwned::to_owned))
}

fn text_file_diff(
    path: &str,
    old_content: Option<String>,
    new_content: Option<String>,
) -> FileDiff {
    let old = old_content.as_deref().unwrap_or("");
    let new = new_content.as_deref().unwrap_or("");
    let diff = TextDiff::from_lines(old, new);
    let mut old_line = 1usize;
    let mut new_line = 1usize;
    let mut additions = 0usize;
    let mut deletions = 0usize;
    let mut lines = Vec::new();
    for change in diff.iter_all_changes() {
        let text = change.value().trim_end_matches(['\r', '\n']).to_string();
        match change.tag() {
            ChangeTag::Equal => {
                lines.push(DiffLine {
                    kind: "ctx".into(),
                    old_line: Some(old_line),
                    new_line: Some(new_line),
                    text,
                });
                old_line += 1;
                new_line += 1;
            }
            ChangeTag::Delete => {
                lines.push(DiffLine {
                    kind: "del".into(),
                    old_line: Some(old_line),
                    new_line: None,
                    text,
                });
                old_line += 1;
                deletions += 1;
            }
            ChangeTag::Insert => {
                lines.push(DiffLine {
                    kind: "add".into(),
                    old_line: None,
                    new_line: Some(new_line),
                    text,
                });
                new_line += 1;
                additions += 1;
            }
        }
    }
    FileDiff {
        path: path.to_string(),
        old_content,
        new_content,
        hunks: vec![DiffHunk {
            header: format!(
                "@@ -1,{} +1,{} @@",
                old_line.saturating_sub(1),
                new_line.saturating_sub(1)
            ),
            lines,
        }],
        additions,
        deletions,
    }
}

fn validate_slug(value: &str) -> Result<(), String> {
    if value.is_empty()
        || !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
    {
        return Err("slug may contain only letters, numbers, '-' and '_'".to_string());
    }
    Ok(())
}

fn content_path(root: &Path, dir: &str, slug: &str, markdown: bool) -> Result<PathBuf, String> {
    let base = section_root(root, dir)?;
    let relative = Path::new(slug);
    if relative.as_os_str().is_empty()
        || relative.components().any(|component| {
            !matches!(component, Component::Normal(_))
                || component.as_os_str().to_string_lossy().starts_with('.')
        })
    {
        return Err("invalid local content path".to_string());
    }
    let path = base.join(relative);
    Ok(if markdown {
        path.with_extension("md")
    } else {
        path
    })
}

fn section_root(root: &Path, dir: &str) -> Result<PathBuf, String> {
    match dir {
        // Legacy CoWiki repositories have an explicit wiki/ folder. An
        // arbitrary folder opened by the user is itself the Wiki root.
        "wiki" if root.join("wiki").is_dir() => Ok(root.join("wiki")),
        "wiki" => Ok(root.to_path_buf()),
        "entities" | "concepts" | "sources" => Ok(root.join(dir)),
        _ => Err(format!("unsupported local content directory: {dir}")),
    }
}

fn ui_path(root: &Path, value: &str) -> Result<PathBuf, String> {
    let relative = Path::new(value);
    if relative.as_os_str().is_empty()
        || relative.components().any(|component| {
            !matches!(component, Component::Normal(_))
                || component.as_os_str().to_string_lossy().starts_with('.')
        })
    {
        return Err("invalid local content path".to_string());
    }
    let mut components = relative.components();
    let section = components
        .next()
        .and_then(|component| component.as_os_str().to_str())
        .ok_or_else(|| "invalid local content path".to_string())?;
    let base = section_root(root, section)?;
    Ok(components.fold(base, |path, component| path.join(component.as_os_str())))
}

fn ensure_inside(root: &Path, path: &Path) -> Result<(), String> {
    if path.starts_with(root) {
        Ok(())
    } else {
        Err("path escapes the Space root".to_string())
    }
}

fn safe_component<'a>(value: &'a str, label: &str) -> Result<&'a str, String> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.starts_with('.')
        || matches!(trimmed, "." | "..")
        || trimmed.contains(['/', '\\'])
    {
        return Err(format!("invalid {label}"));
    }
    Ok(trimmed)
}

fn slugify_filename(value: &str) -> String {
    let mut slug = String::new();
    let mut separator = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
            separator = false;
        } else if !separator && !slug.is_empty() {
            slug.push('-');
            separator = true;
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "source".to_string()
    } else {
        slug.to_string()
    }
}

fn yaml_string(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', " ")
    )
}

fn read_page_tree(root: &Path, section: &str, current: &Path) -> Result<Vec<PageMeta>, String> {
    if !current.is_dir() {
        return Ok(vec![]);
    }
    let mut result = Vec::new();
    for entry in std::fs::read_dir(current).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.')
            || (current == root && ["entities", "concepts", "sources"].contains(&name.as_str()))
        {
            continue;
        }
        let relative = path.strip_prefix(root).map_err(|e| e.to_string())?;
        if path.is_dir() {
            let slug = relative.to_string_lossy().to_string();
            result.push(PageMeta {
                slug: slug.clone(),
                path: format!("{section}/{slug}"),
                title: name,
                summary: String::new(),
                branch: "local".into(),
                kind: "folder".into(),
                children: read_page_tree(root, section, &path)?,
            });
        } else if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        {
            let body = std::fs::read_to_string(&path).unwrap_or_default();
            let slug = relative.with_extension("").to_string_lossy().to_string();
            result.push(PageMeta {
                slug: slug.clone(),
                path: format!("{section}/{slug}.md"),
                title: markdown_title(&body).unwrap_or_else(|| {
                    path.file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string()
                }),
                summary: String::new(),
                branch: "local".into(),
                kind: "page".into(),
                children: vec![],
            });
        }
    }
    result.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
    });
    Ok(result)
}

pub(crate) fn markdown_title(body: &str) -> Option<String> {
    let mut in_frontmatter = false;
    for (index, line) in body.lines().enumerate() {
        if index == 0 && line.trim() == "---" {
            in_frontmatter = true;
            continue;
        }
        if in_frontmatter && line.trim() == "---" {
            in_frontmatter = false;
            continue;
        }
        if in_frontmatter {
            if let Some(value) = line.strip_prefix("title:") {
                return Some(value.trim().trim_matches(['\"', '\'']).to_string());
            }
        } else if let Some(value) = line.trim().strip_prefix("# ") {
            return Some(value.trim().to_string());
        }
    }
    None
}

fn title_case(value: &str) -> String {
    let mut chars = value.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::LocalEngine;

    #[test]
    fn fresh_local_engine_waits_for_a_folder() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(temp.path()).unwrap();

        assert!(engine.list_spaces().unwrap().is_empty());
    }

    #[test]
    fn save_stays_uncommitted_until_submit() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(temp.path()).unwrap();
        let folder = temp.path().join("Research Notes");
        std::fs::create_dir(&folder).unwrap();
        let space = engine
            .add_space("Research Notes", "research-notes", &folder)
            .unwrap();
        assert_eq!(space.local_path, folder.canonicalize().unwrap());
        let initial_commits = engine.commit_count(&space.slug).unwrap();

        engine
            .write_page(
                &space.slug,
                "wiki",
                "notes/first",
                "---\ntitle: \"First\"\n---\n\nLocal draft",
            )
            .unwrap();

        assert_eq!(engine.commit_count(&space.slug).unwrap(), initial_commits);
        assert!(engine.has_uncommitted_changes(&space.slug).unwrap());

        let result = engine
            .submit(&space.slug, &["notes/first.md".to_string()])
            .unwrap();
        assert!(result.committed);
        assert_eq!(
            engine.commit_count(&space.slug).unwrap(),
            initial_commits + 1
        );
        assert!(!engine.has_uncommitted_changes(&space.slug).unwrap());
    }

    #[test]
    fn imports_previously_opened_legacy_repositories_once() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let legacy = temp.path().join("cowiki");
        let repo = legacy.join("personal-device").join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git2::Repository::init(&repo).unwrap();

        assert_eq!(engine.import_legacy_spaces(&legacy).unwrap(), 1);
        assert_eq!(engine.import_legacy_spaces(&legacy).unwrap(), 0);
        let spaces = engine.list_spaces().unwrap();
        assert_eq!(spaces.len(), 1);
        assert_eq!(spaces[0].name, "My Space");
        assert_eq!(spaces[0].local_path, repo.canonicalize().unwrap());
    }

    #[test]
    fn local_file_operations_stay_inside_the_registered_space() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("notes");
        std::fs::create_dir_all(&folder).unwrap();
        let space = engine.add_space("Notes", "notes", &folder).unwrap();

        engine
            .create_folder(&space.slug, "Projects", Some("wiki"))
            .unwrap();
        engine
            .write_page(&space.slug, "wiki", "Projects/brief", "# Brief\n")
            .unwrap();
        engine
            .rename_path(
                &space.slug,
                "wiki/Projects/brief.md",
                "wiki/Projects/plan.md",
            )
            .unwrap();
        assert!(folder.join("Projects/plan.md").is_file());
        assert!(!folder.join("Projects/brief.md").exists());

        engine.delete_path(&space.slug, "wiki/Projects").unwrap();
        assert!(!folder.join("Projects").exists());
        assert!(engine
            .create_folder(&space.slug, "../escape", Some("wiki"))
            .is_err());
        assert!(engine.delete_path(&space.slug, "wiki/../outside").is_err());
    }

    #[test]
    fn checked_save_never_overwrites_external_agent_edits() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("notes");
        std::fs::create_dir_all(&folder).unwrap();
        let space = engine.add_space("Notes", "notes", &folder).unwrap();
        engine
            .write_page(&space.slug, "wiki", "shared", "# Shared\n\nHuman base")
            .unwrap();

        std::fs::write(folder.join("shared.md"), "# Shared\n\nAgent result").unwrap();
        let error = engine
            .write_page_checked(
                &space.slug,
                "wiki",
                "shared",
                "# Shared\n\nHuman newer edit",
                Some("# Shared\n\nHuman base"),
                false,
            )
            .unwrap_err();
        assert!(error.contains("changed outside CoWiki"));
        assert_eq!(
            std::fs::read_to_string(folder.join("shared.md")).unwrap(),
            "# Shared\n\nAgent result"
        );
    }

    #[test]
    fn create_only_page_write_refuses_to_replace_an_existing_page() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("notes");
        std::fs::create_dir_all(&folder).unwrap();
        let space = engine.add_space("Notes", "notes", &folder).unwrap();
        engine
            .write_page(&space.slug, "wiki", "same", "# Original")
            .unwrap();

        assert!(engine
            .write_page_checked(&space.slug, "wiki", "same", "# Replacement", None, true)
            .is_err());
        assert_eq!(
            std::fs::read_to_string(folder.join("same.md")).unwrap(),
            "# Original"
        );
    }

    #[test]
    fn working_diff_exposes_local_changes_for_review() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("notes");
        std::fs::create_dir_all(&folder).unwrap();
        let space = engine.add_space("Notes", "notes", &folder).unwrap();
        engine
            .write_page(&space.slug, "wiki", "draft", "# Draft\n\nAgent proposal\n")
            .unwrap();

        let diffs = engine.working_diff(&space.slug).unwrap();
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].path, "draft.md");
        assert!(diffs[0].old_content.is_none());
        assert!(diffs[0]
            .new_content
            .as_deref()
            .unwrap()
            .contains("Agent proposal"));
        assert!(diffs[0].additions > 0);

        engine
            .submit(&space.slug, &["draft.md".to_string()])
            .unwrap();
        assert!(engine.working_diff(&space.slug).unwrap().is_empty());
    }

    #[test]
    fn keep_rejects_changes_that_arrived_after_review_opened() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("notes");
        std::fs::create_dir_all(&folder).unwrap();
        let space = engine.add_space("Notes", "notes", &folder).unwrap();
        std::fs::write(folder.join("proposal.md"), "# First\n").unwrap();
        let reviewed = engine.working_diff(&space.slug).unwrap();

        std::fs::write(folder.join("proposal.md"), "# Changed after review\n").unwrap();
        let error = engine
            .keep_working_diff(&space.slug, &reviewed)
            .unwrap_err();
        assert!(error.contains("changed after this Review opened"));
        assert_eq!(engine.commit_count(&space.slug).unwrap(), 0);
    }

    #[test]
    fn ui_search_maps_root_pages_to_wiki_and_hides_sources() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("notes");
        std::fs::create_dir_all(folder.join("sources")).unwrap();
        std::fs::write(folder.join("guide.md"), "# Offline Guide\n").unwrap();
        std::fs::write(folder.join("sources/raw.md"), "# Offline Raw Source\n").unwrap();
        let space = engine.add_space("Notes", "notes", &folder).unwrap();

        let response = engine.search(&space.slug, "offline", 10).unwrap();
        assert_eq!(response.keyword.len(), 1);
        assert_eq!(response.keyword[0].path, "wiki/guide.md");
        assert_eq!(response.keyword[0].slug, "guide");
    }

    #[test]
    fn local_ingest_writes_an_okf_source_and_indexes_it() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("knowledge");
        std::fs::create_dir_all(&folder).unwrap();
        let space = engine.add_space("Knowledge", "knowledge", &folder).unwrap();

        let item = engine
            .ingest(
                &space.slug,
                "text",
                "A local-first database keeps working offline.",
                Some("Research Note"),
            )
            .unwrap();
        assert_eq!(item.filename, "research-note.md");
        let body = std::fs::read_to_string(folder.join("sources/research-note.md")).unwrap();
        assert!(body.contains("type: Source"));
        assert!(body.contains("keeps working offline"));
        assert_eq!(
            engine
                .search_pages(&space.slug, "working offline", 10)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn search_index_is_rebuilt_from_markdown_and_ignores_dot_files() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("knowledge");
        std::fs::create_dir_all(folder.join("concepts")).unwrap();
        std::fs::create_dir_all(folder.join(".private")).unwrap();
        std::fs::write(
            folder.join("concepts/local-first.md"),
            "---\ntitle: Local-first software\n---\n\nThe network is optional.",
        )
        .unwrap();
        std::fs::write(
            folder.join(".private/secret.md"),
            "---\ntitle: Secret roadmap\n---\n\nNever index this.",
        )
        .unwrap();
        let space = engine.add_space("Knowledge", "knowledge", &folder).unwrap();

        let hits = engine
            .search_pages(&space.slug, "network optional", 10)
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "concepts/local-first.md");
        assert_eq!(hits[0].title, "Local-first software");
        assert!(hits[0].snippet.contains("network"));
        assert!(!hits[0].title_match);
        assert!(engine
            .search_pages(&space.slug, "secret", 10)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn title_matches_rank_before_body_matches_and_saved_pages_update_incrementally() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("notes");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("body.md"), "# Other\n\nOrchid handbook").unwrap();
        std::fs::write(folder.join("title.md"), "# Orchid\n\nA flower").unwrap();
        let space = engine.add_space("Notes", "notes", &folder).unwrap();

        let before = engine.search_pages(&space.slug, "orchid", 10).unwrap();
        assert_eq!(before[0].path, "title.md");
        assert!(before[0].title_match);

        engine
            .write_page(
                &space.slug,
                "wiki",
                "new-page",
                "# New Page\n\nOrchid field notes",
            )
            .unwrap();
        let after = engine.search_pages(&space.slug, "field notes", 10).unwrap();
        assert_eq!(after[0].path, "new-page.md");
    }

    #[test]
    fn backlinks_are_derived_from_wikilinks_and_rebuilt_after_external_edits() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("wiki");
        std::fs::create_dir_all(folder.join("people")).unwrap();
        std::fs::write(folder.join("alice.md"), "# Alice").unwrap();
        std::fs::write(
            folder.join("people/project.md"),
            "# Project\n\nOwner: [[alice|Alice]]. Also [[alice#Background]].",
        )
        .unwrap();
        let space = engine.add_space("Wiki", "wiki", &folder).unwrap();

        let links = engine.list_backlinks(&space.slug, "alice.md").unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].path, "people/project.md");

        std::fs::write(
            folder.join("people/project.md"),
            "# Project\n\nNo links now.",
        )
        .unwrap();
        engine.rebuild_search_index(&space.slug).unwrap();
        assert!(engine
            .list_backlinks(&space.slug, "alice.md")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn search_refreshes_files_changed_directly_by_an_agent() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("agent-space");
        std::fs::create_dir_all(&folder).unwrap();
        let space = engine.add_space("Agent", "agent", &folder).unwrap();

        std::fs::write(
            folder.join("agent-note.md"),
            "# Agent note\n\nA newly discovered capybara fact.",
        )
        .unwrap();

        let hits = engine.search_pages(&space.slug, "capybara", 5).unwrap();
        assert_eq!(hits[0].path, "agent-note.md");
    }

    #[test]
    fn an_invalid_utf8_markdown_file_does_not_block_the_space() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("mixed-files");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            folder.join("valid.md"),
            "# Valid\n\nSearchable platypus note.",
        )
        .unwrap();
        std::fs::write(folder.join("invalid.md"), [0xff, 0xfe, 0xfd]).unwrap();

        let space = engine.add_space("Mixed", "mixed", &folder).unwrap();
        let hits = engine.search_pages(&space.slug, "platypus", 5).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "valid.md");
    }
}
