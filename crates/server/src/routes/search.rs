use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::{AppError, Result};
use crate::AppState;

#[derive(Deserialize)]
pub struct SearchParams {
    pub q: String,
    pub branch: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Serialize)]
pub struct SearchResult {
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub similarity: f64,
}

pub async fn search(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> Result<Json<Vec<SearchResult>>> {
    let branch = params.branch.unwrap_or_else(|| "main".into());
    let limit = params.limit.unwrap_or(10);

    let embedding = state
        .compiler
        .embed(&params.q)
        .await
        .map_err(AppError::Internal)?;

    let results =
        cowiki_db::pages::find_similar(&state.db, &embedding, &branch, limit, 0.3).await?;

    Ok(Json(
        results
            .into_iter()
            .map(|(page, score)| SearchResult {
                slug: page.slug,
                title: page.title,
                summary: page.summary,
                similarity: score,
            })
            .collect(),
    ))
}
