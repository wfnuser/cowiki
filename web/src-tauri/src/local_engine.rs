use git2::build::CheckoutBuilder;
use git2::{DiffOptions, IndexEntry, IndexTime, Oid, Repository, Signature, StatusOptions};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use similar::{ChangeTag, TextDiff};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use crate::knowledge_index::{self, SearchHit};
use crate::okf::{self, DocumentKind};

mod agent_changes;
pub use agent_changes::AgentChange;

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
#[serde(rename_all = "camelCase")]
pub struct Checkpoint {
    pub id: String,
    pub name: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DraftStatus {
    pub changed_files: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SpaceHistory {
    pub current_draft: DraftStatus,
    pub checkpoints: Vec<Checkpoint>,
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

/// One file's result from a batch `ingest_files` call. A batch import keeps
/// going past a single bad file (an unsupported format, a locked file)
/// rather than failing everything the user selected because of one file.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IngestFileOutcome {
    pub source_path: String,
    pub source: Option<SourceItem>,
    pub error: Option<String>,
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
    pub is_binary: bool,
    pub old_binary_hash: Option<String>,
    pub new_binary_hash: Option<String>,
    pub hunks: Vec<DiffHunk>,
    pub additions: usize,
    pub deletions: usize,
}

#[derive(Clone)]
pub struct LocalEngine {
    db: Arc<Mutex<Connection>>,
    metadata_dir: PathBuf,
    // Product-owned Draft writers serialize here. External filesystem writers
    // cannot join this protocol, so merge still performs its final no-follow
    // content CAS immediately before each replacement.
    mutation_lock: Arc<Mutex<()>>,
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
            db: Arc::new(Mutex::new(connection)),
            metadata_dir: metadata_dir.to_path_buf(),
            mutation_lock: Arc::new(Mutex::new(())),
        };
        engine.rebuild_all_search_indexes()?;
        Ok(engine)
    }

    pub(crate) fn lock_mutations(&self) -> Result<MutexGuard<'_, ()>, String> {
        self.mutation_lock
            .lock()
            .map_err(|_| "local mutation lock is unavailable".to_string())
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
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    eprintln!("CoWiki skipped an unreadable legacy Space entry: {error}");
                    continue;
                }
            };
            let legacy_path = entry.path();
            match self.import_legacy_space(&legacy_path) {
                Ok(true) => imported += 1,
                Ok(false) => {}
                Err(error) => eprintln!(
                    "CoWiki skipped legacy Space '{}': {error}",
                    legacy_path.display()
                ),
            }
        }
        Ok(imported)
    }

    fn import_legacy_space(&self, legacy_path: &Path) -> Result<bool, String> {
        let repo = legacy_path.join("repo");
        if !repo.join(".git").exists() {
            return Ok(false);
        }
        let repo = repo.canonicalize().map_err(|e| e.to_string())?;
        if self.space_by_path(&repo)?.is_some() {
            return Ok(false);
        }

        let slug = legacy_path
            .file_name()
            .ok_or_else(|| "legacy Space directory has no name".to_string())?
            .to_string_lossy()
            .to_string();
        let git = Repository::open(&repo).map_err(|error| error.to_string())?;
        prepare_okf_repository(&git, &repo)?;
        let name = if slug.starts_with("personal-") {
            "My Space".to_string()
        } else if slug.starts_with("general-") {
            "General".to_string()
        } else {
            slug.replace(['-', '_'], " ")
        };
        let space = self.insert_space(&name, &slug, &repo)?;
        if let Err(error) = self.rebuild_search_index(&space.slug) {
            eprintln!(
                "CoWiki imported legacy Space '{}' without its search index: {error}",
                space.slug
            );
        }
        Ok(true)
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
        let repo = Repository::open(&local_path)
            .or_else(|_| Repository::init(&local_path))
            .map_err(|e| format!("cannot initialize local Git repository: {e}"))?;
        prepare_okf_repository(&repo, &local_path)?;

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

    #[cfg(test)]
    pub fn write_page(
        &self,
        space_slug: &str,
        page_slug: &str,
        content: &str,
    ) -> Result<(), String> {
        self.write_page_checked(space_slug, page_slug, content, None, false)
    }

    pub fn write_page_checked(
        &self,
        space_slug: &str,
        page_slug: &str,
        content: &str,
        expected_content: Option<&str>,
        create_only: bool,
    ) -> Result<(), String> {
        let space = self.find_space(space_slug)?;
        okf::ensure_supported_for_write(&space.local_path)?;
        let relative = okf::concept_relative_path(page_slug)?;
        let path = checked_space_path(&space.local_path, &relative)?;
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
        let fallback_title = relative.file_stem().unwrap_or_default().to_string_lossy();
        let normalized = okf::normalize_concept_document(content, &fallback_title)?;
        std::fs::write(&temporary, normalized).map_err(|e| e.to_string())?;
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
        okf::refresh_progressive_indexes(&space.local_path)?;
        let mut db = self
            .db
            .lock()
            .map_err(|_| "local database lock poisoned".to_string())?;
        knowledge_index::rebuild_space(&mut db, &space.id, &space.local_path)
    }

    pub fn create_folder(
        &self,
        space_slug: &str,
        name: &str,
        parent: Option<&str>,
    ) -> Result<(), String> {
        let space = self.find_space(space_slug)?;
        okf::ensure_supported_for_write(&space.local_path)?;
        let clean_name = safe_component(name, "folder name")?;
        let parent_path = match parent {
            Some(parent) => ui_path(&space.local_path, parent)?,
            None => space.local_path.clone(),
        };
        let folder = parent_path.join(clean_name);
        ensure_inside(&space.local_path, &folder)?;
        std::fs::create_dir(&folder).map_err(|error| format!("cannot create folder: {error}"))?;
        std::fs::write(folder.join("index.md"), okf::folder_index(clean_name))
            .map_err(|error| error.to_string())?;
        okf::refresh_progressive_indexes(&space.local_path)?;
        self.rebuild_search_index(space_slug)
    }

    pub fn ingest(
        &self,
        space_slug: &str,
        source_type: &str,
        content: &str,
        filename: Option<&str>,
    ) -> Result<SourceItem, String> {
        let space = self.find_space(space_slug)?;
        okf::ensure_supported_for_write(&space.local_path)?;
        let fallback = match source_type {
            "url" => url::Url::parse(content.trim())
                .ok()
                .and_then(|value| value.host_str().map(ToOwned::to_owned))
                .unwrap_or_else(|| "web-source".to_string()),
            _ => "source".to_string(),
        };
        let requested = filename.unwrap_or(&fallback);
        let title = if source_type == "url" {
            content.trim()
        } else {
            requested.trim()
        };
        let source = self.write_source_document(&space, requested, title, content.trim(), None)?;
        self.refresh_source_views(&space)?;
        Ok(source)
    }

    /// Import files from disk, extracting text from any binary format
    /// `extract::read_source_file` recognizes. Keeps going past a single
    /// file's failure — an unsupported format or an unreadable file in a
    /// multi-select should not block the rest of the batch.
    pub fn ingest_files(
        &self,
        space_slug: &str,
        source_paths: &[String],
    ) -> Result<Vec<IngestFileOutcome>, String> {
        let space = self.find_space(space_slug)?;
        okf::ensure_supported_for_write(&space.local_path)?;
        let outcomes = source_paths
            .iter()
            .map(
                |source_path| match self.ingest_one_file(&space, source_path) {
                    Ok(source) => IngestFileOutcome {
                        source_path: source_path.clone(),
                        source: Some(source),
                        error: None,
                    },
                    Err(error) => IngestFileOutcome {
                        source_path: source_path.clone(),
                        source: None,
                        error: Some(error),
                    },
                },
            )
            .collect::<Vec<_>>();
        if outcomes.iter().any(|outcome| outcome.source.is_some()) {
            self.refresh_source_views(&space)?;
        }
        Ok(outcomes)
    }

    fn ingest_one_file(&self, space: &Space, source_path: &str) -> Result<SourceItem, String> {
        let path = Path::new(source_path);
        if !crate::extract::is_supported(path) {
            // Fail before reading the file at all — no point hashing a
            // potentially large file CoWiki already knows it can't parse.
            return Err(format!(
                "'{source_path}' is not a supported source format (supported: {})",
                crate::extract::all_supported_extensions().join(", ")
            ));
        }
        let text = crate::extract::read_source_file(path)?;
        let content_hash = sha256_file(path)?;
        let original_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| format!("'{source_path}' has no usable file name"))?;
        let title = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or(original_name);
        self.write_source_document(space, original_name, title, &text, Some(&content_hash))
    }

    /// Shared write path for every ingest flavor: pasted text, a URL
    /// placeholder, or text extracted from an imported file.
    ///
    /// A file re-imported under its original name is common (re-syncing a
    /// folder, clicking Import twice by accident) and should update the
    /// same Source rather than pile up copies — so when `content_hash` is
    /// given and matches what's already on disk, this returns the existing
    /// Source without writing so human or Agent annotations survive. A name
    /// collision with genuinely different content gets a hash-derived (not
    /// random) suffix, so re-importing *that* file is still idempotent instead
    /// of spawning a new copy every time. Pasted
    /// text/URLs (`content_hash: None`) keep the original never-clobber
    /// behavior, since two unrelated pastes can legitimately share a title.
    fn write_source_document(
        &self,
        space: &Space,
        requested_filename: &str,
        title: &str,
        body_text: &str,
        content_hash: Option<&str>,
    ) -> Result<SourceItem, String> {
        let relative = okf::source_storage_path(requested_filename)?;
        let mut candidate = checked_space_path(&space.local_path, &relative)?;
        if candidate.exists() {
            if content_hash.is_some_and(|hash| source_hash_matches(&candidate, hash)) {
                return source_item_for_path(space, &candidate);
            }

            let stem = candidate
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            if let Some(hash) = content_hash {
                let mut found = None;
                for suffix_len in [8, 16, 32, hash.len()] {
                    let suffix = hash.get(..suffix_len).unwrap_or(hash);
                    let next = candidate.with_file_name(format!("{stem}-{suffix}.md"));
                    if !next.exists() {
                        found = Some(next);
                        break;
                    }
                    if source_hash_matches(&next, hash) {
                        return source_item_for_path(space, &next);
                    }
                }
                candidate = found.unwrap_or_else(|| loop {
                    let suffix = &uuid::Uuid::new_v4().simple().to_string()[..8];
                    let next = candidate.with_file_name(format!("{stem}-{hash}-{suffix}.md"));
                    if !next.exists() {
                        break next;
                    }
                });
            } else {
                candidate = loop {
                    let id = uuid::Uuid::new_v4().simple().to_string();
                    let next = candidate.with_file_name(format!("{stem}-{}.md", &id[..8]));
                    if !next.exists() {
                        break next;
                    }
                };
            }
        }
        ensure_inside(&space.local_path, &candidate)?;
        if let Some(parent) = candidate.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }

        let mut frontmatter = format!("title: {}\ntype: Source\n", yaml_string(title));
        if let Some(hash) = content_hash {
            frontmatter.push_str(&format!("source_hash: {}\n", yaml_string(hash)));
        }
        let body = format!("---\n{frontmatter}---\n\n{}\n", body_text.trim());
        std::fs::write(&candidate, body).map_err(|error| error.to_string())?;
        source_item_for_path(space, &candidate)
    }

    fn refresh_source_views(&self, space: &Space) -> Result<(), String> {
        okf::refresh_progressive_indexes(&space.local_path)?;
        let mut db = self
            .db
            .lock()
            .map_err(|_| "local database lock poisoned".to_string())?;
        knowledge_index::refresh_space(&mut db, &space.id, &space.local_path)
    }

    pub fn rename_path(&self, space_slug: &str, from: &str, to: &str) -> Result<(), String> {
        let space = self.find_space(space_slug)?;
        okf::ensure_supported_for_write(&space.local_path)?;
        let from = ui_path(&space.local_path, from)?;
        let to = ui_path(&space.local_path, to)?;
        ensure_inside(&space.local_path, &from)?;
        ensure_inside(&space.local_path, &to)?;
        if from.is_file()
            && (DocumentKind::from_path(&from) != DocumentKind::Concept
                || DocumentKind::from_path(&to) != DocumentKind::Concept)
        {
            return Err("index.md and log.md are reserved by OKF".to_string());
        }
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
        okf::refresh_progressive_indexes(&space.local_path)?;
        self.rebuild_search_index(space_slug)
    }

    pub fn delete_path(&self, space_slug: &str, value: &str) -> Result<(), String> {
        let space = self.find_space(space_slug)?;
        okf::ensure_supported_for_write(&space.local_path)?;
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
        okf::refresh_progressive_indexes(&space.local_path)?;
        self.rebuild_search_index(space_slug)
    }

    pub fn list_pages(&self, space_slug: &str) -> Result<Vec<PageMeta>, String> {
        let space = self.find_space(space_slug)?;
        read_page_tree(&space.local_path, &space.local_path)
    }

    pub fn get_page(&self, space_slug: &str, slug: &str) -> Result<PageFull, String> {
        let space = self.find_space(space_slug)?;
        let relative = okf::concept_relative_path(slug)?;
        let path = checked_space_path(&space.local_path, &relative)?;
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
                path: normalize_path(&relative),
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
        let root = space.local_path.join(okf::RAW_SOURCES_DIR);
        if !root.is_dir() {
            return Ok(vec![]);
        }
        let mut sources = Vec::new();
        for entry in walkdir::WalkDir::new(&root)
            .into_iter()
            .filter_map(Result::ok)
        {
            if entry.file_type().is_file()
                && entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
            {
                sources.push(SourceItem {
                    filename: entry
                        .path()
                        .strip_prefix(&root)
                        .map(normalize_path)
                        .unwrap_or_default(),
                });
            }
        }
        Ok(sources)
    }

    pub fn get_source(&self, space_slug: &str, filename: &str) -> Result<SourceContent, String> {
        let space = self.find_space(space_slug)?;
        let relative = safe_source_relative_path(filename)?;
        let path = checked_space_path(
            &space.local_path,
            &Path::new(okf::RAW_SOURCES_DIR).join(relative),
        )?;
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
                match Repository::open(&space.local_path) {
                    Ok(repo) => {
                        if let Err(error) = prepare_okf_repository(&repo, &space.local_path) {
                            eprintln!(
                                "CoWiki skipped startup migration for Space '{}': {error}",
                                space.slug
                            );
                        }
                    }
                    Err(error) => {
                        eprintln!(
                            "CoWiki could not open the Git repository for Space '{}': {error}",
                            space.slug
                        );
                    }
                }
                let mut db = self
                    .db
                    .lock()
                    .map_err(|_| "local database lock poisoned".to_string())?;
                if let Err(error) =
                    knowledge_index::rebuild_space(&mut db, &space.id, &space.local_path)
                {
                    eprintln!(
                        "CoWiki skipped startup indexing for Space '{}': {error}",
                        space.slug
                    );
                }
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
        let relative = safe_knowledge_relative_path(relative_path)?;
        let path = checked_space_path(&space.local_path, relative)?;
        let body = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let title = markdown_title(&body).unwrap_or_else(|| {
            path.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        });
        let normalized = normalize_path(relative);
        let slug = normalized
            .strip_suffix(".md")
            .unwrap_or(&normalized)
            .to_string();
        let kind = match DocumentKind::from_path(relative) {
            DocumentKind::Index => "index",
            DocumentKind::Log => "log",
            DocumentKind::Concept if normalized.starts_with(".cowiki/sources/") => "source",
            _ => "page",
        };
        Ok(PageFull {
            meta: PageMeta {
                slug,
                path: normalized,
                title,
                summary: String::new(),
                branch: "local".to_string(),
                kind: kind.to_string(),
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

    pub fn history(&self, space_slug: &str) -> Result<SpaceHistory, String> {
        let repo = self.repo(space_slug)?;
        let changed_files = draft_changed_files(&repo)?;
        let mut checkpoints = Vec::new();
        let mut current = checkpoint_tip(&repo)?;

        while let Some(oid) = current {
            if repo.find_reference(&checkpoint_reference(oid)).is_err() {
                break;
            }
            let commit = repo.find_commit(oid).map_err(|error| error.to_string())?;
            checkpoints.push(Checkpoint {
                id: oid.to_string(),
                name: commit.summary().unwrap_or("Checkpoint").to_string(),
                created_at: commit.time().seconds(),
            });
            current = if commit.parent_count() == 0 {
                None
            } else {
                Some(commit.parent_id(0).map_err(|error| error.to_string())?)
            };
        }

        Ok(SpaceHistory {
            current_draft: DraftStatus { changed_files },
            checkpoints,
        })
    }

    pub fn create_checkpoint(
        &self,
        space_slug: &str,
        name: Option<&str>,
    ) -> Result<Checkpoint, String> {
        let repo = self.repo(space_slug)?;
        let root = repo
            .workdir()
            .ok_or_else(|| "local Space repository has no working directory".to_string())?;
        let mut snapshot = repo.index().map_err(|error| error.to_string())?;
        reset_index_to_head(&repo, &mut snapshot)?;
        for relative in draft_statuses(&repo)? {
            let path = Path::new(&relative);
            if root.join(path).symlink_metadata().is_ok() {
                snapshot.add_path(path).map_err(|error| error.to_string())?;
            } else {
                let _ = snapshot.remove_path(path);
            }
        }
        let tree_id = snapshot.write_tree().map_err(|error| error.to_string())?;
        let tree = repo.find_tree(tree_id).map_err(|error| error.to_string())?;
        let parent_oid = checkpoint_tip(&repo)?.or_else(|| {
            repo.head()
                .ok()
                .and_then(|head| head.peel_to_commit().ok())
                .map(|commit| commit.id())
        });
        let parent = parent_oid
            .map(|oid| repo.find_commit(oid).map_err(|error| error.to_string()))
            .transpose()?;
        let parents = parent.iter().collect::<Vec<_>>();
        let signature = Signature::now("CoWiki Checkpoint", "checkpoint@cowiki.app")
            .map_err(|error| error.to_string())?;
        let label = name
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("Checkpoint {}", signature.when().seconds()));
        let oid = repo
            .commit(None, &signature, &signature, &label, &tree, &parents)
            .map_err(|error| error.to_string())?;
        let immutable_ref = checkpoint_reference(oid);
        repo.reference(&immutable_ref, oid, false, "Create CoWiki checkpoint")
            .map_err(|error| error.to_string())?;
        if let Err(error) = repo.reference(
            CHECKPOINT_TIP_REF,
            oid,
            true,
            "Advance CoWiki checkpoint history",
        ) {
            if let Ok(mut reference) = repo.find_reference(&immutable_ref) {
                let _ = reference.delete();
            }
            return Err(error.to_string());
        }

        Ok(Checkpoint {
            id: oid.to_string(),
            name: label,
            created_at: signature.when().seconds(),
        })
    }

    pub fn submit(&self, space_slug: &str, paths: &[String]) -> Result<SubmitResult, String> {
        let repo = self.repo(space_slug)?;
        let root = repo
            .workdir()
            .ok_or_else(|| "local Space repository has no working directory".to_string())?;
        okf::ensure_supported_for_write(root)?;
        let backup = create_migration_backup(&repo, root)?;
        if let Err(error) = okf::ensure_bundle(root) {
            rollback_migration(&repo, root, &backup)?;
            return Err(error);
        }
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
        stage_all_okf_changes(&repo, &mut index)?;
        index.write().map_err(|e| e.to_string())?;
        let tree_id = index.write_tree().map_err(|e| e.to_string())?;

        let parent = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
        if parent
            .as_ref()
            .is_some_and(|commit| commit.tree_id() == tree_id)
        {
            delete_migration_backup(&repo, &backup.reference)?;
            return Ok(SubmitResult { committed: false });
        }

        let tree = repo.find_tree(tree_id).map_err(|e| e.to_string())?;
        let signature =
            Signature::now("CoWiki Local", "local@cowiki.app").map_err(|e| e.to_string())?;
        let parents: Vec<&git2::Commit<'_>> = parent.iter().collect();
        if let Err(error) = repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            "Update local Space",
            &tree,
            &parents,
        ) {
            rollback_migration(&repo, root, &backup)?;
            return Err(error.to_string());
        }
        delete_migration_backup(&repo, &backup.reference)?;
        Ok(SubmitResult { committed: true })
    }

    pub fn keep_working_diff(
        &self,
        space_slug: &str,
        expected: &[FileDiff],
    ) -> Result<SubmitResult, String> {
        let repo = self.repo(space_slug)?;
        let root = repo
            .workdir()
            .ok_or_else(|| "local Space repository has no working directory".to_string())?;
        okf::ensure_supported_for_write(root)?;
        let current = self.working_diff(space_slug)?;
        if current != expected {
            return Err(
                "Local files changed after this Review opened. Refresh Reviews before keeping changes."
                    .to_string(),
            );
        }
        let mut index = repo.index().map_err(|error| error.to_string())?;
        reset_index_to_head(&repo, &mut index)?;
        for diff in expected {
            let path = safe_repo_path(&diff.path)?;
            if diff.is_binary {
                if diff.new_binary_hash.is_some() {
                    index.add_path(path).map_err(|error| error.to_string())?;
                } else {
                    let _ = index.remove_path(path);
                }
                continue;
            }
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
        stage_deterministic_okf_artifacts(&repo, &mut index)?;
        commit_index(&repo, &mut index)
    }

    pub fn working_diff(&self, space_slug: &str) -> Result<Vec<FileDiff>, String> {
        let space = self.find_space(space_slug)?;
        let repo = Repository::open(&space.local_path).map_err(|error| error.to_string())?;
        if okf::ensure_supported_for_write(&space.local_path).is_ok() {
            let backup = create_migration_backup(&repo, &space.local_path)?;
            if let Err(error) = okf::ensure_bundle(&space.local_path) {
                rollback_migration(&repo, &space.local_path, &backup)?;
                return Err(error);
            }
            delete_migration_backup(&repo, &backup.reference)?;
        }
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
            if safe_repo_path(relative).is_err() {
                continue;
            }
            let old_bytes = head_bytes(&repo, relative)?;
            let new_path = space.local_path.join(relative);
            let new_bytes = if new_path.is_file() {
                std::fs::read(&new_path).ok()
            } else {
                None
            };
            if old_bytes.is_none() && new_bytes.is_none() {
                continue;
            }
            result.push(file_diff_from_bytes(relative, old_bytes, new_bytes));
        }
        result.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(result)
    }

    fn repo(&self, space_slug: &str) -> Result<Repository, String> {
        let space = self.find_space(space_slug)?;
        Repository::open(&space.local_path).map_err(|e| e.to_string())
    }
}

const CHECKPOINT_TIP_REF: &str = "refs/cowiki/checkpoints/latest";

fn checkpoint_reference(oid: Oid) -> String {
    format!("refs/cowiki/checkpoints/{oid}")
}

fn checkpoint_tip(repo: &Repository) -> Result<Option<Oid>, String> {
    match repo.find_reference(CHECKPOINT_TIP_REF) {
        Ok(reference) => reference
            .target()
            .map(Some)
            .ok_or_else(|| "CoWiki checkpoint history points to a symbolic ref".to_string()),
        Err(error) if error.code() == git2::ErrorCode::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

fn draft_statuses(repo: &Repository) -> Result<Vec<String>, String> {
    let mut options = StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false);
    let statuses = repo
        .statuses(Some(&mut options))
        .map_err(|error| error.to_string())?;
    Ok(statuses
        .iter()
        .filter_map(|entry| entry.path().map(str::to_string))
        .filter(|relative| safe_repo_path(relative).is_ok())
        .collect())
}

fn draft_changed_files(repo: &Repository) -> Result<usize, String> {
    let baseline_oid = checkpoint_tip(repo)?.or_else(|| {
        repo.head()
            .ok()
            .and_then(|head| head.peel_to_commit().ok())
            .map(|commit| commit.id())
    });
    let baseline = baseline_oid
        .map(|oid| {
            repo.find_commit(oid)
                .and_then(|commit| commit.tree())
                .map_err(|error| error.to_string())
        })
        .transpose()?;
    let mut options = DiffOptions::new();
    options.include_untracked(true).recurse_untracked_dirs(true);
    let diff = repo
        .diff_tree_to_workdir(baseline.as_ref(), Some(&mut options))
        .map_err(|error| error.to_string())?;
    Ok(diff.deltas().count())
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

fn stage_all_okf_changes(repo: &Repository, index: &mut git2::Index) -> Result<(), String> {
    let root = repo
        .workdir()
        .ok_or_else(|| "local Space repository has no working directory".to_string())?;
    let mut options = StatusOptions::new();
    options.include_untracked(true).recurse_untracked_dirs(true);
    for status in repo
        .statuses(Some(&mut options))
        .map_err(|error| error.to_string())?
        .iter()
    {
        let Some(relative) = status.path() else {
            continue;
        };
        let markdown = DocumentKind::from_path(Path::new(relative)) != DocumentKind::Other;
        let controlled_artifact = relative.starts_with(".cowiki/legacy/");
        if (!markdown || safe_repo_path(relative).is_err()) && !controlled_artifact {
            continue;
        }
        let path = Path::new(relative);
        if root.join(path).is_file() {
            index.add_path(path).map_err(|error| error.to_string())?;
        } else {
            let _ = index.remove_path(path);
        }
    }
    Ok(())
}

fn stage_deterministic_okf_artifacts(
    repo: &Repository,
    index: &mut git2::Index,
) -> Result<(), String> {
    let root = repo
        .workdir()
        .ok_or_else(|| "local Space repository has no working directory".to_string())?;
    let mut options = StatusOptions::new();
    options.include_untracked(true).recurse_untracked_dirs(true);
    for status in repo
        .statuses(Some(&mut options))
        .map_err(|error| error.to_string())?
        .iter()
    {
        let Some(relative) = status.path() else {
            continue;
        };
        if DocumentKind::from_path(Path::new(relative)) != DocumentKind::Index {
            continue;
        }
        let path = Path::new(relative);
        if root.join(path).is_file() {
            index.add_path(path).map_err(|error| error.to_string())?;
        } else {
            let _ = index.remove_path(path);
        }
    }
    for entry in walkdir::WalkDir::new(root.join(".cowiki/legacy"))
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let relative = entry
            .path()
            .strip_prefix(root)
            .map_err(|error| error.to_string())?;
        index
            .add_path(relative)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn ui_search_hit(_root: &Path, mut hit: SearchHit) -> Option<SearchHit> {
    let without_extension = hit.path.strip_suffix(".md").unwrap_or(&hit.path);
    if without_extension == "index"
        || without_extension == "log"
        || without_extension.ends_with("/index")
        || without_extension.ends_with("/log")
        || without_extension.starts_with(".cowiki/sources/")
    {
        return None;
    }
    hit.slug = without_extension.to_string();
    Some(hit)
}

fn safe_repo_path(value: &str) -> Result<&Path, String> {
    let path = Path::new(value);
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err("invalid reviewed path".to_string());
    }
    let parts = path
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>();
    let controlled_source = parts.first().is_some_and(|part| part == ".cowiki")
        && parts.get(1).is_some_and(|part| part == "sources")
        && parts.len() > 2
        && parts[2..].iter().all(|part| !part.starts_with('.'));
    if parts.iter().any(|part| part.starts_with('.')) && !controlled_source {
        return Err("invalid reviewed path".to_string());
    }
    Ok(path)
}

fn commit_initial_okf_indexes(repo: &Repository, root: &Path) -> Result<(), String> {
    if repo.head().is_ok() {
        return Ok(());
    }
    let mut index = repo.index().map_err(|error| error.to_string())?;
    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| {
            entry.path() == root
                || !entry
                    .path()
                    .strip_prefix(root)
                    .unwrap_or(entry.path())
                    .components()
                    .any(|component| component.as_os_str().to_string_lossy().starts_with('.'))
        })
        .filter_map(Result::ok)
    {
        if entry.file_type().is_file() && entry.file_name() == "index.md" {
            let relative = entry
                .path()
                .strip_prefix(root)
                .map_err(|error| error.to_string())?;
            index
                .add_path(relative)
                .map_err(|error| error.to_string())?;
        }
    }
    index.write().map_err(|error| error.to_string())?;
    let tree_id = index.write_tree().map_err(|error| error.to_string())?;
    let tree = repo.find_tree(tree_id).map_err(|error| error.to_string())?;
    let signature =
        Signature::now("CoWiki Local", "local@cowiki.app").map_err(|error| error.to_string())?;
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        "Initialize OKF Space",
        &tree,
        &[],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn prepare_okf_repository(repo: &Repository, root: &Path) -> Result<(), String> {
    if !okf::needs_migration(root)? {
        return Ok(());
    }
    let has_head = repo.head().is_ok();
    if has_head {
        let mut options = StatusOptions::new();
        options.include_untracked(true).recurse_untracked_dirs(true);
        if !repo
            .statuses(Some(&mut options))
            .map_err(|error| error.to_string())?
            .is_empty()
        {
            if okf::needs_content_migration(root)? {
                return Err(
                    "Cannot upgrade this Space to OKF while it has uncommitted files. Commit or discard the local draft, then open it again."
                        .to_string(),
                );
            }
            return Ok(());
        }
    }
    // A temporary Git tree snapshots every working-tree byte, including ignored
    // and non-UTF-8 files. HEAD alone cannot restore those after a failed move.
    let backup = create_migration_backup(repo, root)?;
    if let Err(error) = okf::ensure_bundle(root) {
        rollback_migration(repo, root, &backup)?;
        return Err(error);
    }
    if !has_head {
        return match commit_initial_okf_indexes(repo, root) {
            Ok(()) => delete_migration_backup(repo, &backup.reference),
            Err(error) => {
                rollback_migration(repo, root, &backup)?;
                Err(error)
            }
        };
    }
    if let Err(error) = commit_okf_migration(repo) {
        rollback_migration(repo, root, &backup)?;
        return Err(error);
    }
    delete_migration_backup(repo, &backup.reference)
}

struct MigrationBackup {
    reference: String,
    original_index: Option<Vec<u8>>,
    directories: Vec<PathBuf>,
}

fn create_migration_backup(repo: &Repository, root: &Path) -> Result<MigrationBackup, String> {
    let original_index = std::fs::read(repo.path().join("index")).ok();
    let mut index = repo.index().map_err(|error| error.to_string())?;
    index.clear().map_err(|error| error.to_string())?;
    let mut directories = Vec::new();
    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| {
            entry.path() == root
                || entry
                    .path()
                    .strip_prefix(root)
                    .ok()
                    .and_then(|relative| relative.components().next())
                    .is_some_and(|component| component.as_os_str() != ".git")
        })
        .filter_map(Result::ok)
    {
        let relative = entry
            .path()
            .strip_prefix(root)
            .map_err(|error| error.to_string())?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        if entry.file_type().is_dir() {
            directories.push(relative.to_path_buf());
        } else if entry.file_type().is_file() || entry.file_type().is_symlink() {
            index
                .add_path(relative)
                .map_err(|error| error.to_string())?;
        }
    }
    let tree_id = index.write_tree().map_err(|error| error.to_string())?;
    let tree = repo.find_tree(tree_id).map_err(|error| error.to_string())?;
    let signature =
        Signature::now("CoWiki Migration", "migration@cowiki.app").map_err(|e| e.to_string())?;
    let reference = format!(
        "refs/cowiki/migration-backup/{}",
        uuid::Uuid::new_v4().simple()
    );
    repo.commit(
        Some(&reference),
        &signature,
        &signature,
        "Temporary pre-OKF migration snapshot",
        &tree,
        &[],
    )
    .map_err(|error| error.to_string())?;
    Ok(MigrationBackup {
        reference,
        original_index,
        directories,
    })
}

fn rollback_migration(
    repo: &Repository,
    root: &Path,
    backup: &MigrationBackup,
) -> Result<(), String> {
    let commit = repo
        .find_reference(&backup.reference)
        .and_then(|reference| reference.peel_to_commit())
        .map_err(|error| error.to_string())?;
    repo.checkout_tree(
        commit.as_object(),
        Some(
            CheckoutBuilder::new()
                .force()
                .remove_untracked(true)
                .remove_ignored(true),
        ),
    )
    .map_err(|error| error.to_string())?;
    for directory in &backup.directories {
        std::fs::create_dir_all(root.join(directory)).map_err(|error| error.to_string())?;
    }
    let index_path = repo.path().join("index");
    match &backup.original_index {
        Some(bytes) => std::fs::write(index_path, bytes).map_err(|error| error.to_string())?,
        None if index_path.exists() => {
            std::fs::remove_file(index_path).map_err(|error| error.to_string())?
        }
        None => {}
    }
    delete_migration_backup(repo, &backup.reference)
}

fn delete_migration_backup(repo: &Repository, reference: &str) -> Result<(), String> {
    let mut reference = repo
        .find_reference(reference)
        .map_err(|error| error.to_string())?;
    reference.delete().map_err(|error| error.to_string())
}

fn commit_okf_migration(repo: &Repository) -> Result<(), String> {
    let mut index = repo.index().map_err(|error| error.to_string())?;
    reset_index_to_head(repo, &mut index)?;
    let root = repo
        .workdir()
        .ok_or_else(|| "local Space repository has no working directory".to_string())?;
    let mut options = StatusOptions::new();
    options.include_untracked(true).recurse_untracked_dirs(true);
    for status in repo
        .statuses(Some(&mut options))
        .map_err(|error| error.to_string())?
        .iter()
    {
        let Some(relative) = status.path() else {
            continue;
        };
        let path = Path::new(relative);
        if root.join(path).is_file() {
            index.add_path(path).map_err(|error| error.to_string())?;
        } else {
            let _ = index.remove_path(path);
        }
    }
    index.write().map_err(|error| error.to_string())?;
    let tree_id = index.write_tree().map_err(|error| error.to_string())?;
    let parent = repo
        .head()
        .and_then(|head| head.peel_to_commit())
        .map_err(|error| error.to_string())?;
    if parent.tree_id() == tree_id {
        return Ok(());
    }
    let tree = repo.find_tree(tree_id).map_err(|error| error.to_string())?;
    let signature =
        Signature::now("CoWiki Migration", "migration@cowiki.app").map_err(|e| e.to_string())?;
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        "Migrate Space to OKF v0.1",
        &tree,
        &[&parent],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn head_bytes(repo: &Repository, relative: &str) -> Result<Option<Vec<u8>>, String> {
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
    Ok(Some(blob.content().to_vec()))
}

fn file_diff_from_bytes(
    path: &str,
    old_bytes: Option<Vec<u8>>,
    new_bytes: Option<Vec<u8>>,
) -> FileDiff {
    let old_content = old_bytes
        .as_deref()
        .map(std::str::from_utf8)
        .transpose()
        .ok()
        .flatten()
        .map(ToOwned::to_owned);
    let new_content = new_bytes
        .as_deref()
        .map(std::str::from_utf8)
        .transpose()
        .ok()
        .flatten()
        .map(ToOwned::to_owned);
    let is_binary = old_bytes.as_deref().is_some_and(is_binary_content)
        || new_bytes.as_deref().is_some_and(is_binary_content);
    if !is_binary {
        return text_file_diff(path, old_content, new_content);
    }
    FileDiff {
        path: path.to_string(),
        old_content: None,
        new_content: None,
        is_binary: true,
        old_binary_hash: old_bytes.as_deref().map(sha256_bytes),
        new_binary_hash: new_bytes.as_deref().map(sha256_bytes),
        hunks: vec![DiffHunk {
            header: "Binary file changed".to_string(),
            lines: vec![DiffLine {
                kind: "ctx".to_string(),
                old_line: None,
                new_line: None,
                text: "Binary file changed (contents cannot be displayed).".to_string(),
            }],
        }],
        additions: 0,
        deletions: 0,
    }
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn is_binary_content(bytes: &[u8]) -> bool {
    bytes.contains(&0) || std::str::from_utf8(bytes).is_err()
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
        is_binary: false,
        old_binary_hash: None,
        new_binary_hash: None,
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
    checked_space_path(root, relative)
}

fn checked_space_path(root: &Path, relative: &Path) -> Result<PathBuf, String> {
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            return Err("path may not escape the Space".to_string());
        };
        current.push(component);
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err("symbolic links are not allowed inside a Space".to_string());
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    ensure_inside(root, &current)?;
    Ok(current)
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

fn yaml_string(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', " ")
    )
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let bytes_read = file
            .read(&mut buffer)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn source_hash_matches(path: &Path, expected_hash: &str) -> bool {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|existing| frontmatter_field(&existing, "source_hash"))
        .is_some_and(|existing_hash| existing_hash == expected_hash)
}

fn source_item_for_path(space: &Space, path: &Path) -> Result<SourceItem, String> {
    Ok(SourceItem {
        filename: path
            .strip_prefix(space.local_path.join(okf::RAW_SOURCES_DIR))
            .map(normalize_path)
            .map_err(|error| error.to_string())?,
    })
}

fn read_page_tree(root: &Path, current: &Path) -> Result<Vec<PageMeta>, String> {
    if !current.is_dir() {
        return Ok(vec![]);
    }
    let mut result = Vec::new();
    for entry in std::fs::read_dir(current).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        if file_type.is_symlink() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let relative = path.strip_prefix(root).map_err(|e| e.to_string())?;
        if file_type.is_dir() {
            let slug = normalize_path(relative);
            let index = std::fs::read_to_string(path.join("index.md")).unwrap_or_default();
            let (index_title, summary) = okf::display_metadata(&index);
            result.push(PageMeta {
                slug: slug.clone(),
                path: slug.clone(),
                title: index_title.unwrap_or(name),
                summary: summary.unwrap_or_default(),
                branch: "local".into(),
                kind: "folder".into(),
                children: read_page_tree(root, &path)?,
            });
        } else if DocumentKind::from_path(&path) == DocumentKind::Concept {
            let body = std::fs::read_to_string(&path).unwrap_or_default();
            let slug = normalize_path(&relative.with_extension(""));
            let (title, summary) = okf::display_metadata(&body);
            result.push(PageMeta {
                slug: slug.clone(),
                path: format!("{slug}.md"),
                title: title.unwrap_or_else(|| {
                    path.file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string()
                }),
                summary: summary.unwrap_or_default(),
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

fn safe_source_relative_path(value: &str) -> Result<&Path, String> {
    let path = Path::new(value);
    if path.as_os_str().is_empty()
        || path
            .extension()
            .is_none_or(|extension| !extension.eq_ignore_ascii_case("md"))
        || path.components().any(|component| {
            !matches!(component, Component::Normal(_))
                || component.as_os_str().to_string_lossy().starts_with('.')
        })
    {
        return Err("invalid Source path".to_string());
    }
    Ok(path)
}

fn safe_knowledge_relative_path(value: &str) -> Result<&Path, String> {
    let path = Path::new(value);
    if path.as_os_str().is_empty()
        || path
            .extension()
            .is_none_or(|extension| !extension.eq_ignore_ascii_case("md"))
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("invalid Markdown path".to_string());
    }
    if path
        .components()
        .any(|component| component.as_os_str().to_string_lossy().starts_with('.'))
        && !value.starts_with(".cowiki/sources/")
    {
        return Err("invalid Markdown path".to_string());
    }
    if value.starts_with(".cowiki/sources/") {
        safe_source_relative_path(value.trim_start_matches(".cowiki/sources/"))?;
    }
    Ok(path)
}

fn normalize_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Reads one flat `key: value` frontmatter field, unquoting it the same
/// way `markdown_title` unquotes `title`. Only used to compare a Source
/// document's recorded `source_hash` against a re-import's — not a
/// general YAML reader.
fn frontmatter_field(body: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    let mut in_frontmatter = false;
    for (index, line) in body.lines().enumerate() {
        if index == 0 && line.trim() == "---" {
            in_frontmatter = true;
            continue;
        }
        if !in_frontmatter {
            break;
        }
        if line.trim() == "---" {
            break;
        }
        if let Some(value) = line.strip_prefix(&prefix) {
            return Some(value.trim().trim_matches(['\"', '\'']).to_string());
        }
    }
    None
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

#[cfg(test)]
mod tests {
    use super::{sha256_file, LocalEngine};
    use git2::Repository;
    use std::sync::{mpsc, Arc, Barrier};
    use std::time::Duration;

    #[test]
    fn fresh_local_engine_waits_for_a_folder() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(temp.path()).unwrap();

        assert!(engine.list_spaces().unwrap().is_empty());
    }

    #[test]
    fn cloned_engines_share_a_live_mutation_lock_that_releases_cleanly() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(temp.path()).unwrap();
        let held = engine.lock_mutations().unwrap();
        let contender = engine.clone();
        let barrier = Arc::new(Barrier::new(2));
        let worker_barrier = barrier.clone();
        let (acquired_tx, acquired_rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            worker_barrier.wait();
            let _guard = contender.lock_mutations().unwrap();
            acquired_tx.send(()).unwrap();
        });

        barrier.wait();
        assert!(acquired_rx.recv_timeout(Duration::from_millis(50)).is_err());
        drop(held);
        acquired_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        worker.join().unwrap();
    }

    #[test]
    fn startup_isolates_a_registered_dirty_legacy_space_without_mutating_it() {
        let temp = tempfile::tempdir().unwrap();
        let metadata = temp.path().join("metadata");
        let folder = temp.path().join("legacy");
        std::fs::create_dir_all(&folder).unwrap();
        let engine = LocalEngine::open(&metadata).unwrap();
        let space = engine.add_space("Legacy", "legacy", &folder).unwrap();
        std::fs::write(
            folder.join("dirty.md"),
            "# Dirty legacy page\n\nSearchable startup evidence.\n",
        )
        .unwrap();
        let healthy_folder = temp.path().join("healthy");
        std::fs::create_dir_all(&healthy_folder).unwrap();
        let healthy = engine
            .add_space("Healthy", "healthy", &healthy_folder)
            .unwrap();
        engine
            .write_page(
                &healthy.slug,
                "ready",
                "---\ntype: Note\n---\n\nHealthy startup evidence.\n",
            )
            .unwrap();
        drop(engine);

        let repo = git2::Repository::open(&folder).unwrap();
        let head_before = repo.head().unwrap().target().unwrap();
        let bytes_before = std::fs::read(folder.join("dirty.md")).unwrap();
        let statuses_before = repo
            .statuses(None)
            .unwrap()
            .iter()
            .filter_map(|entry| {
                entry
                    .path()
                    .map(|path| (path.to_string(), entry.status().bits()))
            })
            .collect::<Vec<_>>();
        drop(repo);

        let reopened =
            LocalEngine::open(&metadata).expect("one dirty Space must not abort startup");
        let spaces = reopened.list_spaces().unwrap();
        assert_eq!(spaces.len(), 2);
        assert!(spaces.contains(&space));
        assert!(spaces.contains(&healthy));
        let hits = reopened
            .search_pages(&space.slug, "startup evidence", 10)
            .unwrap();
        assert!(hits.iter().any(|hit| hit.path == "dirty.md"));
        assert!(reopened
            .search_pages(&healthy.slug, "healthy startup", 10)
            .unwrap()
            .iter()
            .any(|hit| hit.path == "ready.md"));

        let repo = git2::Repository::open(&folder).unwrap();
        assert_eq!(repo.head().unwrap().target().unwrap(), head_before);
        assert_eq!(
            std::fs::read(folder.join("dirty.md")).unwrap(),
            bytes_before
        );
        let statuses_after = repo
            .statuses(None)
            .unwrap()
            .iter()
            .filter_map(|entry| {
                entry
                    .path()
                    .map(|path| (path.to_string(), entry.status().bits()))
            })
            .collect::<Vec<_>>();
        assert_eq!(statuses_after, statuses_before);
    }

    #[test]
    fn local_page_methods_use_full_concept_ids_without_a_directory_selector() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(temp.path()).unwrap();
        let folder = temp.path().join("Full Concept IDs");
        std::fs::create_dir(&folder).unwrap();
        let space = engine
            .add_space("Full Concept IDs", "full-concept-ids", &folder)
            .unwrap();

        engine
            .write_page(
                &space.slug,
                "notes/first",
                "---\ntype: Note\n---\n\nLocal draft",
            )
            .unwrap();

        assert_eq!(engine.list_pages(&space.slug).unwrap().len(), 1);
        assert_eq!(
            engine
                .get_page(&space.slug, "notes/first")
                .unwrap()
                .meta
                .path,
            "notes/first.md"
        );
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

        engine
            .rename_path(&space.slug, "notes/first.md", "notes/renamed.md")
            .unwrap();
        engine
            .submit(&space.slug, &["notes/renamed.md".to_string()])
            .unwrap();
        let repo = git2::Repository::open(&folder).unwrap();
        let tree = repo.head().unwrap().peel_to_tree().unwrap();
        assert!(tree
            .get_path(std::path::Path::new("notes/first.md"))
            .is_err());
        assert!(tree
            .get_path(std::path::Path::new("notes/renamed.md"))
            .is_ok());
        drop(tree);
        drop(repo);

        engine.delete_path(&space.slug, "notes/renamed.md").unwrap();
        engine.submit(&space.slug, &[]).unwrap();
        let repo = git2::Repository::open(&folder).unwrap();
        let tree = repo.head().unwrap().peel_to_tree().unwrap();
        assert!(tree
            .get_path(std::path::Path::new("notes/renamed.md"))
            .is_err());
        assert!(!engine.has_uncommitted_changes(&space.slug).unwrap());
    }

    #[test]
    fn checkpoint_snapshots_the_current_draft_without_moving_head() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("notes");
        std::fs::create_dir_all(&folder).unwrap();
        let space = engine.add_space("Notes", "notes", &folder).unwrap();
        let repo = git2::Repository::open(&folder).unwrap();
        let head_before = repo.head().unwrap().target().unwrap();
        let branch_before = repo.head().unwrap().name().unwrap().to_string();
        drop(repo);

        engine
            .write_page(&space.slug, "draft", "# Draft\n\nFirst checkpoint\n")
            .unwrap();
        let saved_draft = engine.history(&space.slug).unwrap();
        assert_eq!(saved_draft.current_draft.changed_files, 2);
        assert!(saved_draft.checkpoints.is_empty());
        let index_before = std::fs::read(folder.join(".git/index")).unwrap();
        let checkpoint = engine
            .create_checkpoint(&space.slug, Some("Checkpoint 2026-07-15 23:45"))
            .unwrap();

        let repo = git2::Repository::open(&folder).unwrap();
        assert_eq!(repo.head().unwrap().target().unwrap(), head_before);
        assert_eq!(repo.head().unwrap().name().unwrap(), branch_before);
        assert_eq!(
            std::fs::read(folder.join(".git/index")).unwrap(),
            index_before
        );
        assert!(engine.has_uncommitted_changes(&space.slug).unwrap());
        let snapshot = repo
            .find_commit(git2::Oid::from_str(&checkpoint.id).unwrap())
            .unwrap()
            .tree()
            .unwrap();
        let blob = repo
            .find_blob(
                snapshot
                    .get_path(std::path::Path::new("draft.md"))
                    .unwrap()
                    .id(),
            )
            .unwrap();
        assert!(std::str::from_utf8(blob.content())
            .unwrap()
            .contains("First checkpoint"));
        assert_eq!(checkpoint.name, "Checkpoint 2026-07-15 23:45");

        let history = engine.history(&space.slug).unwrap();
        assert_eq!(history.current_draft.changed_files, 0);
        assert_eq!(history.checkpoints, vec![checkpoint]);
        assert_eq!(
            std::fs::read(folder.join(".git/index")).unwrap(),
            index_before
        );
        assert!(repo
            .find_reference(&format!(
                "refs/cowiki/checkpoints/{}",
                history.checkpoints[0].id
            ))
            .is_ok());
    }

    #[test]
    fn later_checkpoints_capture_the_exact_draft_and_list_newest_first() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("notes");
        std::fs::create_dir_all(&folder).unwrap();
        let space = engine.add_space("Notes", "notes", &folder).unwrap();

        engine
            .write_page(&space.slug, "temporary", "# Temporary\n")
            .unwrap();
        let first = engine
            .create_checkpoint(&space.slug, Some("First checkpoint"))
            .unwrap();
        engine.delete_path(&space.slug, "temporary.md").unwrap();
        assert_eq!(
            engine
                .history(&space.slug)
                .unwrap()
                .current_draft
                .changed_files,
            2
        );
        let second = engine
            .create_checkpoint(&space.slug, Some("Second checkpoint"))
            .unwrap();

        let history = engine.history(&space.slug).unwrap();
        assert_eq!(
            history
                .checkpoints
                .iter()
                .map(|checkpoint| checkpoint.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Second checkpoint", "First checkpoint"]
        );
        assert_eq!(history.current_draft.changed_files, 0);

        let repo = git2::Repository::open(&folder).unwrap();
        let first_commit = repo
            .find_commit(git2::Oid::from_str(&first.id).unwrap())
            .unwrap();
        let second_commit = repo
            .find_commit(git2::Oid::from_str(&second.id).unwrap())
            .unwrap();
        assert_eq!(second_commit.parent_id(0).unwrap(), first_commit.id());
        assert!(first_commit
            .tree()
            .unwrap()
            .get_path(std::path::Path::new("temporary.md"))
            .is_ok());
        assert!(second_commit
            .tree()
            .unwrap()
            .get_path(std::path::Path::new("temporary.md"))
            .is_err());
    }

    #[test]
    fn draft_changes_are_measured_from_the_latest_checkpoint() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("notes");
        std::fs::create_dir_all(&folder).unwrap();
        let space = engine.add_space("Notes", "notes", &folder).unwrap();
        engine
            .write_page(&space.slug, "draft", "# Draft\n\nFirst version\n")
            .unwrap();

        engine
            .create_checkpoint(&space.slug, Some("First checkpoint"))
            .unwrap();
        assert_eq!(
            engine
                .history(&space.slug)
                .unwrap()
                .current_draft
                .changed_files,
            0
        );

        std::fs::write(folder.join("draft.md"), "# Draft\n\nSecond version\n").unwrap();
        assert_eq!(
            engine
                .history(&space.slug)
                .unwrap()
                .current_draft
                .changed_files,
            1
        );

        engine
            .create_checkpoint(&space.slug, Some("Second checkpoint"))
            .unwrap();
        let history = engine.history(&space.slug).unwrap();
        assert_eq!(history.current_draft.changed_files, 0);
        assert_eq!(history.checkpoints.len(), 2);
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
    fn legacy_discovery_skips_a_dirty_repo_and_imports_a_healthy_repo_without_mutation() {
        fn commit_legacy_page(folder: &std::path::Path, content: &str) -> git2::Repository {
            std::fs::create_dir_all(folder).unwrap();
            std::fs::write(folder.join("legacy.md"), content).unwrap();
            let repo = git2::Repository::init(folder).unwrap();
            let mut index = repo.index().unwrap();
            index.add_path(std::path::Path::new("legacy.md")).unwrap();
            index.write().unwrap();
            let tree_id = index.write_tree().unwrap();
            let tree = repo.find_tree(tree_id).unwrap();
            let signature = git2::Signature::now("legacy", "legacy@example.com").unwrap();
            repo.commit(Some("HEAD"), &signature, &signature, "legacy", &tree, &[])
                .unwrap();
            drop(tree);
            repo
        }

        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let legacy = temp.path().join("cowiki");
        let dirty_folder = legacy.join("dirty-device").join("repo");
        let dirty_repo = commit_legacy_page(&dirty_folder, "# Committed\n");
        std::fs::write(dirty_folder.join("legacy.md"), "# Unsaved local draft\n").unwrap();
        let dirty_head_before = dirty_repo.head().unwrap().target().unwrap();
        let dirty_bytes_before = std::fs::read(dirty_folder.join("legacy.md")).unwrap();
        let dirty_index_before = std::fs::read(dirty_repo.path().join("index")).unwrap();
        let dirty_status_before = dirty_repo
            .statuses(None)
            .unwrap()
            .iter()
            .filter_map(|entry| {
                entry
                    .path()
                    .map(|path| (path.to_string(), entry.status().bits()))
            })
            .collect::<Vec<_>>();
        drop(dirty_repo);

        let healthy_folder = legacy.join("healthy-device").join("repo");
        drop(commit_legacy_page(
            &healthy_folder,
            "# Healthy legacy page\n\nImport evidence.\n",
        ));

        assert_eq!(engine.import_legacy_spaces(&legacy).unwrap(), 1);
        let spaces = engine.list_spaces().unwrap();
        assert_eq!(spaces.len(), 1);
        assert_eq!(spaces[0].slug, "healthy-device");
        assert_eq!(spaces[0].local_path, healthy_folder.canonicalize().unwrap());
        assert!(engine
            .search_pages("healthy-device", "import evidence", 10)
            .unwrap()
            .iter()
            .any(|hit| hit.path == "legacy.md"));

        let dirty_repo = git2::Repository::open(&dirty_folder).unwrap();
        assert_eq!(
            dirty_repo.head().unwrap().target().unwrap(),
            dirty_head_before
        );
        assert_eq!(
            std::fs::read(dirty_folder.join("legacy.md")).unwrap(),
            dirty_bytes_before
        );
        assert_eq!(
            std::fs::read(dirty_repo.path().join("index")).unwrap(),
            dirty_index_before
        );
        let dirty_status_after = dirty_repo
            .statuses(None)
            .unwrap()
            .iter()
            .filter_map(|entry| {
                entry
                    .path()
                    .map(|path| (path.to_string(), entry.status().bits()))
            })
            .collect::<Vec<_>>();
        assert_eq!(dirty_status_after, dirty_status_before);
        assert!(!dirty_folder.join("index.md").exists());
    }

    #[test]
    fn local_file_operations_stay_inside_the_registered_space() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("notes");
        std::fs::create_dir_all(&folder).unwrap();
        let space = engine.add_space("Notes", "notes", &folder).unwrap();

        engine.create_folder(&space.slug, "Projects", None).unwrap();
        engine
            .write_page(&space.slug, "Projects/brief", "# Brief\n")
            .unwrap();
        engine
            .rename_path(&space.slug, "Projects/brief.md", "Projects/plan.md")
            .unwrap();
        assert!(folder.join("Projects/plan.md").is_file());
        assert!(!folder.join("Projects/brief.md").exists());
        assert!(engine
            .rename_path(&space.slug, "Projects/plan.md", "Projects/log.md")
            .is_err());
        assert!(engine
            .rename_path(&space.slug, "Projects/plan.md", "Projects/index.md")
            .is_err());

        engine.delete_path(&space.slug, "Projects").unwrap();
        assert!(!folder.join("Projects").exists());
        assert!(engine
            .create_folder(&space.slug, "../escape", Some("wiki"))
            .is_err());
        assert!(engine.delete_path(&space.slug, "../outside").is_err());
    }

    #[test]
    fn okf_space_lists_arbitrary_root_hierarchy_and_hides_reserved_documents() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("knowledge");
        std::fs::create_dir_all(folder.join("research")).unwrap();
        std::fs::create_dir_all(folder.join("wiki")).unwrap();
        std::fs::create_dir_all(folder.join("entities")).unwrap();
        std::fs::write(
            folder.join("index.md"),
            "---\nokf_version: \"0.1\"\n---\n\n# Knowledge\n",
        )
        .unwrap();
        std::fs::write(
            folder.join("log.md"),
            "# Directory Update Log\n\n## 2026-07-14\n* Imported.\n",
        )
        .unwrap();
        std::fs::write(folder.join("research/index.md"), "# Research Library\n").unwrap();
        std::fs::write(
            folder.join("research/paper.md"),
            "---\ntype: Note\ntitle: Paper\n---\n",
        )
        .unwrap();
        std::fs::write(folder.join("wiki/legacy.md"), "---\ntype: Note\n---\n").unwrap();
        std::fs::write(
            folder.join("entities/person.md"),
            "---\ntype: Person\n---\n",
        )
        .unwrap();
        let space = engine.add_space("Knowledge", "knowledge", &folder).unwrap();

        let sections = engine.list_pages(&space.slug).unwrap();
        let children = &sections;
        assert!(children.iter().any(|item| item.kind == "folder"
            && item.slug == "research"
            && item.title == "Research Library"));
        assert!(children
            .iter()
            .any(|item| item.kind == "folder" && item.slug == "wiki"));
        assert!(children
            .iter()
            .any(|item| item.kind == "folder" && item.slug == "entities"));
        assert!(!children
            .iter()
            .any(|item| matches!(item.slug.as_str(), "index" | "log")));
    }

    #[test]
    fn okf_reserves_index_and_log_and_folder_creation_writes_an_index() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("knowledge");
        std::fs::create_dir_all(&folder).unwrap();
        let space = engine.add_space("Knowledge", "knowledge", &folder).unwrap();

        let root_index = std::fs::read_to_string(folder.join("index.md")).unwrap();
        assert!(root_index.contains("okf_version"));
        assert!(engine
            .write_page(&space.slug, "index", "# Not a concept")
            .is_err());
        assert!(engine
            .write_page(&space.slug, "log", "# Not a concept")
            .is_err());

        engine.create_folder(&space.slug, "Projects", None).unwrap();
        let folder_index = std::fs::read_to_string(folder.join("Projects/index.md")).unwrap();
        assert!(folder_index.starts_with("# Projects\n"));
        assert!(folder_index.contains("cowiki:generated-index"));
    }

    #[test]
    fn opening_a_space_normalizes_only_invalid_okf_documents_losslessly() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("knowledge");
        std::fs::create_dir_all(folder.join("research")).unwrap();
        let conforming = "---\ntype: Custom\nunknown: keep-me\n---\n\nExact bytes.\n";
        std::fs::write(folder.join("stable.md"), conforming).unwrap();
        std::fs::write(folder.join("draft.md"), "# Draft without frontmatter\n").unwrap();
        std::fs::write(
            folder.join("research/index.md"),
            "---\ntitle: Research Library\nsummary: Curated.\n---\n",
        )
        .unwrap();
        std::fs::write(folder.join("log.md"), "Legacy log without OKF groups.\n").unwrap();

        let space = engine.add_space("Knowledge", "knowledge", &folder).unwrap();
        assert_eq!(
            std::fs::read_to_string(folder.join("stable.md")).unwrap(),
            conforming
        );
        let draft = std::fs::read_to_string(folder.join("draft.md")).unwrap();
        assert!(draft.contains("type: Note"));
        assert!(draft.contains("# Draft without frontmatter"));
        let nested = std::fs::read_to_string(folder.join("research/index.md")).unwrap();
        assert!(!nested.starts_with("---"));
        assert!(nested.contains("# Research Library"));
        assert!(nested.contains("Curated."));
        let log = std::fs::read_to_string(folder.join("log.md")).unwrap();
        assert!(log.starts_with("# Directory Update Log"));
        assert!(folder.join(".cowiki/legacy/log.md.legacy").is_file());

        let before = std::fs::read_to_string(folder.join("index.md")).unwrap();
        engine.rebuild_search_index(&space.slug).unwrap();
        crate::okf::ensure_bundle(&folder).unwrap();
        assert_eq!(
            std::fs::read_to_string(folder.join("index.md")).unwrap(),
            before
        );
    }

    #[test]
    fn legacy_sources_move_to_the_controlled_okf_namespace_and_remain_reviewable() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("knowledge");
        std::fs::create_dir_all(folder.join("sources/nested")).unwrap();
        std::fs::write(folder.join("sources/.gitkeep"), "").unwrap();
        std::fs::write(folder.join("sources/raw.md"), "Legacy source body.\n").unwrap();
        std::fs::write(
            folder.join("sources/nested/index.md"),
            "Legacy reserved source body.\n",
        )
        .unwrap();
        std::fs::write(folder.join("sources/nested/report.pdf"), [0xff, 0x00, 0xfe]).unwrap();
        let space = engine.add_space("Knowledge", "knowledge", &folder).unwrap();

        assert!(!folder.join("sources").exists());
        let source = std::fs::read_to_string(folder.join(".cowiki/sources/raw.md")).unwrap();
        assert!(source.contains("type: Source"));
        assert!(source.contains("Legacy source body."));
        let migrated = engine.list_sources(&space.slug).unwrap();
        assert_eq!(migrated.len(), 3);
        assert!(!migrated.iter().any(|item| {
            engine
                .get_source(&space.slug, &item.filename)
                .is_ok_and(|source| source.content.contains("title: .gitkeep"))
        }));
        let migrated_bodies = migrated
            .iter()
            .map(|item| {
                engine
                    .get_source(&space.slug, &item.filename)
                    .unwrap()
                    .content
            })
            .collect::<Vec<_>>();
        assert!(migrated_bodies
            .iter()
            .any(|body| body.contains("title: nested/index.md")));
        assert!(migrated_bodies
            .iter()
            .any(|body| body.contains("title: nested/report.pdf")
                && body.contains("original non-UTF-8 Source bytes")));
        assert_eq!(
            std::fs::read(folder.join(".cowiki/legacy/source-binaries/nested/report.md.legacy"))
                .unwrap(),
            [0xff, 0x00, 0xfe]
        );
        let diffs = engine.working_diff(&space.slug).unwrap();
        assert!(diffs
            .iter()
            .any(|diff| diff.path == ".cowiki/sources/raw.md"));
    }

    #[test]
    fn clean_existing_git_space_gets_one_explicit_idempotent_migration_commit() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("knowledge");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("legacy.md"), "# Legacy\n").unwrap();
        let repo = git2::Repository::init(&folder).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("legacy.md")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let signature = git2::Signature::now("legacy", "legacy@example.com").unwrap();
        repo.commit(Some("HEAD"), &signature, &signature, "legacy", &tree, &[])
            .unwrap();
        drop(tree);
        drop(repo);

        let space = engine.add_space("Knowledge", "knowledge", &folder).unwrap();
        assert_eq!(engine.commit_count(&space.slug).unwrap(), 2);
        assert!(!engine.has_uncommitted_changes(&space.slug).unwrap());
        assert!(folder.join("index.md").is_file());

        crate::okf::ensure_bundle(&folder).unwrap();
        assert!(!engine.has_uncommitted_changes(&space.slug).unwrap());
        assert_eq!(engine.commit_count(&space.slug).unwrap(), 2);
    }

    #[test]
    fn dirty_existing_git_space_is_not_partially_migrated() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("knowledge");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("legacy.md"), "# Committed\n").unwrap();
        let repo = git2::Repository::init(&folder).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("legacy.md")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let signature = git2::Signature::now("legacy", "legacy@example.com").unwrap();
        repo.commit(Some("HEAD"), &signature, &signature, "legacy", &tree, &[])
            .unwrap();
        drop(tree);
        drop(repo);
        std::fs::write(folder.join("legacy.md"), "# Unsaved local draft\n").unwrap();

        let error = engine
            .add_space("Knowledge", "knowledge", &folder)
            .unwrap_err();
        assert!(error.contains("uncommitted"));
        assert!(!folder.join("index.md").exists());
        assert_eq!(
            std::fs::read_to_string(folder.join("legacy.md")).unwrap(),
            "# Unsaved local draft\n"
        );
    }

    #[test]
    fn dirty_conforming_bundles_open_without_optional_indexes_or_version() {
        fn commit_all(folder: &std::path::Path, paths: &[&str]) {
            let repo = git2::Repository::init(folder).unwrap();
            let mut index = repo.index().unwrap();
            for path in paths {
                index.add_path(std::path::Path::new(path)).unwrap();
            }
            index.write().unwrap();
            let tree_id = index.write_tree().unwrap();
            let tree = repo.find_tree(tree_id).unwrap();
            let signature = git2::Signature::now("author", "author@example.com").unwrap();
            repo.commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])
                .unwrap();
        }

        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let missing = temp.path().join("missing-index");
        std::fs::create_dir_all(&missing).unwrap();
        std::fs::write(
            missing.join("note.md"),
            "---\ntype: Note\n---\n\nCommitted.\n",
        )
        .unwrap();
        commit_all(&missing, &["note.md"]);
        std::fs::write(missing.join("note.md"), "---\ntype: Note\n---\n\nDirty.\n").unwrap();
        let first = engine.add_space("Missing", "missing", &missing).unwrap();
        assert!(!missing.join("index.md").exists());
        assert!(engine.has_uncommitted_changes(&first.slug).unwrap());

        let versionless = temp.path().join("versionless");
        std::fs::create_dir_all(&versionless).unwrap();
        let versionless_index = "# Versionless root\n";
        std::fs::write(versionless.join("index.md"), versionless_index).unwrap();
        std::fs::write(
            versionless.join("note.md"),
            "---\ntype: Note\n---\n\nCommitted.\n",
        )
        .unwrap();
        commit_all(&versionless, &["index.md", "note.md"]);
        std::fs::write(
            versionless.join("note.md"),
            "---\ntype: Note\n---\n\nDirty.\n",
        )
        .unwrap();
        let second = engine
            .add_space("Versionless", "versionless", &versionless)
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(versionless.join("index.md")).unwrap(),
            versionless_index
        );
        assert!(engine.has_uncommitted_changes(&second.slug).unwrap());
    }

    #[test]
    fn failed_clean_migration_restores_head_worktree_and_index() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("knowledge");
        std::fs::create_dir_all(&folder).unwrap();
        let malformed = "---\nokf_version: \"0.1\"\n---\n\n# Knowledge\n\n<!-- cowiki:generated-index:start -->\n";
        std::fs::write(folder.join("index.md"), malformed).unwrap();
        std::fs::write(
            folder.join("stable.md"),
            "---\ntype: Note\ntitle: Stable\n---\n",
        )
        .unwrap();
        std::fs::write(folder.join(".gitignore"), "sources/\n").unwrap();
        std::fs::create_dir_all(folder.join("sources")).unwrap();
        std::fs::write(folder.join("sources/raw.md"), [0xff, 0x00, 0xfe]).unwrap();
        let repo = git2::Repository::init(&folder).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new(".gitignore")).unwrap();
        index.add_path(std::path::Path::new("index.md")).unwrap();
        index.add_path(std::path::Path::new("stable.md")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let signature = git2::Signature::now("legacy", "legacy@example.com").unwrap();
        let head = repo
            .commit(Some("HEAD"), &signature, &signature, "legacy", &tree, &[])
            .unwrap();
        drop(tree);
        drop(repo);

        assert!(engine.add_space("Knowledge", "knowledge", &folder).is_err());
        let repo = git2::Repository::open(&folder).unwrap();
        assert_eq!(repo.head().unwrap().target().unwrap(), head);
        let statuses = repo.statuses(None).unwrap();
        assert!(statuses.iter().all(|status| status.status().is_ignored()));
        assert_eq!(
            std::fs::read_to_string(folder.join("index.md")).unwrap(),
            malformed
        );
        assert_eq!(
            std::fs::read(folder.join("sources/raw.md")).unwrap(),
            [0xff, 0x00, 0xfe]
        );
        assert!(!folder.join(".cowiki").exists());
    }

    #[test]
    fn failed_unborn_migration_restores_every_local_file_and_directory() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("knowledge");
        std::fs::create_dir_all(folder.join("sources/empty")).unwrap();
        std::fs::write(folder.join("sources/raw.md"), "Original source\n").unwrap();
        let malformed = "---\nokf_version: \"0.1\"\n---\n\n# Knowledge\n\n<!-- cowiki:generated-index:start -->\n";
        std::fs::write(folder.join("index.md"), malformed).unwrap();

        assert!(engine.add_space("Knowledge", "knowledge", &folder).is_err());
        assert_eq!(
            std::fs::read_to_string(folder.join("sources/raw.md")).unwrap(),
            "Original source\n"
        );
        assert!(folder.join("sources/empty").is_dir());
        assert!(!folder.join(".cowiki").exists());
        assert_eq!(
            std::fs::read_to_string(folder.join("index.md")).unwrap(),
            malformed
        );
        let repo = git2::Repository::open(&folder).unwrap();
        assert!(repo.head().is_err());
        assert!(repo
            .references_glob("refs/cowiki/migration-backup/*")
            .unwrap()
            .next()
            .is_none());
    }

    #[test]
    fn linked_git_worktree_can_be_migrated_without_indexing_its_git_file() {
        let temp = tempfile::tempdir().unwrap();
        let main = temp.path().join("main");
        let linked = temp.path().join("linked");
        std::fs::create_dir_all(&main).unwrap();
        std::fs::write(main.join("legacy.md"), "# Legacy\n").unwrap();
        let repo = git2::Repository::init(&main).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("legacy.md")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let signature = git2::Signature::now("legacy", "legacy@example.com").unwrap();
        repo.commit(Some("HEAD"), &signature, &signature, "legacy", &tree, &[])
            .unwrap();
        drop(tree);
        repo.worktree("linked", &linked, None).unwrap();
        drop(repo);
        assert!(linked.join(".git").is_file());

        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let space = engine.add_space("Linked", "linked", &linked).unwrap();
        assert!(linked.join("index.md").is_file());
        assert_eq!(engine.commit_count(&space.slug).unwrap(), 2);
    }

    #[test]
    fn checked_save_never_overwrites_external_agent_edits() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("notes");
        std::fs::create_dir_all(&folder).unwrap();
        let space = engine.add_space("Notes", "notes", &folder).unwrap();
        engine
            .write_page(&space.slug, "shared", "# Shared\n\nHuman base")
            .unwrap();

        std::fs::write(folder.join("shared.md"), "# Shared\n\nAgent result").unwrap();
        let error = engine
            .write_page_checked(
                &space.slug,
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
            .write_page(&space.slug, "same", "# Original")
            .unwrap();

        assert!(engine
            .write_page_checked(&space.slug, "same", "# Replacement", None, true)
            .is_err());
        let preserved = std::fs::read_to_string(folder.join("same.md")).unwrap();
        assert!(preserved.contains("# Original"));
        assert!(!preserved.contains("Replacement"));
    }

    #[test]
    fn working_diff_exposes_local_changes_for_review() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("notes");
        std::fs::create_dir_all(&folder).unwrap();
        let space = engine.add_space("Notes", "notes", &folder).unwrap();
        engine
            .write_page(&space.slug, "draft", "# Draft\n\nAgent proposal\n")
            .unwrap();

        let diffs = engine.working_diff(&space.slug).unwrap();
        let draft = diffs.iter().find(|diff| diff.path == "draft.md").unwrap();
        assert!(draft.old_content.is_none());
        assert!(draft
            .new_content
            .as_deref()
            .unwrap()
            .contains("Agent proposal"));
        assert!(draft.additions > 0);
        assert!(diffs.iter().any(|diff| diff.path == "index.md"));

        engine
            .submit(&space.slug, &["draft.md".to_string()])
            .unwrap();
        assert!(engine.working_diff(&space.slug).unwrap().is_empty());
    }

    #[test]
    fn background_agent_change_is_isolated_and_lists_its_diff() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("notes");
        std::fs::create_dir_all(&folder).unwrap();
        let space = engine.add_space("Notes", "notes", &folder).unwrap();
        std::fs::write(folder.join("draft.md"), "# Draft at dispatch\n").unwrap();
        let index_before = std::fs::read(folder.join(".git/index")).unwrap();

        let change = engine.create_agent_change(&space.slug, "Codex").unwrap();
        assert_eq!(
            std::fs::read(folder.join(".git/index")).unwrap(),
            index_before
        );
        assert_eq!(
            std::fs::read_to_string(change.worktree_path.join("draft.md")).unwrap(),
            "# Draft at dispatch\n"
        );
        std::fs::write(
            change.worktree_path.join("agent.md"),
            "# Background result\n",
        )
        .unwrap();

        assert!(!folder.join("agent.md").exists());
        let changes = engine.list_agent_changes(&space.slug).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].id, change.id);
        assert_eq!(changes[0].status, "open");
        let diff = changes[0]
            .diffs
            .iter()
            .find(|diff| diff.path == "agent.md")
            .unwrap();
        assert!(diff.old_content.is_none());
        assert_eq!(diff.new_content.as_deref(), Some("# Background result\n"));
    }

    #[test]
    fn binary_draft_and_agent_deltas_remain_visible_and_merge_raw_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("notes");
        std::fs::create_dir_all(&folder).unwrap();
        let space = engine.add_space("Notes", "notes", &folder).unwrap();
        let baseline = [0_u8, 159, 146, 150];
        std::fs::write(folder.join("asset.bin"), baseline).unwrap();
        engine
            .submit(&space.slug, &["asset.bin".to_string()])
            .unwrap();

        let human_bytes = [0_u8, 255, 1, 2, 3];
        std::fs::write(folder.join("asset.bin"), human_bytes).unwrap();
        let draft = engine.working_diff(&space.slug).unwrap();
        let binary_draft = draft.iter().find(|diff| diff.path == "asset.bin").unwrap();
        assert!(binary_draft.is_binary);
        assert!(binary_draft.old_binary_hash.is_some());
        assert!(binary_draft.new_binary_hash.is_some());
        assert!(binary_draft.hunks[0].lines[0].text.contains("Binary file"));
        engine.keep_working_diff(&space.slug, &draft).unwrap();
        assert!(engine.working_diff(&space.slug).unwrap().is_empty());

        let change = engine.create_agent_change(&space.slug, "Codex").unwrap();
        let agent_bytes = [255_u8, 0, 42, 11];
        std::fs::write(change.worktree_path.join("asset.bin"), agent_bytes).unwrap();
        let added_bytes = [254_u8, 253, 0, 1];
        std::fs::write(change.worktree_path.join("new.bin"), added_bytes).unwrap();
        let listed = engine.list_agent_changes(&space.slug).unwrap();
        let modified = listed[0]
            .diffs
            .iter()
            .find(|diff| diff.path == "asset.bin")
            .unwrap();
        let added = listed[0]
            .diffs
            .iter()
            .find(|diff| diff.path == "new.bin")
            .unwrap();
        assert!(modified.is_binary);
        assert!(modified.old_binary_hash.is_some());
        assert!(modified.new_binary_hash.is_some());
        assert!(added.is_binary);
        assert!(added.old_binary_hash.is_none());
        assert!(added.new_binary_hash.is_some());

        let merged = engine.merge_agent_change(&space.slug, &change.id).unwrap();
        assert_eq!(merged.status, "merged");
        assert_eq!(
            std::fs::read(folder.join("asset.bin")).unwrap(),
            agent_bytes
        );
        assert_eq!(std::fs::read(folder.join("new.bin")).unwrap(), added_bytes);
    }

    #[cfg(unix)]
    #[test]
    fn ignored_symlink_parent_cannot_escape_draft_during_merge() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("notes");
        let outside = temp.path().join("outside");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let space = engine.add_space("Notes", "notes", &folder).unwrap();
        let change = engine.create_agent_change(&space.slug, "Codex").unwrap();
        std::fs::create_dir_all(change.worktree_path.join("redirect")).unwrap();
        std::fs::write(
            change.worktree_path.join("redirect/escaped.md"),
            "# Must stay inside\n",
        )
        .unwrap();
        engine.list_agent_changes(&space.slug).unwrap();

        std::fs::write(folder.join(".git/info/exclude"), "redirect/\n").unwrap();
        symlink(&outside, folder.join("redirect")).unwrap();
        let error = engine
            .merge_agent_change(&space.slug, &change.id)
            .unwrap_err();
        assert!(error.to_lowercase().contains("symbolic link"));
        assert!(!outside.join("escaped.md").exists());
        assert!(folder.join("redirect").is_symlink());
    }

    #[cfg(unix)]
    #[test]
    fn ignored_symlink_target_cannot_be_replaced_during_merge() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("notes");
        let outside = temp.path().join("outside.md");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(&outside, "# Outside\n").unwrap();
        let space = engine.add_space("Notes", "notes", &folder).unwrap();
        let change = engine.create_agent_change(&space.slug, "Codex").unwrap();
        std::fs::write(change.worktree_path.join("escaped.md"), "# Agent result\n").unwrap();
        engine.list_agent_changes(&space.slug).unwrap();

        std::fs::write(folder.join(".git/info/exclude"), "escaped.md\n").unwrap();
        symlink(&outside, folder.join("escaped.md")).unwrap();
        let error = engine
            .merge_agent_change(&space.slug, &change.id)
            .unwrap_err();
        assert!(error.to_lowercase().contains("symbolic link"));
        assert_eq!(std::fs::read_to_string(outside).unwrap(), "# Outside\n");
    }

    #[cfg(unix)]
    #[test]
    fn managed_agent_worktree_rejects_symlink_replacement() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("notes");
        let outside = temp.path().join("outside");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let space = engine.add_space("Notes", "notes", &folder).unwrap();
        let change = engine.create_agent_change(&space.slug, "Codex").unwrap();
        std::fs::remove_dir_all(&change.worktree_path).unwrap();
        symlink(&outside, &change.worktree_path).unwrap();

        let error = engine
            .agent_change_worktree(&space.slug, &change.id)
            .unwrap_err();
        assert!(error.to_lowercase().contains("symbolic link"));
    }

    #[test]
    fn clean_agent_merge_preserves_human_draft_and_head() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("notes");
        std::fs::create_dir_all(&folder).unwrap();
        let space = engine.add_space("Notes", "notes", &folder).unwrap();
        std::fs::write(folder.join("human.md"), "# Human baseline\n").unwrap();
        std::fs::write(folder.join("agent.md"), "# Agent baseline\n").unwrap();
        engine.submit(&space.slug, &[]).unwrap();
        let change = engine.create_agent_change(&space.slug, "Codex").unwrap();
        let head_before = git2::Repository::open(&folder)
            .unwrap()
            .head()
            .unwrap()
            .target()
            .unwrap();

        std::fs::write(folder.join("human.md"), "# Human latest Draft\n").unwrap();
        let index_repo = git2::Repository::open(&folder).unwrap();
        let mut user_index = index_repo.index().unwrap();
        user_index
            .add_path(std::path::Path::new("human.md"))
            .unwrap();
        user_index.write().unwrap();
        drop(user_index);
        drop(index_repo);
        let index_before = std::fs::read(folder.join(".git/index")).unwrap();
        std::fs::write(
            change.worktree_path.join("agent.md"),
            "# Background Agent result\n",
        )
        .unwrap();

        let merged = engine.merge_agent_change(&space.slug, &change.id).unwrap();
        assert_eq!(merged.status, "merged");
        assert_eq!(
            std::fs::read_to_string(folder.join("human.md")).unwrap(),
            "# Human latest Draft\n"
        );
        assert_eq!(
            std::fs::read_to_string(folder.join("agent.md")).unwrap(),
            "# Background Agent result\n"
        );
        let repo = git2::Repository::open(&folder).unwrap();
        assert_eq!(repo.head().unwrap().target().unwrap(), head_before);
        assert_eq!(
            std::fs::read(folder.join(".git/index")).unwrap(),
            index_before
        );
        drop(repo);
        let draft = engine.working_diff(&space.slug).unwrap();
        assert!(draft.iter().any(|diff| diff.path == "human.md"));
        assert!(draft.iter().any(|diff| diff.path == "agent.md"));
        assert!(!change.worktree_path.exists());
        let repo = Repository::open(&folder).unwrap();
        assert!(!repo
            .path()
            .join("worktrees")
            .join(format!("cowiki-agent-{}", change.id))
            .exists());
        let closed = engine.list_agent_changes(&space.slug).unwrap();
        assert_eq!(closed[0].status, "merged");
        assert!(closed[0].diffs.iter().any(|diff| diff.path == "agent.md"));
        assert!(engine
            .agent_change_worktree(&space.slug, &change.id)
            .is_err());
    }

    #[test]
    fn conflicting_agent_merge_leaves_draft_bytes_unchanged() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("notes");
        std::fs::create_dir_all(&folder).unwrap();
        let space = engine.add_space("Notes", "notes", &folder).unwrap();
        std::fs::write(folder.join("shared.md"), "# Shared baseline\n").unwrap();
        engine.submit(&space.slug, &[]).unwrap();
        let change = engine
            .create_agent_change(&space.slug, "Claude Code")
            .unwrap();

        std::fs::write(folder.join("shared.md"), "# Human latest Draft\n").unwrap();
        std::fs::write(
            change.worktree_path.join("shared.md"),
            "# Background Agent result\n",
        )
        .unwrap();
        let draft_before = std::fs::read(folder.join("shared.md")).unwrap();

        let conflict = engine.merge_agent_change(&space.slug, &change.id).unwrap();
        assert_eq!(conflict.status, "needsResolution");
        assert_eq!(
            std::fs::read(folder.join("shared.md")).unwrap(),
            draft_before
        );
        let listed = engine.list_agent_changes(&space.slug).unwrap();
        assert_eq!(listed[0].status, "needsResolution");
    }

    #[test]
    fn discarding_agent_change_does_not_modify_draft() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("notes");
        std::fs::create_dir_all(&folder).unwrap();
        let space = engine.add_space("Notes", "notes", &folder).unwrap();
        std::fs::write(folder.join("shared.md"), "# Baseline\n").unwrap();
        engine.submit(&space.slug, &[]).unwrap();
        let change = engine
            .create_agent_change(&space.slug, "Gemini CLI")
            .unwrap();
        std::fs::write(folder.join("shared.md"), "# Human Draft\n").unwrap();
        std::fs::write(change.worktree_path.join("shared.md"), "# Agent result\n").unwrap();
        let draft_before = std::fs::read(folder.join("shared.md")).unwrap();
        let head_before = git2::Repository::open(&folder)
            .unwrap()
            .head()
            .unwrap()
            .target()
            .unwrap();

        let discarded = engine
            .discard_agent_change(&space.slug, &change.id)
            .unwrap();
        assert_eq!(discarded.status, "discarded");
        assert_eq!(
            std::fs::read(folder.join("shared.md")).unwrap(),
            draft_before
        );
        assert_eq!(
            git2::Repository::open(&folder)
                .unwrap()
                .head()
                .unwrap()
                .target()
                .unwrap(),
            head_before
        );
        assert!(!change.worktree_path.exists());
        let repo = Repository::open(&folder).unwrap();
        assert!(!repo
            .path()
            .join("worktrees")
            .join(format!("cowiki-agent-{}", change.id))
            .exists());
        let closed = engine.list_agent_changes(&space.slug).unwrap();
        assert_eq!(closed[0].status, "discarded");
        assert!(closed[0].diffs.iter().any(|diff| diff.path == "shared.md"));
        assert!(engine
            .agent_change_worktree(&space.slug, &change.id)
            .is_err());
    }

    #[test]
    fn concurrent_draft_change_before_write_is_not_overwritten() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("notes");
        std::fs::create_dir_all(&folder).unwrap();
        let space = engine.add_space("Notes", "notes", &folder).unwrap();
        std::fs::write(folder.join("shared.md"), "# Baseline\n").unwrap();
        engine.submit(&space.slug, &[]).unwrap();
        let change = engine.create_agent_change(&space.slug, "Codex").unwrap();
        std::fs::write(change.worktree_path.join("shared.md"), "# Agent result\n").unwrap();
        let concurrent = b"# Concurrent human change\n".to_vec();

        let error = engine
            .merge_agent_change_with_pre_write_hook(&space.slug, &change.id, |_| {
                std::fs::write(folder.join("shared.md"), &concurrent)
                    .map_err(|error| error.to_string())
            })
            .unwrap_err();
        assert!(error.contains("changed while"));
        assert_eq!(std::fs::read(folder.join("shared.md")).unwrap(), concurrent);
        assert_eq!(
            engine.list_agent_changes(&space.slug).unwrap()[0].status,
            "open"
        );
    }

    #[test]
    fn merged_change_succeeds_when_cleanup_fails_and_list_retries_cleanup() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("notes");
        std::fs::create_dir_all(&folder).unwrap();
        let space = engine.add_space("Notes", "notes", &folder).unwrap();
        let change = engine.create_agent_change(&space.slug, "Codex").unwrap();
        std::fs::write(change.worktree_path.join("agent.md"), "# Agent result\n").unwrap();

        let merged = engine
            .merge_agent_change_with_cleanup_hook(&space.slug, &change.id, || {
                Err("injected cleanup failure".to_string())
            })
            .unwrap();
        assert_eq!(merged.status, "merged");
        assert!(merged.diffs.iter().any(|diff| diff.path == "agent.md"));
        assert!(change.worktree_path.exists());
        assert_eq!(
            std::fs::read_to_string(folder.join("agent.md")).unwrap(),
            "# Agent result\n"
        );

        let listed = engine.list_agent_changes(&space.slug).unwrap();
        assert_eq!(listed[0].status, "merged");
        assert!(listed[0].diffs.iter().any(|diff| diff.path == "agent.md"));
        assert!(!change.worktree_path.exists());
        assert!(engine
            .agent_change_worktree(&space.slug, &change.id)
            .is_err());
    }

    #[test]
    fn discarded_change_succeeds_when_cleanup_fails_and_list_retries_cleanup() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("notes");
        std::fs::create_dir_all(&folder).unwrap();
        let space = engine.add_space("Notes", "notes", &folder).unwrap();
        let change = engine.create_agent_change(&space.slug, "Codex").unwrap();
        std::fs::write(change.worktree_path.join("agent.md"), "# Discard me\n").unwrap();

        let discarded = engine
            .discard_agent_change_with_cleanup_hook(&space.slug, &change.id, || {
                Err("injected cleanup failure".to_string())
            })
            .unwrap();
        assert_eq!(discarded.status, "discarded");
        assert!(discarded.diffs.iter().any(|diff| diff.path == "agent.md"));
        assert!(change.worktree_path.exists());
        assert!(!folder.join("agent.md").exists());

        let listed = engine.list_agent_changes(&space.slug).unwrap();
        assert_eq!(listed[0].status, "discarded");
        assert!(listed[0].diffs.iter().any(|diff| diff.path == "agent.md"));
        assert!(!change.worktree_path.exists());
        assert!(engine
            .agent_change_worktree(&space.slug, &change.id)
            .is_err());
    }

    #[test]
    fn submit_normalizes_external_agent_edits_and_refreshes_reserved_documents() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("notes");
        std::fs::create_dir_all(&folder).unwrap();
        let space = engine.add_space("Notes", "notes", &folder).unwrap();
        std::fs::write(
            folder.join("agent.md"),
            "# Agent draft without frontmatter\n",
        )
        .unwrap();
        std::fs::write(folder.join("log.md"), "invalid agent log\n").unwrap();

        engine.submit(&space.slug, &[]).unwrap();

        let concept = std::fs::read_to_string(folder.join("agent.md")).unwrap();
        let log = std::fs::read_to_string(folder.join("log.md")).unwrap();
        let index = std::fs::read_to_string(folder.join("index.md")).unwrap();
        assert!(concept.contains("type: Note"));
        assert!(log.starts_with("# Directory Update Log"));
        assert!(index.contains("agent.md"));
        assert!(folder.join(".cowiki/legacy/log.md.legacy").is_file());
        assert!(!engine.has_uncommitted_changes(&space.slug).unwrap());
        let repo = git2::Repository::open(&folder).unwrap();
        let tree = repo.head().unwrap().peel_to_tree().unwrap();
        assert!(tree
            .get_path(std::path::Path::new(".cowiki/legacy/log.md.legacy"))
            .is_ok());
    }

    #[test]
    fn keep_rejects_changes_that_arrived_after_review_opened() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("notes");
        std::fs::create_dir_all(&folder).unwrap();
        let space = engine.add_space("Notes", "notes", &folder).unwrap();
        std::fs::write(folder.join("proposal.md"), "# First\n").unwrap();
        let initial_commits = engine.commit_count(&space.slug).unwrap();
        let reviewed = engine.working_diff(&space.slug).unwrap();

        std::fs::write(folder.join("proposal.md"), "# Changed after review\n").unwrap();
        let error = engine
            .keep_working_diff(&space.slug, &reviewed)
            .unwrap_err();
        assert!(error.contains("changed after this Review opened"));
        assert_eq!(engine.commit_count(&space.slug).unwrap(), initial_commits);
    }

    #[test]
    fn review_prepares_external_agent_pages_before_keep_without_false_conflicts() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("notes");
        std::fs::create_dir_all(&folder).unwrap();
        let space = engine.add_space("Notes", "notes", &folder).unwrap();
        std::fs::write(
            folder.join("agent.md"),
            "---\ntype: Note\n---\n\nConforming external page.\n",
        )
        .unwrap();

        let reviewed = engine.working_diff(&space.slug).unwrap();
        assert!(reviewed.iter().any(|diff| diff.path == "agent.md"));
        assert!(reviewed.iter().any(|diff| diff.path == "index.md"));
        assert!(
            engine
                .keep_working_diff(&space.slug, &reviewed)
                .unwrap()
                .committed
        );
        assert!(engine.working_diff(&space.slug).unwrap().is_empty());

        std::fs::write(folder.join("malformed.md"), "# Missing frontmatter\n").unwrap();
        let normalized_review = engine.working_diff(&space.slug).unwrap();
        let malformed = normalized_review
            .iter()
            .find(|diff| diff.path == "malformed.md")
            .unwrap();
        assert!(malformed
            .new_content
            .as_deref()
            .unwrap()
            .contains("type: Note"));
        assert!(
            engine
                .keep_working_diff(&space.slug, &normalized_review)
                .unwrap()
                .committed
        );
        assert!(engine.working_diff(&space.slug).unwrap().is_empty());
    }

    #[test]
    fn ui_search_maps_root_pages_to_wiki_and_hides_sources() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("notes");
        std::fs::create_dir_all(folder.join(".cowiki/sources")).unwrap();
        std::fs::write(folder.join("guide.md"), "# Offline Guide\n").unwrap();
        std::fs::write(
            folder.join(".cowiki/sources/raw.md"),
            "---\ntype: Source\ntitle: Raw\n---\n\n# Offline Raw Source\n",
        )
        .unwrap();
        let space = engine.add_space("Notes", "notes", &folder).unwrap();

        let response = engine.search(&space.slug, "offline", 10).unwrap();
        assert_eq!(response.keyword.len(), 1);
        assert_eq!(response.keyword[0].path, "guide.md");
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
        assert!(item.filename.starts_with("_encoded/"));
        assert!(item.filename.ends_with(".md"));
        let body =
            std::fs::read_to_string(folder.join(".cowiki/sources").join(&item.filename)).unwrap();
        assert!(body.contains("type: Source"));
        assert!(body.contains("keeps working offline"));
        assert_eq!(
            engine
                .search_pages(&space.slug, "working offline", 10)
                .unwrap()
                .len(),
            1
        );
        assert!(engine
            .working_diff(&space.slug)
            .unwrap()
            .iter()
            .any(|diff| diff.path == format!(".cowiki/sources/{}", item.filename)));
        assert!(engine.submit(&space.slug, &[]).unwrap().committed);
        assert!(!engine.has_uncommitted_changes(&space.slug).unwrap());
    }

    #[test]
    fn reimporting_same_content_reuses_source_and_different_content_stays_separate() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("knowledge");
        std::fs::create_dir_all(&folder).unwrap();
        let space = engine.add_space("Knowledge", "knowledge", &folder).unwrap();

        let source_path = temp.path().join("report.txt");
        std::fs::write(&source_path, "First draft of the report.").unwrap();
        let first = engine
            .ingest_files(&space.slug, &[source_path.to_string_lossy().into_owned()])
            .unwrap();
        assert_eq!(first.len(), 1);
        let first = first[0].source.clone().unwrap();

        // Re-importing the identical file (e.g. a re-synced folder, or the
        // user clicking Import twice) should land on the exact same path,
        // not spawn a second copy.
        let repeat = engine
            .ingest_files(&space.slug, &[source_path.to_string_lossy().into_owned()])
            .unwrap();
        assert_eq!(repeat[0].source.as_ref().unwrap().filename, first.filename);
        assert_eq!(
            engine.list_sources(&space.slug).unwrap().len(),
            1,
            "identical re-import must not create a second Source document"
        );

        // A different file that happens to share the original's name
        // should not silently overwrite it, but re-importing *that* file
        // again should still be idempotent (same suffix every time).
        std::fs::write(&source_path, "Genuinely different content, same filename.").unwrap();
        let collision_a = engine
            .ingest_files(&space.slug, &[source_path.to_string_lossy().into_owned()])
            .unwrap();
        let collision_a = collision_a[0].source.clone().unwrap();
        assert_ne!(collision_a.filename, first.filename);

        let collision_b = engine
            .ingest_files(&space.slug, &[source_path.to_string_lossy().into_owned()])
            .unwrap();
        assert_eq!(
            collision_b[0].source.as_ref().unwrap().filename,
            collision_a.filename,
            "re-importing the same colliding file must converge on the same suffixed path"
        );
        assert_eq!(engine.list_sources(&space.slug).unwrap().len(), 2);
    }

    #[test]
    fn reimporting_identical_file_does_not_overwrite_source_edits() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("knowledge");
        std::fs::create_dir_all(&folder).unwrap();
        let space = engine.add_space("Knowledge", "knowledge", &folder).unwrap();

        let source_path = temp.path().join("report.txt");
        std::fs::write(&source_path, "Original report text.").unwrap();
        let first = engine
            .ingest_files(&space.slug, &[source_path.to_string_lossy().into_owned()])
            .unwrap();
        let source = first[0].source.as_ref().unwrap();
        let stored_path = folder.join(".cowiki/sources").join(&source.filename);
        let mut annotated = std::fs::read_to_string(&stored_path).unwrap();
        annotated.push_str("\nHuman annotation that must survive re-import.\n");
        std::fs::write(&stored_path, &annotated).unwrap();

        engine
            .ingest_files(&space.slug, &[source_path.to_string_lossy().into_owned()])
            .unwrap();

        assert_eq!(std::fs::read_to_string(stored_path).unwrap(), annotated);
    }

    #[test]
    fn reimporting_a_colliding_file_does_not_overwrite_source_edits() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("knowledge");
        std::fs::create_dir_all(&folder).unwrap();
        let space = engine.add_space("Knowledge", "knowledge", &folder).unwrap();

        let source_path = temp.path().join("report.txt");
        std::fs::write(&source_path, "First report.").unwrap();
        engine
            .ingest_files(&space.slug, &[source_path.to_string_lossy().into_owned()])
            .unwrap();

        std::fs::write(&source_path, "Second report with the same file name.").unwrap();
        let collision = engine
            .ingest_files(&space.slug, &[source_path.to_string_lossy().into_owned()])
            .unwrap();
        let collision = collision[0].source.as_ref().unwrap();
        let stored_path = folder.join(".cowiki/sources").join(&collision.filename);
        let mut annotated = std::fs::read_to_string(&stored_path).unwrap();
        annotated.push_str("\nAnnotation on the colliding Source.\n");
        std::fs::write(&stored_path, &annotated).unwrap();

        engine
            .ingest_files(&space.slug, &[source_path.to_string_lossy().into_owned()])
            .unwrap();

        assert_eq!(std::fs::read_to_string(stored_path).unwrap(), annotated);
    }

    #[test]
    fn source_hashing_streams_the_complete_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("source.txt");
        std::fs::write(&path, "abc").unwrap();

        assert_eq!(
            sha256_file(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn ingest_files_reports_per_file_errors_without_failing_the_batch() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("knowledge");
        std::fs::create_dir_all(&folder).unwrap();
        let space = engine.add_space("Knowledge", "knowledge", &folder).unwrap();

        let good = temp.path().join("notes.txt");
        std::fs::write(&good, "Readable notes.").unwrap();
        let unsupported = temp.path().join("archive.doc");
        std::fs::write(&unsupported, b"legacy binary format").unwrap();

        let outcomes = engine
            .ingest_files(
                &space.slug,
                &[
                    good.to_string_lossy().into_owned(),
                    unsupported.to_string_lossy().into_owned(),
                ],
            )
            .unwrap();

        assert_eq!(outcomes.len(), 2);
        assert!(outcomes[0].source.is_some() && outcomes[0].error.is_none());
        assert!(outcomes[1].source.is_none());
        assert!(outcomes[1]
            .error
            .as_deref()
            .unwrap()
            .contains("not a supported source format"));
        assert_eq!(engine.list_sources(&space.slug).unwrap().len(), 1);
    }

    #[test]
    fn source_storage_is_collision_safe_for_reserved_and_non_markdown_names() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("knowledge");
        std::fs::create_dir_all(&folder).unwrap();
        let space = engine.add_space("Knowledge", "knowledge", &folder).unwrap();

        let markdown = engine
            .ingest(&space.slug, "text", "Markdown", Some("foo.md"))
            .unwrap();
        let plain = engine
            .ingest(&space.slug, "text", "Plain", Some("foo"))
            .unwrap();
        let reserved = engine
            .ingest(&space.slug, "text", "Reserved", Some("index.md"))
            .unwrap();
        let duplicate = engine
            .ingest(&space.slug, "text", "Duplicate", Some("foo.md"))
            .unwrap();

        assert_eq!(markdown.filename, "foo.md");
        assert!(plain.filename.starts_with("_encoded/"));
        assert!(reserved.filename.starts_with("_encoded/"));
        assert_ne!(plain.filename, reserved.filename);
        assert_ne!(markdown.filename, duplicate.filename);
        for item in [markdown, plain, reserved, duplicate] {
            let source = engine.get_source(&space.slug, &item.filename).unwrap();
            assert!(source.content.contains("type: Source"));
        }
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
            .write_page(&space.slug, "new-page", "# New Page\n\nOrchid field notes")
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
    fn backlinks_resolve_standard_markdown_links_with_case_sensitive_full_paths() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("knowledge");
        std::fs::create_dir_all(folder.join("notes/deep")).unwrap();
        std::fs::write(folder.join("Target.md"), "# Uppercase Target\n").unwrap();
        std::fs::write(folder.join("My Doc.md"), "# Encoded target\n").unwrap();
        std::fs::write(folder.join("Fake.md"), "# Must not receive a backlink\n").unwrap();
        std::fs::write(folder.join("notes/Target.md"), "# Nested Target\n").unwrap();
        std::fs::write(
            folder.join("notes/deep/source.md"),
            "# Source\n\n[Root][root], [nested](../Target.md), and [encoded](../../My%20Doc.md).\n\n[root]: ../../Target.md#details\n\n`[not a link](../../Fake.md)`\n\n```md\n[fake](../../Fake.md)\n```\n",
        )
        .unwrap();
        let space = engine.add_space("Knowledge", "knowledge", &folder).unwrap();

        let root_links = engine.list_backlinks(&space.slug, "Target.md").unwrap();
        assert_eq!(root_links.len(), 1);
        assert_eq!(root_links[0].path, "notes/deep/source.md");
        let nested_links = engine
            .list_backlinks(&space.slug, "notes/Target.md")
            .unwrap();
        assert_eq!(nested_links.len(), 1);
        assert_eq!(
            engine
                .list_backlinks(&space.slug, "My Doc.md")
                .unwrap()
                .len(),
            1
        );
        assert!(engine
            .list_backlinks(&space.slug, "Fake.md")
            .unwrap()
            .is_empty());
        assert!(engine
            .list_backlinks(&space.slug, "target.md")
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
        let replacement = std::fs::read_to_string(folder.join("invalid.md")).unwrap();
        assert!(replacement.contains("type: Note"));
        assert!(replacement.contains("original non-UTF-8 bytes"));
        assert_eq!(
            std::fs::read(folder.join(".cowiki/legacy/invalid.md.legacy")).unwrap(),
            [0xff, 0xfe, 0xfd]
        );
    }

    #[cfg(unix)]
    #[test]
    fn symbolic_links_never_escape_the_selected_space() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("knowledge");
        let outside = temp.path().join("outside");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let secret = "---\ntype: Note\n---\n\nSecret outside.\n";
        std::fs::write(outside.join("secret.md"), secret).unwrap();
        let space = engine.add_space("Knowledge", "knowledge", &folder).unwrap();
        symlink(&outside, folder.join("notes")).unwrap();
        symlink(outside.join("secret.md"), folder.join("linked.md")).unwrap();

        let tree = engine.list_pages(&space.slug).unwrap();
        assert!(!tree
            .iter()
            .any(|page| page.slug == "notes" || page.slug == "linked"));
        assert!(engine.get_page(&space.slug, "notes/secret").is_err());
        assert!(engine
            .write_page(&space.slug, "notes/secret", "changed")
            .is_err());
        assert!(engine
            .rename_path(&space.slug, "notes/secret.md", "moved.md")
            .is_err());
        assert!(engine.delete_path(&space.slug, "notes/secret.md").is_err());
        assert_eq!(
            std::fs::read_to_string(outside.join("secret.md")).unwrap(),
            secret
        );

        symlink(&outside, folder.join(".cowiki")).unwrap();
        assert!(engine
            .ingest(&space.slug, "text", "must stay inside", Some("source.md"))
            .is_err());
        assert!(!outside.join("sources/source.md").exists());
    }

    #[test]
    fn future_okf_bundles_are_read_only_and_never_silently_downgraded() {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let folder = temp.path().join("future");
        std::fs::create_dir_all(&folder).unwrap();
        let index = "---\nokf_version: \"0.2\"\n---\n\n# Future\n";
        let page = "---\ntype: Future\n---\n\nUnchanged.\n";
        std::fs::write(folder.join("index.md"), index).unwrap();
        std::fs::write(folder.join("page.md"), page).unwrap();
        let space = engine.add_space("Future", "future", &folder).unwrap();

        assert!(engine
            .write_page(&space.slug, "page", "---\ntype: Future\n---\nchanged")
            .is_err());
        assert!(engine
            .rename_path(&space.slug, "page.md", "renamed.md")
            .is_err());
        assert!(engine
            .ingest(&space.slug, "text", "source", Some("source.md"))
            .is_err());
        assert!(engine.create_folder(&space.slug, "New", None).is_err());
        assert!(engine.delete_path(&space.slug, "page.md").is_err());
        assert_eq!(
            std::fs::read_to_string(folder.join("index.md")).unwrap(),
            index
        );
        assert_eq!(
            std::fs::read_to_string(folder.join("page.md")).unwrap(),
            page
        );
    }
}
