use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;

use crate::error::{AppError, Result};
use crate::routes::guard::{self, Permission};
use crate::AppState;

#[derive(Deserialize)]
pub struct SearchParams {
    pub q: String,
    pub limit: Option<i64>,
}

#[derive(Serialize)]
pub struct SearchResult {
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub similarity: f64,
    /// Which branch the hit came from: "draft" (your branch) or "main".
    pub source: String,
}

/// Workspace-scoped semantic search (#44, #55). Requires membership; searches the
/// caller's effective view only — their own draft branch plus the workspace's merged
/// `main` — never other users' un-reviewed drafts. Per-slug, the draft hit wins
/// (it is what the user would see when opening the page).
pub async fn search(
    State(state): State<Arc<AppState>>,
    Path(ws_slug): Path<String>,
    headers: axum::http::HeaderMap,
    Query(params): Query<SearchParams>,
) -> Result<Json<Vec<SearchResult>>> {
    let guard = guard::require_membership(&state, &headers, &ws_slug).await?;
    guard::require(&guard, Permission::ViewContent)?;

    let limit = params.limit.unwrap_or(10).clamp(1, 50);
    if params.q.trim().is_empty() {
        return Ok(Json(Vec::new()));
    }

    let embedding = state
        .compiler
        .embed(&params.q)
        .await
        .map_err(AppError::Internal)?;

    let user_branch = format!("user/{}", guard.user.id);
    let drafts = cowiki_db::pages::find_similar(
        &state.db,
        &embedding,
        &user_branch,
        limit,
        0.3,
        Some(&ws_slug),
    )
    .await?;
    let merged =
        cowiki_db::pages::find_similar(&state.db, &embedding, "main", limit, 0.3, Some(&ws_slug))
            .await?;

    // Overlay semantics: a draft of a page shadows its merged version.
    let mut seen: HashSet<String> = HashSet::new();
    let mut results: Vec<SearchResult> = Vec::new();
    for (page, score, source) in drafts
        .into_iter()
        .map(|(p, s)| (p, s, "draft"))
        .chain(merged.into_iter().map(|(p, s)| (p, s, "main")))
    {
        if seen.insert(page.slug.clone()) {
            results.push(SearchResult {
                slug: page.slug,
                title: page.title,
                summary: page.summary,
                similarity: score,
                source: source.into(),
            });
        }
    }
    results.sort_by(|a, b| b.similarity.total_cmp(&a.similarity));
    results.truncate(limit as usize);
    Ok(Json(results))
}
