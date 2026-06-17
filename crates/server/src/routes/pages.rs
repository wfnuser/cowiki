use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::{AppError, Result};
use crate::AppState;

#[derive(Deserialize)]
pub struct PageQueryParams {
    pub branch: Option<String>,
    /// Content directory: "wiki" (default), "entities", "concepts", "all"
    pub dir: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct PageListItem {
    pub slug: String,
    pub path: String,
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

pub(crate) fn parse_frontmatter(content: &str) -> (Option<String>, String) {
    if content.starts_with("---") {
        let parts: Vec<&str> = content.splitn(3, "---").collect();
        if parts.len() >= 3 {
            let fm = parts[1];
            let title = fm
                .lines()
                .find(|l| l.trim().starts_with("title:"))
                .map(|l| {
                    l.trim()
                        .trim_start_matches("title:")
                        .trim()
                        .trim_matches('"')
                        .to_string()
                })
                .filter(|title| !title.trim().is_empty());
            let summary = fm
                .lines()
                .find(|l| l.trim().starts_with("summary:"))
                .map(|l| {
                    l.trim()
                        .trim_start_matches("summary:")
                        .trim()
                        .trim_matches('"')
                        .to_string()
                })
                .unwrap_or_default();
            if title.is_some() {
                return (title, summary);
            }
        }
    }

    // Fallback: first Markdown heading (# Title or ## Title)
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(title) = trimmed
            .strip_prefix("# ")
            .or_else(|| trimmed.strip_prefix("## "))
        {
            let title = title.trim().to_string();
            if !title.is_empty() {
                return (Some(title), String::new());
            }
        }
    }

    (None, String::new())
}

/// Strict frontmatter parsing: only checks the YAML frontmatter block for a
/// non-empty `title:` field. No heading/slug fallback — use this for write
/// validation where a frontmatter title is mandatory.
pub(crate) fn parse_frontmatter_strict(content: &str) -> (Option<String>, String) {
    if !content.starts_with("---") {
        return (None, String::new());
    }
    let parts: Vec<&str> = content.splitn(3, "---").collect();
    if parts.len() < 3 {
        return (None, String::new());
    }
    let fm = parts[1];
    let title = fm
        .lines()
        .find(|l| l.trim().starts_with("title:"))
        .map(|l| {
            l.trim()
                .trim_start_matches("title:")
                .trim()
                .trim_matches('"')
                .to_string()
        })
        .filter(|title| !title.trim().is_empty());
    let summary = fm
        .lines()
        .find(|l| l.trim().starts_with("summary:"))
        .map(|l| {
            l.trim()
                .trim_start_matches("summary:")
                .trim()
                .trim_matches('"')
                .to_string()
        })
        .unwrap_or_default();
    (title, summary)
}

pub(crate) fn require_page_title(content: &str) -> Result<(String, String)> {
    let (title, summary) = parse_frontmatter_strict(content);
    let title = title.ok_or_else(|| {
        AppError::BadRequest("wiki pages require non-empty frontmatter.title".into())
    })?;
    Ok((title, summary))
}

fn fallback_title_from_content_or_slug(content: &str, slug: &str) -> String {
    content
        .lines()
        .find_map(|line| line.trim().strip_prefix("# "))
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| title_from_slug(slug))
}

fn title_from_slug(slug: &str) -> String {
    let base = slug
        .trim_end_matches("/_index")
        .rsplit('/')
        .next()
        .unwrap_or(slug);
    let title = base
        .replace(['-', '_'], " ")
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    if title.is_empty() {
        "Untitled".into()
    } else {
        title
    }
}

#[derive(Deserialize)]
pub struct WritePage {
    pub slug: String,
    pub body: String,
    pub branch: String,
    /// Content directory: "wiki" (default), "entities", "concepts"
    pub dir: Option<String>,
    /// Optional title — if set, server prepends YAML frontmatter
    pub title: Option<String>,
    /// Optional summary for YAML frontmatter
    pub summary: Option<String>,
}

/// Create a folder (directory with _index.md)
#[derive(Deserialize)]
pub struct CreateFolder {
    pub name: String,
    pub parent: Option<String>, // parent directory path, e.g. "wiki/infra"
    pub branch: String,
}

// ── Workspace-scoped routes (use per-workspace repos) ──

pub async fn list_pages_ws(
    State(state): State<Arc<AppState>>,
    Path(ws_slug): Path<String>,
    Query(params): Query<PageQueryParams>,
) -> Result<Json<Vec<PageListItem>>> {
    let repo = state
        .repo_manager
        .get(&ws_slug)
        .map_err(|e| AppError::Internal(format!("repo error: {e}")))?;
    let branch = params.branch.unwrap_or_else(|| "main".into());
    ensure_user_branch_if_needed(&repo, &branch)?;

    let dir = params.dir.as_deref().unwrap_or("wiki");

    if dir == "all" {
        return list_pages_all_dirs(&repo, &branch);
    }

    cowiki_core::wiki_fs::validate_dir(dir).map_err(|e| AppError::BadRequest(e))?;

    let files = cowiki_core::wiki_fs::list_pages_recursive(&repo, &branch, dir)
        .map_err(|e| AppError::Internal(e))?;
    list_pages_from_dir(&repo, &branch, dir, &files)
}

pub async fn get_page_ws(
    State(state): State<Arc<AppState>>,
    Path((ws_slug, raw_slug)): Path<(String, String)>,
    Query(params): Query<PageQueryParams>,
) -> Result<Json<PageResponse>> {
    let slug = raw_slug.strip_prefix('/').unwrap_or(&raw_slug).to_string();
    let repo = state
        .repo_manager
        .get(&ws_slug)
        .map_err(|e| AppError::Internal(format!("repo error: {e}")))?;
    let branch = params.branch.unwrap_or_else(|| "main".into());
    ensure_user_branch_if_needed(&repo, &branch)?;
    let dir = params.dir.as_deref().unwrap_or("wiki");
    if dir == "all" {
        return Err(AppError::BadRequest(
            "dir=all is only supported for listing. Use a specific directory to read.".into(),
        ));
    }
    cowiki_core::wiki_fs::validate_dir(dir).map_err(|e| AppError::BadRequest(e))?;
    let content = cowiki_core::wiki_fs::read_page(&repo, &branch, dir, &slug)
        .map_err(|e| AppError::Internal(e))?
        .ok_or_else(|| AppError::NotFound(format!("page {slug} not found in {dir}")))?;
    let body = String::from_utf8_lossy(&content).into_owned();
    let (title, summary) = parse_frontmatter(&body);
    let title = title.unwrap_or_else(|| fallback_title_from_content_or_slug(&body, &slug));
    Ok(Json(PageResponse {
        slug,
        title,
        summary,
        body,
        branch,
    }))
}

pub async fn write_page_ws(
    State(state): State<Arc<AppState>>,
    Path(ws_slug): Path<String>,
    headers: axum::http::HeaderMap,
    Json(input): Json<WritePage>,
) -> Result<Json<serde_json::Value>> {
    // Writes go through membership + write permission and only ever land on the caller's
    // own draft branch; main changes exclusively via merge_pr.
    let guard = crate::routes::guard::require_membership(&state, &headers, &ws_slug).await?;
    crate::routes::guard::require(&guard, crate::routes::guard::Permission::EditContent)?;
    require_own_branch(&input.branch, guard.user.id)?;
    let repo = state
        .repo_manager
        .get(&ws_slug)
        .map_err(|e| AppError::Internal(format!("repo error: {e}")))?;
    ensure_user_branch_if_needed(&repo, &input.branch)?;
    let dir = input.dir.as_deref().unwrap_or("wiki");
    if dir == "all" {
        return Err(AppError::BadRequest(
            "dir=all is only supported for listing. Use a specific directory to write.".into(),
        ));
    }
    cowiki_core::wiki_fs::validate_dir(dir).map_err(|e| AppError::BadRequest(e))?;

    // If title is provided, prepend YAML frontmatter to the body
    let final_body = if let Some(ref title) = input.title {
        let summary = input.summary.as_deref().unwrap_or("");
        format!(
            "---\ntitle: \"{}\"\nsummary: \"{}\"\n---\n\n{}",
            title, summary, input.body
        )
    } else {
        input.body.clone()
    };

    require_page_title(&final_body)?;

    cowiki_core::wiki_fs::write_page(
        &repo,
        &input.branch,
        dir,
        &input.slug,
        final_body.as_bytes(),
        &guard.user.name,
    )
    .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Json(
        serde_json::json!({"ok": true, "slug": input.slug, "path": format!("{dir}/{}", input.slug)}),
    ))
}

pub async fn create_folder_ws(
    State(state): State<Arc<AppState>>,
    Path(ws_slug): Path<String>,
    headers: axum::http::HeaderMap,
    Json(input): Json<CreateFolder>,
) -> Result<Json<serde_json::Value>> {
    let guard = crate::routes::guard::require_membership(&state, &headers, &ws_slug).await?;
    crate::routes::guard::require(&guard, crate::routes::guard::Permission::EditContent)?;
    require_own_branch(&input.branch, guard.user.id)?;
    let repo = state
        .repo_manager
        .get(&ws_slug)
        .map_err(|e| AppError::Internal(format!("repo error: {e}")))?;
    ensure_user_branch_if_needed(&repo, &input.branch)?;
    let slug = input
        .name
        .to_lowercase()
        .replace(|c: char| !c.is_alphanumeric() && c != ' ', "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-");
    let parent = input.parent.as_deref().ok_or_else(|| {
        AppError::BadRequest(
            "parent is required (e.g. wiki, entities, entities/people)".into(),
        )
    })?;
    let dir = format!("{parent}/{slug}");
    let index_path = format!("{dir}/_index.md");
    let body = format!(
        "---\ntitle: \"{}\"\nsummary: \"\"\nkind: overview\n---\n\n",
        input.name
    );
    repo.write_file(
        &input.branch,
        &index_path,
        body.as_bytes(),
        &format!("create folder: {}", input.name),
        &guard.user.name,
    )
    .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Json(
        serde_json::json!({"ok": true, "slug": format!("{slug}/_index"), "path": dir}),
    ))
}

/// Ensure user branch exists lazily (for workspace repos and legacy routes)
pub(crate) fn ensure_user_branch_if_needed(
    repo: &cowiki_core::git::WikiRepo,
    branch: &str,
) -> Result<()> {
    if let Some(user_id) = branch.strip_prefix("user/") {
        repo.ensure_user_branch(user_id).map_err(|e| {
            AppError::Internal(format!("failed to ensure user branch '{branch}': {e}"))
        })?;
    }
    Ok(())
}

/// Given a full directory path and a parent directory, returns the immediate
/// child directory relative to parent. Returns None if the item is not under
/// the parent or is directly at the parent level.
///
/// Examples:
/// - ("entities/projects", "") → Some("entities")
/// - ("entities/projects/alpha", "entities") → Some("entities/projects")
/// - ("wiki/page", "") → Some("wiki")
fn immediate_child_dir(item_dir: &str, parent_dir: &str) -> Option<String> {
    if item_dir.is_empty() {
        return None;
    }
    if parent_dir.is_empty() {
        // Top level: first segment is the child dir
        let first = item_dir.split('/').next().unwrap_or("");
        if first.is_empty() {
            None
        } else {
            Some(first.to_string())
        }
    } else if item_dir == parent_dir {
        None // same directory, not a child
    } else if let Some(rest) = item_dir.strip_prefix(&format!("{parent_dir}/")) {
        let next = rest.split('/').next().unwrap_or("");
        if next.is_empty() {
            None
        } else {
            Some(format!("{parent_dir}/{next}"))
        }
    } else {
        None // not under parent_dir
    }
}

/// Internal: list pages from a specific directory, building a tree.
fn list_pages_from_dir(
    repo: &cowiki_core::git::WikiRepo,
    branch: &str,
    dir: &str,
    files: &[String],
) -> Result<Json<Vec<PageListItem>>> {
    // Collect all items: pages and folder _index metadata
    struct RawItem {
        slug: String,
        title: String,
        summary: String,
        is_index: bool, // true if this is a folder's _index.md
        dir: String,    // parent directory path (e.g. "test-folder" or "test-folder/sub")
    }

    let mut items: Vec<RawItem> = Vec::new();

    for file_path in files {
        let prefix = format!("{dir}/");
        let rel = file_path.strip_prefix(&prefix).unwrap_or(file_path);
        let slug = rel.strip_suffix(".md").unwrap_or(rel).to_string();
        let (title, summary) = match repo.read_file(branch, file_path) {
            Ok(Some(content)) => {
                let body = String::from_utf8_lossy(&content);
                let (title, summary) = parse_frontmatter(&body);
                let title =
                    title.unwrap_or_else(|| fallback_title_from_content_or_slug(&body, &slug));
                (title, summary)
            }
            _ => {
                return Err(AppError::BadRequest(format!(
                    "{file_path} could not be read"
                )));
            }
        };
        let parts: Vec<&str> = slug.split('/').collect();
        if parts.len() == 1 {
            items.push(RawItem {
                slug: slug.clone(),
                title,
                summary,
                is_index: false,
                dir: String::new(),
            });
        } else {
            let dir = parts[..parts.len() - 1].join("/");
            let filename = *parts.last().unwrap();
            items.push(RawItem {
                slug: slug.clone(),
                title,
                summary,
                is_index: filename == "_index",
                dir,
            });
        }
    }

    // Build tree recursively
    fn build_level(items: &[RawItem], parent_dir: &str, branch: &str, dir: &str) -> Vec<PageListItem> {
        let mut result: Vec<PageListItem> = Vec::new();

        // Find pages directly in this directory (not _index)
        for item in items {
            if item.dir == parent_dir && !item.is_index {
                result.push(PageListItem {
                    slug: item.slug.clone(),
                    // item.slug is the full relative path under dir (e.g. "people/alice"),
                    // so dir + slug always produces the correct full path.
                    path: format!("{}/{}", dir, item.slug),
                    title: item.title.clone(),
                    summary: item.summary.clone(),
                    branch: branch.into(),
                    kind: "page".into(),
                    children: Vec::new(),
                });
            }
        }

        // Find subdirectories (folders that have _index or have children at this level)
        let mut subdirs: Vec<String> = Vec::new();
        for item in items {
            if let Some(child) = immediate_child_dir(&item.dir, parent_dir) {
                if !subdirs.contains(&child) {
                    subdirs.push(child);
                }
            }
        }
        subdirs.sort();
        subdirs.dedup();

        for subdir in subdirs {
            // Find _index for this subdir
            let (title, summary) = items
                .iter()
                .find(|i| i.dir == subdir && i.is_index)
                .map(|i| (i.title.clone(), i.summary.clone()))
                .unwrap_or_else(|| {
                    let name = subdir.rsplit('/').next().unwrap_or(&subdir);
                    (name.to_string(), String::new())
                });

            let children = build_level(items, &subdir, branch, dir);
            result.push(PageListItem {
                slug: format!("{subdir}/_index"),
                path: format!("{dir}/{subdir}/_index"),
                title,
                summary,
                branch: branch.into(),
                kind: "folder".into(),
                children,
            });
        }

        // Sort: folders first, then alphabetically
        result.sort_by(|a, b| {
            (b.kind == "folder")
                .cmp(&(a.kind == "folder"))
                .then(a.title.cmp(&b.title))
        });
        result
    }

    let tree = build_level(&items, "", branch, dir);
    Ok(Json(tree))
}

// ── Path operations: rename / delete (files and folders under wiki/) ──

/// Validate a repo path for destructive ops: must be inside a known content directory
/// (`wiki/`, `entities/`, `concepts/`), no traversal, no empty segments, and never the
/// root itself.
fn validate_wiki_path(p: &str) -> Result<()> {
    let allowed = cowiki_core::wiki_fs::all_dirs();
    let ok = allowed.iter().any(|d| p.starts_with(&format!("{d}/")))
        && !p.ends_with('/')
        && p.split('/')
            .all(|seg| !seg.is_empty() && seg != "." && seg != "..");
    if !ok {
        return Err(AppError::BadRequest(format!(
            "invalid path '{p}': paths must be inside one of {:?}",
            allowed
        )));
    }
    Ok(())
}

/// Mutating ops only touch the caller's own draft branch — never main, pr/* snapshots,
/// or other users' branches (all writes flow to main exclusively through merge_pr).
pub(crate) fn require_own_branch(branch: &str, user_id: uuid::Uuid) -> Result<()> {
    if branch != format!("user/{user_id}") {
        return Err(AppError::Forbidden(
            "writes are only allowed on your own draft branch".into(),
        ));
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct RenamePathRequest {
    pub branch: String,
    pub from: String,
    pub to: String,
}

pub async fn rename_path_ws(
    State(state): State<Arc<AppState>>,
    Path(ws_slug): Path<String>,
    headers: axum::http::HeaderMap,
    Json(input): Json<RenamePathRequest>,
) -> Result<Json<serde_json::Value>> {
    let guard = crate::routes::guard::require_membership(&state, &headers, &ws_slug).await?;
    crate::routes::guard::require(&guard, crate::routes::guard::Permission::EditContent)?;
    require_own_branch(&input.branch, guard.user.id)?;
    validate_wiki_path(&input.from)?;
    validate_wiki_path(&input.to)?;

    let repo = state
        .repo_manager
        .get(&ws_slug)
        .map_err(|e| AppError::Internal(format!("repo error: {e}")))?;
    ensure_user_branch_if_needed(&repo, &input.branch)?;
    repo.rename_path(
        &input.branch,
        &input.from,
        &input.to,
        &format!("rename: {} -> {}", input.from, input.to),
        &guard.user.name,
    )
    .map_err(|e| AppError::BadRequest(e.to_string()))?;
    Ok(Json(serde_json::json!({"ok": true})))
}

#[derive(Deserialize)]
pub struct DeletePathRequest {
    pub branch: String,
    pub path: String,
}

pub async fn delete_path_ws(
    State(state): State<Arc<AppState>>,
    Path(ws_slug): Path<String>,
    headers: axum::http::HeaderMap,
    Json(input): Json<DeletePathRequest>,
) -> Result<Json<serde_json::Value>> {
    let guard = crate::routes::guard::require_membership(&state, &headers, &ws_slug).await?;
    crate::routes::guard::require(&guard, crate::routes::guard::Permission::EditContent)?;
    require_own_branch(&input.branch, guard.user.id)?;
    validate_wiki_path(&input.path)?;

    let repo = state
        .repo_manager
        .get(&ws_slug)
        .map_err(|e| AppError::Internal(format!("repo error: {e}")))?;
    ensure_user_branch_if_needed(&repo, &input.branch)?;
    repo.delete_path(
        &input.branch,
        &input.path,
        &format!("delete: {}", input.path),
        &guard.user.name,
    )
    .map_err(|e| AppError::BadRequest(e.to_string()))?;
    Ok(Json(serde_json::json!({"ok": true})))
}

/// List pages across all content directories, building a merged tree
/// where each directory becomes a top-level folder node.
fn list_pages_all_dirs(
    repo: &cowiki_core::git::WikiRepo,
    branch: &str,
) -> Result<Json<Vec<PageListItem>>> {
    let mut tree: Vec<PageListItem> = Vec::new();

    for dir in cowiki_core::wiki_fs::all_dirs() {
        let files = match cowiki_core::wiki_fs::list_pages_recursive(repo, branch, dir) {
            Ok(f) => f,
            Err(_) => continue,
        };

        if files.is_empty() {
            continue;
        }

        // Build the subtree for this directory using the shared helper
        let subtree_json = list_pages_from_dir(repo, branch, dir, &files)?;
        let children = subtree_json.0;

        let dir_node = PageListItem {
            slug: format!("{dir}/_index"),
            path: format!("{dir}/_index"),
            title: dir.to_string(),
            summary: String::new(),
            branch: branch.into(),
            kind: "folder".into(),
            children,
        };
        tree.push(dir_node);
    }

    Ok(Json(tree))
}
