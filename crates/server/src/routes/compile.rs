use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;

use crate::error::{AppError, Result};
use crate::AppState;

#[derive(Deserialize)]
pub struct CompileRequest {
    pub branch: String,
}

#[derive(Serialize)]
pub struct CompileResponse {
    pub pages: Vec<CompiledPage>,
}

#[derive(Serialize)]
pub struct CompiledPage {
    pub slug: String,
    pub title: String,
    pub summary: String,
}

pub async fn compile(
    State(state): State<Arc<AppState>>,
    Json(input): Json<CompileRequest>,
) -> Result<Json<CompileResponse>> {
    let source_files = state
        .wiki_repo
        .list_files(&input.branch, "sources")
        .map_err(|e| AppError::Internal(e.to_string()))?;

    if source_files.is_empty() {
        return Ok(Json(CompileResponse { pages: vec![] }));
    }

    let mut sources = Vec::new();
    for file in &source_files {
        if let Some(content) = state
            .wiki_repo
            .read_file(&input.branch, file)
            .map_err(|e| AppError::Internal(e.to_string()))?
        {
            let text = String::from_utf8_lossy(&content).into_owned();
            let name = file.rsplit('/').next().unwrap_or(file);
            sources.push((name.to_string(), text));
        }
    }

    let compiled = state
        .compiler
        .compile(&sources)
        .await
        .map_err(AppError::Internal)?;

    let default_user = cowiki_db::users::get_default(&state.db).await?;

    let mut result_pages = Vec::new();
    for page in &compiled {
        let full_content = format!(
            "---\ntitle: \"{}\"\nsummary: \"{}\"\nsources:\n{}\n---\n\n{}",
            page.title,
            page.summary,
            page.sources
                .iter()
                .map(|s| format!("  - {s}"))
                .collect::<Vec<_>>()
                .join("\n"),
            page.body,
        );

        let path = format!("wiki/{}.md", page.slug);
        state
            .wiki_repo
            .write_file(
                &input.branch,
                &path,
                full_content.as_bytes(),
                &format!("compile: {}", page.title),
                &input.branch,
            )
            .map_err(|e| AppError::Internal(e.to_string()))?;

        let hash = format!("{:x}", Sha256::digest(full_content.as_bytes()));
        if let Ok(emb) = state
            .compiler
            .embed(&format!("{}\n{}", page.title, page.summary))
            .await
        {
            cowiki_db::pages::upsert(
                &state.db,
                &page.slug,
                &page.title,
                &page.summary,
                &input.branch,
                &hash,
                Some(&emb),
                default_user.id,
            )
            .await
            .ok();
        }

        result_pages.push(CompiledPage {
            slug: page.slug.clone(),
            title: page.title.clone(),
            summary: page.summary.clone(),
        });
    }

    Ok(Json(CompileResponse { pages: result_pages }))
}
