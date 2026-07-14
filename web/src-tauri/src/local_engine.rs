use git2::{IndexAddOption, Repository, Signature, StatusOptions};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
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
    pub compiled: bool,
    pub compiled_pages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SourceContent {
    pub filename: String,
    pub content: String,
    pub compiled: bool,
    pub compiled_pages: Vec<String>,
}

pub struct LocalEngine {
    db: Mutex<Connection>,
}

impl LocalEngine {
    pub fn open(metadata_dir: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(metadata_dir).map_err(|e| e.to_string())?;
        let connection = Connection::open(metadata_dir.join("local.db")).map_err(|e| e.to_string())?;
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
        Ok(Self {
            db: Mutex::new(connection),
        })
    }

    pub fn list_spaces(&self) -> Result<Vec<Space>, String> {
        let db = self.db.lock().map_err(|_| "local database lock poisoned".to_string())?;
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
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
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
            self.insert_space(&name, &slug, &repo)?;
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

        self.insert_space(name, slug, &local_path)
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
        let db = self.db.lock().map_err(|_| "local database lock poisoned".to_string())?;
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

    pub fn write_page(
        &self,
        space_slug: &str,
        dir: &str,
        page_slug: &str,
        content: &str,
    ) -> Result<(), String> {
        let space = self.find_space(space_slug)?;
        let path = content_path(&space.local_path, dir, page_slug, true)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let temporary = path.with_extension("md.cowiki-tmp");
        std::fs::write(&temporary, content).map_err(|e| e.to_string())?;
        std::fs::rename(&temporary, &path).map_err(|e| e.to_string())
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
            path.file_stem().unwrap_or_default().to_string_lossy().to_string()
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
        if !root.is_dir() { return Ok(vec![]); }
        let mut sources = Vec::new();
        for entry in walkdir::WalkDir::new(root).into_iter().filter_map(Result::ok) {
            if entry.file_type().is_file() {
                sources.push(SourceItem {
                    filename: entry.path().file_name().unwrap_or_default().to_string_lossy().to_string(),
                    compiled: false,
                    compiled_pages: vec![],
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
            compiled: false,
            compiled_pages: vec![],
        })
    }

    pub fn commit_count(&self, space_slug: &str) -> Result<usize, String> {
        let repo = self.repo(space_slug)?;
        let mut walk = repo.revwalk().map_err(|e| e.to_string())?;
        if walk.push_head().is_err() {
            return Ok(0);
        }
        Ok(walk.filter_map(Result::ok).count())
    }

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

    pub fn submit(&self, space_slug: &str) -> Result<SubmitResult, String> {
        let repo = self.repo(space_slug)?;
        let mut index = repo.index().map_err(|e| e.to_string())?;
        index
            .add_all(["*"].iter(), IndexAddOption::DEFAULT, None)
            .map_err(|e| e.to_string())?;

        let mut status_options = StatusOptions::new();
        status_options
            .include_untracked(true)
            .recurse_untracked_dirs(true)
            .include_ignored(false);
        let statuses = repo
            .statuses(Some(&mut status_options))
            .map_err(|e| e.to_string())?;
        for status in statuses.iter() {
            if status.status().is_wt_deleted() {
                if let Some(path) = status.path() {
                    let _ = index.remove_path(Path::new(path));
                }
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
        let signature = Signature::now("CoWiki Local", "local@cowiki.app")
            .map_err(|e| e.to_string())?;
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

    fn repo(&self, space_slug: &str) -> Result<Repository, String> {
        let space = self.find_space(space_slug)?;
        Repository::open(&space.local_path).map_err(|e| e.to_string())
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
    Ok(if markdown { path.with_extension("md") } else { path })
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

fn read_page_tree(root: &Path, section: &str, current: &Path) -> Result<Vec<PageMeta>, String> {
    if !current.is_dir() { return Ok(vec![]); }
    let mut result = Vec::new();
    for entry in std::fs::read_dir(current).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || (current == root && ["entities", "concepts", "sources"].contains(&name.as_str())) {
            continue;
        }
        let relative = path.strip_prefix(root).map_err(|e| e.to_string())?;
        if path.is_dir() {
            let slug = relative.to_string_lossy().to_string();
            result.push(PageMeta {
                slug: slug.clone(), path: format!("{section}/{slug}"), title: name,
                summary: String::new(), branch: "local".into(), kind: "folder".into(),
                children: read_page_tree(root, section, &path)?,
            });
        } else if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("md")) {
            let body = std::fs::read_to_string(&path).unwrap_or_default();
            let slug = relative.with_extension("").to_string_lossy().to_string();
            result.push(PageMeta {
                slug: slug.clone(), path: format!("{section}/{slug}.md"),
                title: markdown_title(&body).unwrap_or_else(|| path.file_stem().unwrap_or_default().to_string_lossy().to_string()),
                summary: String::new(), branch: "local".into(), kind: "page".into(), children: vec![],
            });
        }
    }
    result.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase())));
    Ok(result)
}

fn markdown_title(body: &str) -> Option<String> {
    let mut in_frontmatter = false;
    for (index, line) in body.lines().enumerate() {
        if index == 0 && line.trim() == "---" { in_frontmatter = true; continue; }
        if in_frontmatter && line.trim() == "---" { in_frontmatter = false; continue; }
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
    chars.next().map(|first| first.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default()
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

        let result = engine.submit(&space.slug).unwrap();
        assert!(result.committed);
        assert_eq!(engine.commit_count(&space.slug).unwrap(), initial_commits + 1);
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
}
