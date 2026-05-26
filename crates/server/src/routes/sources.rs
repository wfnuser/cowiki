use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::error::{AppError, Result};
use crate::AppState;

#[derive(Deserialize)]
pub struct ListSourcesParams {
    #[serde(default = "default_branch")]
    pub branch: String,
}

fn default_branch() -> String {
    "main".into()
}

#[derive(Deserialize)]
pub struct GetSourceParams {
    #[serde(default = "default_branch")]
    pub branch: String,
}

#[derive(Serialize)]
pub struct SourceItem {
    pub filename: String,
    pub compiled: bool,
    pub compiled_pages: Vec<String>,
}

#[derive(Serialize)]
pub struct SourceContent {
    pub filename: String,
    pub content: String,
    pub compiled: bool,
    pub compiled_pages: Vec<String>,
}

fn load_compile_state(repo: &cowiki_core::git::WikiRepo, branch: &str) -> HashMap<String, String> {
    #[derive(Deserialize, Default)]
    struct CompileState {
        sources: HashMap<String, String>,
    }
    repo.read_file(branch, ".cowiki/state.json")
        .ok()
        .flatten()
        .and_then(|bytes| serde_json::from_slice::<CompileState>(&bytes).ok())
        .map(|s| s.sources)
        .unwrap_or_default()
}

fn find_referencing_pages(
    repo: &cowiki_core::git::WikiRepo,
    branch: &str,
    source_filename: &str,
) -> Vec<String> {
    let wiki_files = repo.list_files(branch, "wiki").unwrap_or_default();
    let mut pages = Vec::new();
    for file in &wiki_files {
        if let Ok(Some(content)) = repo.read_file(branch, file) {
            let text = String::from_utf8_lossy(&content);
            if text.contains(&format!("  - {source_filename}"))
                || text.contains(&format!("- {source_filename}"))
            {
                let slug = file
                    .strip_prefix("wiki/")
                    .unwrap_or(file)
                    .strip_suffix(".md")
                    .unwrap_or(file)
                    .to_string();
                pages.push(slug);
            }
        }
    }
    pages
}

pub async fn list_sources(
    State(state): State<Arc<AppState>>,
    Path(ws_slug): Path<String>,
    Query(params): Query<ListSourcesParams>,
) -> Result<Json<Vec<SourceItem>>> {
    let repo = state
        .repo_manager
        .get(&ws_slug)
        .map_err(|e| AppError::Internal(format!("repo error: {e}")))?;

    let branch = &params.branch;
    let compile_state = load_compile_state(&repo, branch);
    let source_files = repo
        .list_files_recursive(branch, "sources")
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let mut items = Vec::new();
    for file in &source_files {
        let filename = file
            .strip_prefix("sources/")
            .unwrap_or(file)
            .to_string();
        let compiled = compile_state.contains_key(&filename);
        let compiled_pages = if compiled {
            find_referencing_pages(&repo, branch, &filename)
        } else {
            Vec::new()
        };
        items.push(SourceItem { filename, compiled, compiled_pages });
    }
    Ok(Json(items))
}

pub async fn get_source(
    State(state): State<Arc<AppState>>,
    Path((ws_slug, filename)): Path<(String, String)>,
    Query(params): Query<GetSourceParams>,
) -> Result<Json<SourceContent>> {
    // Path traversal protection
    if filename.is_empty()
        || filename.contains("..")
        || filename.contains('/')
        || filename.contains('\\')
        || filename.starts_with('.')
        || filename.len() > 255
    {
        return Err(AppError::BadRequest("invalid filename".into()));
    }

    let repo = state
        .repo_manager
        .get(&ws_slug)
        .map_err(|e| AppError::Internal(format!("repo error: {e}")))?;

    let branch = &params.branch;
    let file_path = format!("sources/{filename}");
    let content_bytes = repo
        .read_file(branch, &file_path)
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("source file not found: {filename}")))?;

    let content = String::from_utf8_lossy(&content_bytes).into_owned();
    let compile_state = load_compile_state(&repo, branch);
    let compiled = compile_state.contains_key(&filename);
    let compiled_pages = if compiled {
        find_referencing_pages(&repo, branch, &filename)
    } else {
        Vec::new()
    };

    Ok(Json(SourceContent { filename, content, compiled, compiled_pages }))
}
