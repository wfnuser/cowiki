use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
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

fn load_compile_state(
    repo: &cowiki_core::git::WikiRepo,
    branch: &str,
) -> cowiki_core::compile::CompileState {
    repo.read_file(branch, ".cowiki/state.json")
        .ok()
        .flatten()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub async fn list_sources(
    State(state): State<Arc<AppState>>,
    Path(ws_slug): Path<String>,
    headers: axum::http::HeaderMap,
    Query(params): Query<ListSourcesParams>,
) -> Result<Json<Vec<SourceItem>>> {
    let guard = crate::routes::guard::require_membership(&state, &headers, &ws_slug).await?;
    crate::routes::guard::require(&guard, crate::routes::guard::Permission::ViewContent)?;
    super::guard::require_readable_branch(&params.branch, guard.user.id)?;
    let repo = state
        .repo_manager
        .get(&ws_slug)
        .map_err(|e| AppError::Internal(format!("repo error: {e}")))?;

    let branch = &params.branch;
    let compile_state = load_compile_state(&repo, branch);
    let source_files = repo
        .list_files(branch, "sources")
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let mut items = Vec::new();
    for file in &source_files {
        let filename = file.strip_prefix("sources/").unwrap_or(file).to_string();
        let compiled = compile_state.sources.contains_key(&filename);
        let compiled_pages = if compiled {
            compile_state
                .source_pages
                .get(&filename)
                .cloned()
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        items.push(SourceItem {
            filename,
            compiled,
            compiled_pages,
        });
    }
    Ok(Json(items))
}

pub async fn get_source(
    State(state): State<Arc<AppState>>,
    Path((ws_slug, filename)): Path<(String, String)>,
    headers: axum::http::HeaderMap,
    Query(params): Query<GetSourceParams>,
) -> Result<Json<SourceContent>> {
    let guard = crate::routes::guard::require_membership(&state, &headers, &ws_slug).await?;
    crate::routes::guard::require(&guard, crate::routes::guard::Permission::ViewContent)?;
    super::guard::require_readable_branch(&params.branch, guard.user.id)?;
    super::validate_source_filename(&filename)?;

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
    let compiled = compile_state.sources.contains_key(&filename);
    let compiled_pages = if compiled {
        compile_state
            .source_pages
            .get(&filename)
            .cloned()
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    Ok(Json(SourceContent {
        filename,
        content,
        compiled,
        compiled_pages,
    }))
}
