use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
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
    pub skipped: usize,
}

#[derive(Serialize)]
pub struct CompiledPage {
    pub slug: String,
    pub title: String,
    pub summary: String,
}

/// State file tracks source hashes for incremental compilation
#[derive(Serialize, Deserialize, Default)]
struct CompileState {
    sources: HashMap<String, String>, // filename → sha256 hash
}

pub async fn compile(
    State(state): State<Arc<AppState>>,
    Json(input): Json<CompileRequest>,
) -> Result<Json<CompileResponse>> {
    let branch = &input.branch;

    // 1. Load existing compile state from .cowiki/state.json on this branch
    let mut compile_state = load_state(&state, branch);

    // 2. List all sources on this branch
    let source_files = state
        .wiki_repo
        .list_files(branch, "sources")
        .map_err(|e| AppError::Internal(e.to_string()))?;

    if source_files.is_empty() {
        return Ok(Json(CompileResponse { pages: vec![], skipped: 0 }));
    }

    // 3. Read sources and check which ones changed (incremental)
    let mut new_sources = Vec::new();
    let mut skipped = 0usize;

    for file in &source_files {
        if let Some(content) = state
            .wiki_repo
            .read_file(branch, file)
            .map_err(|e| AppError::Internal(e.to_string()))?
        {
            let text = String::from_utf8_lossy(&content).into_owned();
            let hash = format!("{:x}", Sha256::digest(text.as_bytes()));
            let name = file.rsplit('/').next().unwrap_or(file).to_string();

            // Skip if hash unchanged
            if compile_state.sources.get(&name) == Some(&hash) {
                skipped += 1;
                continue;
            }

            compile_state.sources.insert(name.clone(), hash);
            new_sources.push((name, text));
        }
    }

    if new_sources.is_empty() {
        return Ok(Json(CompileResponse { pages: vec![], skipped }));
    }

    // 4. Compile only new/changed sources via LLM
    let compiled = state
        .compiler
        .compile(&new_sources)
        .await
        .map_err(AppError::Internal)?;

    let default_user = cowiki_db::users::get_default(&state.db).await?;

    // 5. Write compiled pages and update DB
    let mut result_pages = Vec::new();
    for page in &compiled {
        let full_content = format!(
            "---\ntitle: \"{}\"\nsummary: \"{}\"\nkind: concept\nsources:\n{}\n---\n\n{}",
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
                branch,
                &path,
                full_content.as_bytes(),
                &format!("compile: {}", page.title),
                branch,
            )
            .map_err(|e| AppError::Internal(e.to_string()))?;

        // Generate embedding and save to DB
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
                branch,
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

    // 6. Save updated compile state
    save_state(&state, branch, &compile_state);

    Ok(Json(CompileResponse { pages: result_pages, skipped }))
}

fn load_state(state: &AppState, branch: &str) -> CompileState {
    state
        .wiki_repo
        .read_file(branch, ".cowiki/state.json")
        .ok()
        .flatten()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save_state(state: &AppState, branch: &str, compile_state: &CompileState) {
    if let Ok(json) = serde_json::to_string_pretty(compile_state) {
        state
            .wiki_repo
            .write_file(
                branch,
                ".cowiki/state.json",
                json.as_bytes(),
                "update compile state",
                "cowiki",
            )
            .ok();
    }
}
