use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::{AppError, Result};
use crate::AppState;

#[derive(Deserialize)]
pub struct ListParams {
    pub branch: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct PageListItem {
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub branch: String,
    /// "page" or "folder" (folder has _index.md)
    pub kind: String,
    /// For folders: child pages
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<PageListItem>,
}

#[derive(Serialize)]
pub struct PageResponse {
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub body: String,
    pub branch: String,
}

/// List pages as a tree by reading from Git recursively
pub async fn list_pages(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<PageListItem>>> {
    let branch = params.branch.unwrap_or_else(|| "main".into());

    let files = state
        .wiki_repo
        .list_files_recursive(&branch, "wiki")
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // Build tree from flat file list
    let mut root_items: Vec<PageListItem> = Vec::new();
    let mut folders: std::collections::HashMap<String, Vec<PageListItem>> = std::collections::HashMap::new();
    let mut folder_index: std::collections::HashMap<String, (String, String)> = std::collections::HashMap::new(); // dir → (title, summary)

    for file_path in &files {
        let rel = file_path.strip_prefix("wiki/").unwrap_or(file_path);
        let slug = rel.strip_suffix(".md").unwrap_or(rel).to_string();

        let (title, summary) = match state.wiki_repo.read_file(&branch, file_path) {
            Ok(Some(content)) => {
                let text = String::from_utf8_lossy(&content);
                parse_frontmatter(&text)
            }
            _ => ("Untitled".into(), String::new()),
        };

        let parts: Vec<&str> = slug.split('/').collect();

        if parts.len() == 1 {
            // Top-level page
            root_items.push(PageListItem {
                slug,
                title,
                summary,
                branch: branch.clone(),
                kind: "page".into(),
                children: Vec::new(),
            });
        } else {
            // Nested: e.g. "infra/docker" or "infra/_index"
            let dir = parts[..parts.len() - 1].join("/");
            let filename = *parts.last().unwrap();

            if filename == "_index" {
                // This is the folder's own content
                folder_index.insert(dir, (title, summary));
            } else {
                let item = PageListItem {
                    slug,
                    title,
                    summary,
                    branch: branch.clone(),
                    kind: "page".into(),
                    children: Vec::new(),
                };
                folders.entry(dir).or_default().push(item);
            }
        }
    }

    // Build folder items and merge children
    let mut folder_names: Vec<String> = folders.keys().chain(folder_index.keys()).cloned().collect();
    folder_names.sort();
    folder_names.dedup();

    for dir in folder_names {
        let (title, summary) = folder_index.remove(&dir).unwrap_or_else(|| {
            // No _index.md, use directory name as title
            let name = dir.rsplit('/').next().unwrap_or(&dir);
            (name.to_string(), String::new())
        });
        let children = folders.remove(&dir).unwrap_or_default();

        root_items.push(PageListItem {
            slug: format!("{dir}/_index"),
            title,
            summary,
            branch: branch.clone(),
            kind: "folder".into(),
            children,
        });
    }

    // Sort: folders first, then pages alphabetically
    root_items.sort_by(|a, b| {
        let a_folder = a.kind == "folder";
        let b_folder = b.kind == "folder";
        b_folder.cmp(&a_folder).then(a.title.cmp(&b.title))
    });

    Ok(Json(root_items))
}

pub async fn get_page(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    Query(params): Query<ListParams>,
) -> Result<Json<PageResponse>> {
    let branch = params.branch.unwrap_or_else(|| "main".into());
    let path = format!("wiki/{slug}.md");
    let content = state
        .wiki_repo
        .read_file(&branch, &path)
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("page {slug} not found")))?;
    let body = String::from_utf8_lossy(&content).into_owned();
    let (title, summary) = parse_frontmatter(&body);

    Ok(Json(PageResponse {
        slug,
        title,
        summary,
        body,
        branch,
    }))
}

fn parse_frontmatter(content: &str) -> (String, String) {
    if !content.starts_with("---") {
        return ("Untitled".into(), String::new());
    }
    let parts: Vec<&str> = content.splitn(3, "---").collect();
    if parts.len() < 3 {
        return ("Untitled".into(), String::new());
    }
    let fm = parts[1];
    let title = fm
        .lines()
        .find(|l| l.trim().starts_with("title:"))
        .map(|l| l.trim().trim_start_matches("title:").trim().trim_matches('"').to_string())
        .unwrap_or_else(|| "Untitled".into());
    let summary = fm
        .lines()
        .find(|l| l.trim().starts_with("summary:"))
        .map(|l| l.trim().trim_start_matches("summary:").trim().trim_matches('"').to_string())
        .unwrap_or_default();
    (title, summary)
}

#[derive(Deserialize)]
pub struct WritePage {
    pub slug: String,
    pub body: String,
    pub branch: String,
}

pub async fn write_page(
    State(state): State<Arc<AppState>>,
    Json(input): Json<WritePage>,
) -> Result<Json<serde_json::Value>> {
    let path = format!("wiki/{}.md", input.slug);
    state
        .wiki_repo
        .write_file(
            &input.branch,
            &path,
            input.body.as_bytes(),
            &format!("edit: {}", input.slug),
            &input.branch,
        )
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Json(serde_json::json!({"ok": true, "slug": input.slug})))
}

/// Create a folder (directory with _index.md)
#[derive(Deserialize)]
pub struct CreateFolder {
    pub name: String,
    pub parent: Option<String>, // parent directory path, e.g. "wiki/infra"
    pub branch: String,
}

pub async fn create_folder(
    State(state): State<Arc<AppState>>,
    Json(input): Json<CreateFolder>,
) -> Result<Json<serde_json::Value>> {
    let slug = input.name.to_lowercase()
        .replace(|c: char| !c.is_alphanumeric() && c != ' ', "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-");

    let dir = match &input.parent {
        Some(p) => format!("{p}/{slug}"),
        None => format!("wiki/{slug}"),
    };

    let index_path = format!("{dir}/_index.md");
    let body = format!(
        "---\ntitle: \"{}\"\nsummary: \"\"\nkind: overview\n---\n\n",
        input.name
    );

    state
        .wiki_repo
        .write_file(
            &input.branch,
            &index_path,
            body.as_bytes(),
            &format!("create folder: {}", input.name),
            &input.branch,
        )
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(serde_json::json!({"ok": true, "slug": format!("{slug}/_index"), "path": dir})))
}

// ── Workspace-scoped routes (use per-workspace repos) ──

pub async fn list_pages_ws(
    State(state): State<Arc<AppState>>,
    Path(ws_slug): Path<String>,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<PageListItem>>> {
    let repo = state.repo_manager.get(&ws_slug)
        .map_err(|e| AppError::Internal(format!("repo error: {e}")))?;
    let branch = params.branch.unwrap_or_else(|| "main".into());
    list_pages_from_repo(&repo, &branch)
}

pub async fn get_page_ws(
    State(state): State<Arc<AppState>>,
    Path((ws_slug, slug)): Path<(String, String)>,
    Query(params): Query<ListParams>,
) -> Result<Json<PageResponse>> {
    let repo = state.repo_manager.get(&ws_slug)
        .map_err(|e| AppError::Internal(format!("repo error: {e}")))?;
    let branch = params.branch.unwrap_or_else(|| "main".into());
    let path = format!("wiki/{slug}.md");
    let content = repo.read_file(&branch, &path)
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("page {slug} not found")))?;
    let body = String::from_utf8_lossy(&content).into_owned();
    let (title, summary) = parse_frontmatter(&body);
    Ok(Json(PageResponse { slug, title, summary, body, branch }))
}

pub async fn write_page_ws(
    State(state): State<Arc<AppState>>,
    Path(ws_slug): Path<String>,
    Json(input): Json<WritePage>,
) -> Result<Json<serde_json::Value>> {
    let repo = state.repo_manager.get(&ws_slug)
        .map_err(|e| AppError::Internal(format!("repo error: {e}")))?;
    let path = format!("wiki/{}.md", input.slug);
    repo.write_file(&input.branch, &path, input.body.as_bytes(), &format!("edit: {}", input.slug), &input.branch)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Json(serde_json::json!({"ok": true, "slug": input.slug})))
}

pub async fn create_folder_ws(
    State(state): State<Arc<AppState>>,
    Path(ws_slug): Path<String>,
    Json(input): Json<CreateFolder>,
) -> Result<Json<serde_json::Value>> {
    let repo = state.repo_manager.get(&ws_slug)
        .map_err(|e| AppError::Internal(format!("repo error: {e}")))?;
    let slug = input.name.to_lowercase()
        .replace(|c: char| !c.is_alphanumeric() && c != ' ', "")
        .split_whitespace().collect::<Vec<_>>().join("-");
    let dir = match &input.parent {
        Some(p) => format!("{p}/{slug}"),
        None => format!("wiki/{slug}"),
    };
    let index_path = format!("{dir}/_index.md");
    let body = format!("---\ntitle: \"{}\"\nsummary: \"\"\nkind: overview\n---\n\n", input.name);
    repo.write_file(&input.branch, &index_path, body.as_bytes(), &format!("create folder: {}", input.name), &input.branch)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Json(serde_json::json!({"ok": true, "slug": format!("{slug}/_index"), "path": dir})))
}

/// Internal: list pages from a specific repo instance
fn list_pages_from_repo(repo: &cowiki_core::git::WikiRepo, branch: &str) -> Result<Json<Vec<PageListItem>>> {
    let files = repo.list_files_recursive(branch, "wiki")
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let mut root_items: Vec<PageListItem> = Vec::new();
    let mut folders: std::collections::HashMap<String, Vec<PageListItem>> = std::collections::HashMap::new();
    let mut folder_index: std::collections::HashMap<String, (String, String)> = std::collections::HashMap::new();

    for file_path in &files {
        let rel = file_path.strip_prefix("wiki/").unwrap_or(file_path);
        let slug = rel.strip_suffix(".md").unwrap_or(rel).to_string();
        let (title, summary) = match repo.read_file(branch, file_path) {
            Ok(Some(content)) => parse_frontmatter(&String::from_utf8_lossy(&content)),
            _ => ("Untitled".into(), String::new()),
        };
        let parts: Vec<&str> = slug.split('/').collect();
        if parts.len() == 1 {
            root_items.push(PageListItem { slug, title, summary, branch: branch.into(), kind: "page".into(), children: Vec::new() });
        } else {
            let dir = parts[..parts.len() - 1].join("/");
            let filename = *parts.last().unwrap();
            if filename == "_index" {
                folder_index.insert(dir, (title, summary));
            } else {
                folders.entry(dir).or_default().push(PageListItem { slug, title, summary, branch: branch.into(), kind: "page".into(), children: Vec::new() });
            }
        }
    }
    let mut folder_names: Vec<String> = folders.keys().chain(folder_index.keys()).cloned().collect();
    folder_names.sort();
    folder_names.dedup();
    for dir in folder_names {
        let (title, summary) = folder_index.remove(&dir).unwrap_or_else(|| {
            let name = dir.rsplit('/').next().unwrap_or(&dir);
            (name.to_string(), String::new())
        });
        let children = folders.remove(&dir).unwrap_or_default();
        root_items.push(PageListItem { slug: format!("{dir}/_index"), title, summary, branch: branch.into(), kind: "folder".into(), children });
    }
    root_items.sort_by(|a, b| (b.kind == "folder").cmp(&(a.kind == "folder")).then(a.title.cmp(&b.title)));
    Ok(Json(root_items))
}
