use axum::extract::{Path, State};
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

#[derive(Serialize, Deserialize, Default)]
pub(crate) struct CompileState {
    pub sources: HashMap<String, String>,
    #[serde(default)]
    pub source_pages: HashMap<String, Vec<String>>,
}

/// Workspace-scoped compile
pub async fn compile_ws(
    State(state): State<Arc<AppState>>,
    Path(ws_slug): Path<String>,
    Json(input): Json<CompileRequest>,
) -> Result<Json<CompileResponse>> {
    let repo = state.repo_manager.get(&ws_slug)
        .map_err(|e| AppError::Internal(format!("repo error: {e}")))?;
    super::pages::ensure_user_branch_if_needed(&repo, &input.branch)?;
    do_compile(&state, &repo, &input.branch).await
}

async fn do_compile(
    state: &AppState,
    repo: &cowiki_core::git::WikiRepo,
    branch: &str,
) -> Result<Json<CompileResponse>> {
    // 1. Load compile state
    let mut compile_state = load_state(repo, branch);

    // 2. List sources
    let source_files = repo
        .list_files(branch, "sources")
        .map_err(|e| AppError::Internal(e.to_string()))?;

    if source_files.is_empty() {
        return Ok(Json(CompileResponse { pages: vec![], skipped: 0 }));
    }

    // 3. Read sources, check which changed
    let mut new_sources = Vec::new();
    let mut skipped = 0usize;

    for file in &source_files {
        if let Some(content) = repo
            .read_file(branch, file)
            .map_err(|e| AppError::Internal(e.to_string()))?
        {
            let text = String::from_utf8_lossy(&content).into_owned();
            let hash = format!("{:x}", Sha256::digest(text.as_bytes()));
            let name = file.rsplit('/').next().unwrap_or(file).to_string();

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

    // 4. Compile via LLM
    let compiled = state.compiler.compile(&new_sources).await.map_err(AppError::Internal)?;

    let default_user = cowiki_db::users::get_default(&state.db).await?;

    // 5. Write pages
    let mut result_pages = Vec::new();
    for page in &compiled {
        let full_content = format!(
            "---\ntitle: \"{}\"\nsummary: \"{}\"\nkind: concept\nsources:\n{}\n---\n\n{}",
            page.title, page.summary,
            page.sources.iter().map(|s| format!("  - {s}")).collect::<Vec<_>>().join("\n"),
            page.body,
        );

        let path = format!("wiki/{}.md", page.slug);
        repo.write_file(branch, &path, full_content.as_bytes(), &format!("compile: {}", page.title), branch)
            .map_err(|e| AppError::Internal(e.to_string()))?;

        let hash = format!("{:x}", Sha256::digest(full_content.as_bytes()));
        if let Ok(emb) = state.compiler.embed(&format!("{}\n{}", page.title, page.summary)).await {
            cowiki_db::pages::upsert(&state.db, &page.slug, &page.title, &page.summary, branch, &hash, Some(&emb), default_user.id)
                .await.ok();
        }

        // Record source→page mapping from the compiler's output
        for source in &page.sources {
            compile_state
                .source_pages
                .entry(source.clone())
                .or_default()
                .push(page.slug.clone());
        }

        result_pages.push(CompiledPage { slug: page.slug.clone(), title: page.title.clone(), summary: page.summary.clone() });
    }

    // 6. Save state
    save_state(repo, branch, &compile_state);

    Ok(Json(CompileResponse { pages: result_pages, skipped }))
}

fn load_state(repo: &cowiki_core::git::WikiRepo, branch: &str) -> CompileState {
    repo.read_file(branch, ".cowiki/state.json")
        .ok().flatten()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save_state(repo: &cowiki_core::git::WikiRepo, branch: &str, compile_state: &CompileState) {
    if let Ok(json) = serde_json::to_string_pretty(compile_state) {
        repo.write_file(branch, ".cowiki/state.json", json.as_bytes(), "update compile state", "cowiki").ok();
    }
}
