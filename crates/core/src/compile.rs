//! Compiler — LLM-powered wiki page compiler.
//!
//! The Compiler struct provides the core LLM/embedder capabilities.
//! Orchestration logic (do_compile, compile_via_agent) lives here so
//! server/routes/compile.rs stays thin.
//!
//! The 4-step fixed pipeline has moved to cowiki-agents::harness::cowiki_compiler.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::ai::embedder::Embedder;
use crate::ai::llm::Llm;
use crate::ai::token_usage::TokenUsage;
use crate::dispatch::{AgentCompileOutput, AgentDispatcher};
use crate::models::Page;

// ── Public types ───────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CompileRequest {
    pub branch: String,
    #[serde(default)]
    pub mode: Option<CompileMode>,
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CompileMode {
    Auto,
    Pipeline,
    Agent,
}

impl Default for CompileMode {
    fn default() -> Self {
        CompileMode::Auto
    }
}

#[derive(Serialize)]
pub struct CompileResponse {
    pub pages: Vec<CompiledPage>,
    pub skipped: usize,
}

#[derive(Serialize, Deserialize)]
pub struct CompiledPage {
    pub slug: String,
    pub title: String,
    pub summary: String,
}

#[derive(Serialize, Deserialize, Default)]
pub struct CompileState {
    pub sources: HashMap<String, String>,
    #[serde(default)]
    pub source_pages: HashMap<String, Vec<String>>,
}

// ── Compiler struct ────────────────────────────────────────────

pub struct Compiler {
    llm: Box<dyn Llm>,
    embedder: Box<dyn Embedder>,
}

impl Compiler {
    pub fn new(llm: Box<dyn Llm>, embedder: Box<dyn Embedder>) -> Self {
        Self { llm, embedder }
    }

    /// Original single-pass compile (kept for backward compatibility).
    pub async fn compile(&self, sources: &[(String, String)]) -> Result<Vec<Page>, String> {
        let combined = sources
            .iter()
            .map(|(name, content)| format!("## Source: {name}\n\n{content}"))
            .collect::<Vec<_>>()
            .join("\n\n---\n\n");

        let system = r#"You are a knowledge compiler. Given source documents, extract distinct concepts and produce wiki pages.

For each concept, output a markdown document with YAML frontmatter:

```
---
title: "Concept Title"
summary: "One-line summary"
sources:
  - source-filename.md
---

Content here with clear explanations.
```

Separate multiple pages with `===PAGE_BREAK===`.

Be concise. One concept per page. Use clear headings. Attribute claims to sources with `^[filename.md]`."#;

        let user = format!("Compile the following sources into wiki pages:\n\n{combined}");
        let result = self.llm.chat(system, &user).await?;

        let pages = result
            .content
            .split("===PAGE_BREAK===")
            .filter(|s: &&str| !s.trim().is_empty())
            .map(|raw: &str| parse_compiled_page(raw.trim()))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(pages)
    }

    pub async fn generate_summary(&self, content: &str) -> Result<String, String> {
        let resp = self
            .llm
            .chat(
                "Generate a one-line summary (max 100 chars) of this content. Return only the summary, nothing else.",
                content,
            )
            .await?;
        Ok(resp.content)
    }

    pub async fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        let result: crate::ai::embedder::EmbedResult = self.embedder.embed(text, false).await?;
        Ok(result.vector)
    }

    /// Get LLM token usage snapshot.
    pub fn llm_usage(&self) -> HashMap<String, TokenUsage> {
        self.llm.usage_snapshot()
    }

    /// Get embedder token usage snapshot.
    pub fn embedder_usage(&self) -> HashMap<String, TokenUsage> {
        self.embedder.usage_snapshot()
    }
}

// ── Orchestration: do_compile ──────────────────────────────────

/// Main compile entry point. Called by the server route handler.
pub async fn do_compile(
    dispatcher: &dyn AgentDispatcher,
    compiler: &Compiler,
    repo: &crate::git::WikiRepo,
    ws_slug: &str,
    wiki_fs_gateway: &crate::gateway::WikiFsGateway,
    db: &sqlx::PgPool,
    branch: &str,
    mode: Option<CompileMode>,
) -> Result<CompileResponse, String> {
    let mut compile_state = load_state(repo, branch);

    let source_files = repo
        .list_files(branch, "sources")
        .map_err(|e| format!("list sources: {e}"))?;

    tracing::warn!(branch = %branch, count = source_files.len(), files = ?source_files, "compile: listed sources");

    if source_files.is_empty() {
        return Ok(CompileResponse {
            pages: vec![],
            skipped: 0,
        });
    }

    // Detect changed sources
    let mut new_sources = Vec::new();
    let mut skipped = 0usize;
    for file in &source_files {
        if let Some(content) = repo
            .read_file(branch, file)
            .map_err(|e| format!("read source: {e}"))?
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
        return Ok(CompileResponse {
            pages: vec![],
            skipped,
        });
    }

    let mode = mode.unwrap_or(CompileMode::Auto);
    let all_simple_md = new_sources
        .iter()
        .all(|(name, _)| !name.contains('/') && name.ends_with(".md"));
    let use_pipeline = match mode {
        CompileMode::Pipeline => true,
        CompileMode::Agent => false,
        CompileMode::Auto => all_simple_md,
    };

    let source_scope: Vec<String> = new_sources
        .iter()
        .map(|(name, _)| format!("sources/{}", name))
        .collect();

    let result_pages = if use_pipeline {
        // Pipeline delegates to agent since compile_fixed has moved to
        // the cowiki_compiler harness (AgentHandle). For now, just use
        // the agent path directly.
        tracing::info!(
            count = new_sources.len(),
            ?mode,
            "compile: pipeline mode -> delegating to agent"
        );
        compile_via_agent(
            dispatcher,
            compiler,
            repo,
            wiki_fs_gateway,
            db,
            branch,
            ws_slug,
            &new_sources,
            &source_scope,
            &mut compile_state,
        )
        .await?
    } else {
        compile_via_agent(
            dispatcher,
            compiler,
            repo,
            wiki_fs_gateway,
            db,
            branch,
            ws_slug,
            &new_sources,
            &source_scope,
            &mut compile_state,
        )
        .await?
    };

    save_state(repo, branch, &compile_state);
    Ok(CompileResponse {
        pages: result_pages,
        skipped,
    })
}

// ── Agent compile path ─────────────────────────────────────────

async fn compile_via_agent(
    dispatcher: &dyn AgentDispatcher,
    compiler: &Compiler,
    repo: &crate::git::WikiRepo,
    wiki_fs_gateway: &crate::gateway::WikiFsGateway,
    db: &sqlx::PgPool,
    branch: &str,
    ws_slug: &str,
    new_sources: &[(String, String)],
    source_scope: &[String],
    compile_state: &mut CompileState,
) -> Result<Vec<CompiledPage>, String> {
    tracing::info!(count = new_sources.len(), "compile: using agent dispatch");

    wiki_fs_gateway.reset_tracking();

    // Remove old MCP tracking file so only this session's writes are collected.
    let tracking_file = repo.path().join(".cowiki").join("tracking.json");
    let _ = std::fs::remove_file(&tracking_file);

    let output = crate::dispatch::dispatch_and_collect(
        dispatcher,
        "compiler",
        repo,
        branch,
        ws_slug,
        new_sources,
        source_scope,
    )
    .await
    .map_err(|e| format!("agent dispatch: {e}"))?;

    index_compiled_pages(
        compiler,
        repo,
        db,
        branch,
        ws_slug,
        &output,
        new_sources,
        compile_state,
    )
    .await
}

// ── Post-processing ────────────────────────────────────────────

/// Index compiled pages: embed + DB upsert + compile_state update.
async fn index_compiled_pages(
    compiler: &Compiler,
    _repo: &crate::git::WikiRepo,
    db: &sqlx::PgPool,
    branch: &str,
    ws_slug: &str,
    output: &AgentCompileOutput,
    new_sources: &[(String, String)],
    compile_state: &mut CompileState,
) -> Result<Vec<CompiledPage>, String> {
    let default_user = cowiki_db::users::get_default(db)
        .await
        .map_err(|e| format!("get default user: {e}"))?;
    let mut result = Vec::new();
    let total = output.pages.len();

    for (i, (path, body)) in output.pages.iter().enumerate() {
        let slug = page_path_to_slug(path);
        let (title, summary) = parse_frontmatter(body);
        let title = title.unwrap_or_else(|| slug.clone());
        let summary = summary.unwrap_or_default();
        let hash = format!("{:x}", Sha256::digest(body.as_bytes()));

        tracing::info!(page = i + 1, total, slug = %slug, "embedding page");
        match compiler.embed(&format!("{}\n{}", title, summary)).await {
            Ok(emb) => {
                let _ = cowiki_db::pages::upsert(
                    db,
                    &slug,
                    &title,
                    &summary,
                    branch,
                    &hash,
                    Some(&emb),
                    default_user.id,
                    ws_slug,
                )
                .await;
            }
            Err(e) => tracing::warn!("embed failed for '{}': {e}", slug),
        }

        for (source_name, _) in new_sources {
            compile_state
                .source_pages
                .entry(source_name.clone())
                .or_default()
                .push(slug.clone());
        }
        result.push(CompiledPage {
            slug,
            title,
            summary,
        });
    }

    Ok(result)
}

// ── Parse helpers ──────────────────────────────────────────────

/// Parse a single compiled page (from single-pass compile).
fn parse_compiled_page(raw: &str) -> Result<Page, String> {
    let raw = raw
        .trim_start_matches("```markdown")
        .trim_start_matches("```md")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    let (title, summary) = parse_frontmatter(raw);
    let title = title.unwrap_or_else(|| "Untitled".to_string());
    let slug = title
        .to_lowercase()
        .replace(|c: char| !c.is_alphanumeric() && c != ' ', "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-");

    Ok(Page {
        slug,
        title: title.clone(),
        summary: summary.clone().unwrap_or_default(),
        body: raw.to_string(),
        sources: vec![],
        created_at: String::new(),
    })
}

/// Parse YAML frontmatter. Returns (title, summary).
pub fn parse_frontmatter(raw: &str) -> (Option<String>, Option<String>) {
    if !raw.starts_with("---\n") {
        return (None, None);
    }
    let end = raw[4..].find("\n---\n").map(|i| i + 4);
    let end = match end {
        Some(e) => e,
        None => return (None, None),
    };
    let yaml = &raw[4..end];
    let mut title = None;
    let mut summary = None;
    for line in yaml.lines() {
        if let Some(val) = line.strip_prefix("title: ") {
            title = Some(val.trim().trim_matches('"').to_string());
        } else if let Some(val) = line.strip_prefix("summary: ") {
            summary = Some(val.trim().trim_matches('"').to_string());
        }
    }
    (title, summary)
}

fn page_path_to_slug(file: &str) -> String {
    let dirs = ["wiki/", "entities/", "concepts/"];
    let stripped = dirs
        .iter()
        .find_map(|d| file.strip_prefix(d))
        .unwrap_or(file);
    stripped.strip_suffix(".md").unwrap_or(stripped).to_string()
}

// ── State persistence ──────────────────────────────────────────

fn load_state(repo: &crate::git::WikiRepo, branch: &str) -> CompileState {
    repo.read_file(branch, ".cowiki/state.json")
        .ok()
        .flatten()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save_state(repo: &crate::git::WikiRepo, branch: &str, state: &CompileState) {
    if let Ok(json) = serde_json::to_string_pretty(state) {
        if let Err(e) = repo.write_file(
            branch,
            ".cowiki/state.json",
            json.as_bytes(),
            "update compile state",
            "cowiki",
        ) {
            tracing::warn!("failed to persist compile state on '{branch}': {e}");
        }
    }
}
