use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path as FsPath;
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
    /// `page` for an OKF concept, or `folder` for a directory represented by index.md.
    pub kind: String,
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

#[derive(Deserialize)]
pub struct WritePage {
    pub slug: String,
    pub body: String,
    pub branch: String,
}

#[derive(Deserialize)]
pub struct CreateFolder {
    pub name: String,
    /// Bundle-relative parent directory. A legacy `wiki/` prefix is accepted during migration.
    pub parent: Option<String>,
    pub branch: String,
}

pub async fn list_pages(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<PageListItem>>> {
    let branch = params.branch.unwrap_or_else(|| "main".into());
    ensure_user_branch_if_needed(&state.wiki_repo, &branch)?;
    list_pages_from_repo(&state.wiki_repo, &branch)
}

pub async fn get_page(
    State(state): State<Arc<AppState>>,
    Path(raw_slug): Path<String>,
    Query(params): Query<ListParams>,
) -> Result<Json<PageResponse>> {
    let branch = params.branch.unwrap_or_else(|| "main".into());
    ensure_user_branch_if_needed(&state.wiki_repo, &branch)?;
    get_page_from_repo(&state.wiki_repo, raw_slug, branch)
}

pub async fn write_page(
    State(state): State<Arc<AppState>>,
    Json(input): Json<WritePage>,
) -> Result<Json<serde_json::Value>> {
    ensure_user_branch_if_needed(&state.wiki_repo, &input.branch)?;
    write_page_to_repo(&state.wiki_repo, input)
}

pub async fn create_folder(
    State(state): State<Arc<AppState>>,
    Json(input): Json<CreateFolder>,
) -> Result<Json<serde_json::Value>> {
    ensure_user_branch_if_needed(&state.wiki_repo, &input.branch)?;
    create_folder_in_repo(&state.wiki_repo, input)
}

pub async fn list_pages_ws(
    State(state): State<Arc<AppState>>,
    Path(ws_slug): Path<String>,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<PageListItem>>> {
    let repo = state
        .repo_manager
        .get(&ws_slug)
        .map_err(|error| AppError::Internal(format!("repo error: {error}")))?;
    let branch = params.branch.unwrap_or_else(|| "main".into());
    ensure_user_branch_if_needed(&repo, &branch)?;
    list_pages_from_repo(&repo, &branch)
}

pub async fn get_page_ws(
    State(state): State<Arc<AppState>>,
    Path((ws_slug, raw_slug)): Path<(String, String)>,
    Query(params): Query<ListParams>,
) -> Result<Json<PageResponse>> {
    let repo = state
        .repo_manager
        .get(&ws_slug)
        .map_err(|error| AppError::Internal(format!("repo error: {error}")))?;
    let branch = params.branch.unwrap_or_else(|| "main".into());
    ensure_user_branch_if_needed(&repo, &branch)?;
    get_page_from_repo(&repo, raw_slug, branch)
}

pub async fn write_page_ws(
    State(state): State<Arc<AppState>>,
    Path(ws_slug): Path<String>,
    Json(input): Json<WritePage>,
) -> Result<Json<serde_json::Value>> {
    let repo = state
        .repo_manager
        .get(&ws_slug)
        .map_err(|error| AppError::Internal(format!("repo error: {error}")))?;
    ensure_user_branch_if_needed(&repo, &input.branch)?;
    write_page_to_repo(&repo, input)
}

pub async fn create_folder_ws(
    State(state): State<Arc<AppState>>,
    Path(ws_slug): Path<String>,
    Json(input): Json<CreateFolder>,
) -> Result<Json<serde_json::Value>> {
    let repo = state
        .repo_manager
        .get(&ws_slug)
        .map_err(|error| AppError::Internal(format!("repo error: {error}")))?;
    ensure_user_branch_if_needed(&repo, &input.branch)?;
    create_folder_in_repo(&repo, input)
}

fn get_page_from_repo(
    repo: &cowiki_core::git::WikiRepo,
    raw_slug: String,
    branch: String,
) -> Result<Json<PageResponse>> {
    let slug = normalize_api_slug(raw_slug.trim_start_matches('/'));
    let path = document_path_for_slug(&slug)?;
    let content = repo
        .read_file(&branch, &path)
        .map_err(|error| AppError::Internal(error.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("page {slug} not found")))?;
    let body = String::from_utf8_lossy(&content).into_owned();
    let (title, summary) = parse_metadata(&path, &body);
    Ok(Json(PageResponse {
        slug,
        title,
        summary,
        body,
        branch,
    }))
}

fn write_page_to_repo(
    repo: &cowiki_core::git::WikiRepo,
    input: WritePage,
) -> Result<Json<serde_json::Value>> {
    let slug = normalize_api_slug(&input.slug);
    let path = document_path_for_slug(&slug)?;
    let fallback_title = path
        .rsplit('/')
        .next()
        .unwrap_or("Untitled")
        .trim_end_matches(".md");
    let normalized = match cowiki_core::okf::DocumentKind::from_path(&path) {
        cowiki_core::okf::DocumentKind::Concept => {
            cowiki_core::okf::normalize_concept_document(&input.body, fallback_title)
        }
        cowiki_core::okf::DocumentKind::Index => {
            cowiki_core::okf::normalize_index_document(&path, &input.body)
        }
        _ => Err("only OKF Markdown documents can be edited as pages".into()),
    }
    .map_err(AppError::BadRequest)?;
    repo.write_file(
        &input.branch,
        &path,
        normalized.as_bytes(),
        &format!("edit: {slug}"),
        &input.branch,
    )
    .map_err(|error| AppError::Internal(error.to_string()))?;
    Ok(Json(serde_json::json!({"ok": true, "slug": slug})))
}

fn create_folder_in_repo(
    repo: &cowiki_core::git::WikiRepo,
    input: CreateFolder,
) -> Result<Json<serde_json::Value>> {
    let folder_slug = slugify(&input.name);
    if folder_slug.is_empty() {
        return Err(AppError::BadRequest(
            "folder name must contain letters or numbers".into(),
        ));
    }
    let parent = input.parent.as_deref().map(normalize_parent).transpose()?;
    let dir = match parent.filter(|parent| !parent.is_empty()) {
        Some(parent) => format!("{parent}/{folder_slug}"),
        None => folder_slug.clone(),
    };
    let index_path = cowiki_core::okf::folder_index_path(&dir).map_err(AppError::BadRequest)?;
    let body = cowiki_core::okf::folder_index(&input.name);
    repo.write_file(
        &input.branch,
        &index_path,
        body.as_bytes(),
        &format!("create folder: {}", input.name),
        &input.branch,
    )
    .map_err(|error| AppError::Internal(error.to_string()))?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "slug": format!("{dir}/index"),
        "path": dir,
    })))
}

fn normalize_parent(parent: &str) -> Result<String> {
    let parent = parent
        .trim_matches('/')
        .strip_prefix("wiki/")
        .unwrap_or(parent.trim_matches('/'))
        .trim_end_matches("/index")
        .trim_end_matches("/_index");
    if parent.is_empty() {
        return Ok(String::new());
    }
    let probe = cowiki_core::okf::folder_index_path(parent).map_err(AppError::BadRequest)?;
    Ok(probe.trim_end_matches("/index.md").to_string())
}

fn document_path_for_slug(slug: &str) -> Result<String> {
    if slug == "index" {
        return Ok("index.md".into());
    }
    if let Some(folder) = slug.strip_suffix("/index") {
        return cowiki_core::okf::folder_index_path(folder).map_err(AppError::BadRequest);
    }
    cowiki_core::okf::concept_path(slug).map_err(AppError::BadRequest)
}

fn normalize_api_slug(slug: &str) -> String {
    slug.strip_suffix("/_index")
        .map(|folder| format!("{folder}/index"))
        .unwrap_or_else(|| slug.to_string())
}

fn list_pages_from_repo(
    repo: &cowiki_core::git::WikiRepo,
    branch: &str,
) -> Result<Json<Vec<PageListItem>>> {
    let files = repo
        .list_files_recursive(branch, "")
        .map_err(|error| AppError::Internal(error.to_string()))?;
    let mut concepts: HashMap<String, (String, String)> = HashMap::new();
    let mut indexes: HashMap<String, (String, String)> = HashMap::new();
    let mut directories = HashSet::new();

    for path in files {
        match cowiki_core::okf::DocumentKind::from_path(&path) {
            cowiki_core::okf::DocumentKind::Concept => {
                let slug = path.trim_end_matches(".md").to_string();
                let content = repo
                    .read_file(branch, &path)
                    .ok()
                    .flatten()
                    .unwrap_or_default();
                concepts.insert(
                    slug.clone(),
                    parse_metadata(&path, &String::from_utf8_lossy(&content)),
                );
                add_parent_directories(&slug, &mut directories);
            }
            cowiki_core::okf::DocumentKind::Index if path != "index.md" => {
                let dir = path.trim_end_matches("/index.md").to_string();
                let content = repo
                    .read_file(branch, &path)
                    .ok()
                    .flatten()
                    .unwrap_or_default();
                indexes.insert(
                    dir.clone(),
                    parse_metadata(&path, &String::from_utf8_lossy(&content)),
                );
                add_parent_directories(&dir, &mut directories);
                directories.insert(dir);
            }
            _ => {}
        }
    }

    fn build_level(
        parent: &str,
        branch: &str,
        concepts: &HashMap<String, (String, String)>,
        indexes: &HashMap<String, (String, String)>,
        directories: &HashSet<String>,
    ) -> Vec<PageListItem> {
        let mut result = Vec::new();
        for (slug, (title, summary)) in concepts {
            let concept_parent = slug
                .rsplit_once('/')
                .map(|(parent, _)| parent)
                .unwrap_or("");
            if concept_parent == parent {
                result.push(PageListItem {
                    slug: slug.clone(),
                    title: title.clone(),
                    summary: summary.clone(),
                    branch: branch.into(),
                    kind: "page".into(),
                    children: Vec::new(),
                });
            }
        }
        for dir in directories {
            let dir_parent = dir.rsplit_once('/').map(|(parent, _)| parent).unwrap_or("");
            if dir_parent != parent {
                continue;
            }
            let fallback = dir.rsplit('/').next().unwrap_or(dir);
            let (title, summary) = indexes
                .get(dir)
                .cloned()
                .unwrap_or_else(|| (humanize(fallback), String::new()));
            result.push(PageListItem {
                slug: format!("{dir}/index"),
                title,
                summary,
                branch: branch.into(),
                kind: "folder".into(),
                children: build_level(dir, branch, concepts, indexes, directories),
            });
        }
        result.sort_by(|a, b| {
            (b.kind == "folder")
                .cmp(&(a.kind == "folder"))
                .then_with(|| a.title.cmp(&b.title))
        });
        result
    }

    Ok(Json(build_level(
        "",
        branch,
        &concepts,
        &indexes,
        &directories,
    )))
}

fn add_parent_directories(path: &str, directories: &mut HashSet<String>) {
    let parts = path.split('/').collect::<Vec<_>>();
    for depth in 1..parts.len() {
        directories.insert(parts[..depth].join("/"));
    }
}

fn parse_metadata(path: &str, content: &str) -> (String, String) {
    if cowiki_core::okf::DocumentKind::from_path(path) == cowiki_core::okf::DocumentKind::Index {
        let heading = content
            .lines()
            .find_map(|line| line.strip_prefix("# "))
            .map(str::trim)
            .filter(|heading| !heading.is_empty());
        let fallback = FsPath::new(path)
            .parent()
            .and_then(FsPath::file_name)
            .and_then(|name| name.to_str())
            .unwrap_or("Knowledge");
        return (
            heading
                .map(str::to_string)
                .unwrap_or_else(|| humanize(fallback)),
            String::new(),
        );
    }
    let frontmatter = content
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---"))
        .map(|(frontmatter, _)| frontmatter)
        .unwrap_or("");
    let title = yaml_scalar(frontmatter, "title").unwrap_or_else(|| {
        humanize(
            FsPath::new(path)
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("Untitled"),
        )
    });
    let summary = yaml_scalar(frontmatter, "description")
        .or_else(|| yaml_scalar(frontmatter, "summary"))
        .unwrap_or_default();
    (title, summary)
}

fn yaml_scalar(frontmatter: &str, key: &str) -> Option<String> {
    frontmatter.lines().find_map(|line| {
        let value = line.trim().strip_prefix(&format!("{key}:"))?.trim();
        Some(value.trim_matches(['\'', '"']).to_string())
    })
}

fn humanize(value: &str) -> String {
    value
        .replace(['-', '_'], " ")
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn slugify(value: &str) -> String {
    value
        .to_lowercase()
        .replace(
            |character: char| !character.is_alphanumeric() && character != ' ',
            "",
        )
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
}

/// Ensure user branch exists lazily (for workspace repos and legacy routes).
pub(crate) fn ensure_user_branch_if_needed(
    repo: &cowiki_core::git::WikiRepo,
    branch: &str,
) -> Result<()> {
    if let Some(user_id) = branch.strip_prefix("user/") {
        repo.ensure_user_branch(user_id).map_err(|error| {
            AppError::Internal(format!("failed to ensure user branch '{branch}': {error}"))
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod okf_tests {
    use super::*;

    #[test]
    fn lists_root_concepts_and_okf_folder_indexes() {
        let data = tempfile::tempdir().unwrap();
        let repo = cowiki_core::git::WikiRepo::open_or_init(data.path().to_str().unwrap()).unwrap();
        repo.write_file(
            "main",
            "welcome.md",
            b"---\ntype: Note\ntitle: Welcome\ndescription: Start here.\n---\n",
            "add welcome",
            "test",
        )
        .unwrap();
        repo.write_file("main", "teams/index.md", b"# Teams\n", "add folder", "test")
            .unwrap();
        repo.write_file(
            "main",
            "teams/customer.md",
            b"---\ntype: Entity\ntitle: Customer\n---\n",
            "add customer",
            "test",
        )
        .unwrap();

        let Json(items) = list_pages_from_repo(&repo, "main").unwrap();
        assert_eq!(items.len(), 2);
        let folder = items.iter().find(|item| item.kind == "folder").unwrap();
        assert_eq!(folder.slug, "teams/index");
        assert_eq!(folder.title, "Teams");
        assert_eq!(folder.children[0].slug, "teams/customer");
        let page = items.iter().find(|item| item.kind == "page").unwrap();
        assert_eq!(page.slug, "welcome");
        assert_eq!(page.summary, "Start here.");
    }
}
