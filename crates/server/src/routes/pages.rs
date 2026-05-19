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

#[derive(Serialize)]
pub struct PageResponse {
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub body: String,
    pub branch: String,
}

pub async fn list_pages(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<cowiki_db::pages::PageMeta>>> {
    let branch = params.branch.unwrap_or_else(|| "main".into());
    let pages = cowiki_db::pages::list_by_branch(&state.db, &branch).await?;
    Ok(Json(pages))
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
        .map(|l| {
            l.trim()
                .trim_start_matches("title:")
                .trim()
                .trim_matches('"')
                .to_string()
        })
        .unwrap_or_else(|| "Untitled".into());
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
