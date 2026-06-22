//! Compile module — agent dispatch + pipeline compile + post-processing.
//!
//! Two paths:
//! - `compile_via_pipeline`: 4-step LLM pipeline (entities → concepts → wiki → cross-refs)
//! - `compile_via_agent`: dispatch to agent (pi/copilot) via cowiki CLI
//! `do_compile` dispatches based on COMPILE_MODE.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::ai::embedder::{self, Embedder};
use crate::ai::llm::Llm;
use crate::dispatch::AgentDispatcher;

pub mod task_type {
    pub const COMPILE: &str = "compile";
    pub const DEEP_COMPILE: &str = "deep-compile";
    pub const REVIEW: &str = "review";
}

// ── Public types ───────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CompileRequest {
    pub branch: String,
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

// ── Prompt ─────────────────────────────────────────────────────

fn pipeline_prompt(
    step: &str,
    sources: &[(String, String)],
    summaries: &[(String, String)], // (dir/slug, summary)
) -> String {
    let src_list = sources
        .iter()
        .map(|(n, _)| format!("sources/{}", n))
        .collect::<Vec<_>>()
        .join(", ");
    let summary_list = summaries
        .iter()
        .map(|(p, s)| format!("- {p}: {s}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!("Step {step}: {src_list}\n\nExisting summaries:\n{summary_list}")
}

// ── Pipeline ───────────────────────────────────────────────────

pub async fn compile_via_pipeline(
    llm: &dyn Llm,
    embedder: &dyn Embedder,
    repo: &crate::git::WikiRepo,
    ws_slug: &str,
    db: &sqlx::PgPool,
    branch: &str,
    new_sources: &[(String, String)],
    compile_state: &mut CompileState,
) -> Result<Vec<CompiledPage>, String> {
    let source_texts: Vec<(String, String)> = new_sources.to_vec();

    // ── Step 1: sources → entities ──
    let prompt = pipeline_prompt("1a: Extract entities", &source_texts, &[]);
    let resp = llm
        .chat(
            "Extract named entities from sources. Output YAML frontmatter pages.",
            &prompt,
        )
        .await?;
    let entities = write_pages(
        repo,
        branch,
        "entities",
        &resp.content,
        llm,
        embedder,
        db,
        ws_slug,
    )
    .await?;

    // ── Step 2: sources + entities → concepts ──
    let entity_summaries: Vec<(String, String)> = read_summaries(repo, branch, "entities").await;
    let prompt = pipeline_prompt("2a: Extract concepts", &source_texts, &entity_summaries);
    let resp = llm
        .chat(
            "Extract concepts/patterns from sources and entities.",
            &prompt,
        )
        .await?;
    let concepts = write_pages(
        repo,
        branch,
        "concepts",
        &resp.content,
        llm,
        embedder,
        db,
        ws_slug,
    )
    .await?;

    // ── Step 3: sources + entities + concepts → wiki ──
    let concept_summaries: Vec<(String, String)> = read_summaries(repo, branch, "concepts").await;
    let mut all_summaries = entity_summaries;
    all_summaries.extend(concept_summaries);
    let prompt = pipeline_prompt("3a: Synthesize wiki pages", &source_texts, &all_summaries);
    let resp = llm
        .chat(
            "Synthesize wiki pages from all extracted knowledge.",
            &prompt,
        )
        .await?;
    let wiki_pages = write_pages(
        repo,
        branch,
        "wiki",
        &resp.content,
        llm,
        embedder,
        db,
        ws_slug,
    )
    .await?;

    // ── Step 4: wiki pages → cross-refs ──
    let _wiki_bodies: Vec<(String, String)> = entities
        .iter()
        .chain(concepts.iter())
        .chain(wiki_pages.iter())
        .map(|p| (format!("{}/{}.md", "wiki", p.slug), String::new()))
        .collect();
    // ... cross-ref fixup via LLM ...

    let mut all_pages = entities;
    all_pages.extend(concepts);
    all_pages.extend(wiki_pages);

    for (source_name, _) in new_sources {
        compile_state
            .source_pages
            .entry(source_name.clone())
            .or_default()
            .extend(all_pages.iter().map(|p| p.slug.clone()));
    }
    Ok(all_pages)
}

async fn write_pages(
    repo: &crate::git::WikiRepo,
    branch: &str,
    dir: &str,
    llm_output: &str,
    _llm: &dyn Llm,
    embedder: &dyn Embedder,
    db: &sqlx::PgPool,
    ws_slug: &str,
) -> Result<Vec<CompiledPage>, String> {
    let pages = llm_output
        .split("===PAGE_BREAK===")
        .filter(|s| !s.trim().is_empty());
    let default_user = cowiki_db::users::get_default(db)
        .await
        .map_err(|e| format!("user: {e}"))?;
    let mut result = Vec::new();
    for raw in pages {
        let raw = raw
            .trim()
            .trim_start_matches("```markdown")
            .trim_start_matches("```md")
            .trim_end_matches("```")
            .trim();
        let (title, summary) = parse_frontmatter(raw);
        let title = title.unwrap_or_else(|| "Untitled".into());
        let slug = title
            .to_lowercase()
            .replace(|c: char| !c.is_alphanumeric() && c != ' ', "")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join("-");
        let body = raw.to_string();
        let path = format!("{dir}/{slug}.md");
        repo.write_file(branch, &path, body.as_bytes(), "compile", "cowiki")
            .map_err(|e| format!("write: {e}"))?;
        let hash = format!("{:x}", Sha256::digest(body.as_bytes()));
        match embedder::embed(
            embedder,
            &format!("{title}\n{}", summary.as_deref().unwrap_or("")),
        )
        .await
        {
            Ok(emb) => {
                let _ = cowiki_db::pages::upsert(
                    db,
                    &slug,
                    &title,
                    summary.as_deref().unwrap_or(""),
                    branch,
                    &hash,
                    Some(&emb),
                    default_user.id,
                    ws_slug,
                )
                .await;
            }
            Err(e) => tracing::warn!(%slug, %e, "embed failed"),
        }
        result.push(CompiledPage {
            slug: slug.clone(),
            title,
            summary: summary.unwrap_or_default(),
        });
    }
    Ok(result)
}

async fn read_summaries(
    repo: &crate::git::WikiRepo,
    branch: &str,
    dir: &str,
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Ok(files) = repo.list_files(branch, dir) {
        for f in files {
            if f.ends_with(".md") {
                if let Ok(Some(content)) = repo.read_file(branch, &f) {
                    let text = String::from_utf8_lossy(&content);
                    let (_, summary) = parse_frontmatter(&text);
                    out.push((f.clone(), summary.unwrap_or_default()));
                }
            }
        }
    }
    out
}

// ── Orchestration ──────────────────────────────────────────────

pub async fn do_compile(
    dispatcher: &dyn AgentDispatcher,
    llm: &dyn Llm,
    embedder: &dyn Embedder,
    repo: &crate::git::WikiRepo,
    ws_slug: &str,
    db: &sqlx::PgPool,
    branch: &str,
    api_key: &str,
    compile_mode: &str,
) -> Result<CompileResponse, String> {
    let mut compile_state = load_state(repo, branch);
    let source_files = repo
        .list_files(branch, "sources")
        .map_err(|e| format!("list: {e}"))?;
    if source_files.is_empty() {
        return Ok(CompileResponse {
            pages: vec![],
            skipped: 0,
        });
    }

    let mut new_sources = Vec::new();
    let mut skipped = 0usize;
    for file in &source_files {
        if let Some(content) = repo
            .read_file(branch, file)
            .map_err(|e| format!("read: {e}"))?
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

    let source_scope: Vec<String> = new_sources
        .iter()
        .map(|(n, _)| format!("sources/{}", n))
        .collect();
    let result_pages = match compile_mode {
        "pipeline" => {
            compile_via_pipeline(
                llm,
                embedder,
                repo,
                ws_slug,
                db,
                branch,
                &new_sources,
                &mut compile_state,
            )
            .await?
        }
        _ => {
            compile_via_agent(
                dispatcher,
                embedder,
                repo,
                db,
                branch,
                ws_slug,
                &new_sources,
                &source_scope,
                &mut compile_state,
                api_key,
            )
            .await?
        }
    };
    save_state(repo, branch, &compile_state);
    Ok(CompileResponse {
        pages: result_pages,
        skipped,
    })
}

// ── Agent path ─────────────────────────────────────────────────

async fn compile_via_agent(
    dispatcher: &dyn AgentDispatcher,
    embedder: &dyn Embedder,
    repo: &crate::git::WikiRepo,
    db: &sqlx::PgPool,
    branch: &str,
    ws_slug: &str,
    new_sources: &[(String, String)],
    source_scope: &[String],
    compile_state: &mut CompileState,
    api_key: &str,
) -> Result<Vec<CompiledPage>, String> {
    let prompt = build_compile_prompt(new_sources, ws_slug, branch);
    let mut extra_args = HashMap::new();
    extra_args.insert("branch".into(), branch.into());
    extra_args.insert("source_scope".into(), source_scope.join(","));
    extra_args.insert("api_key".into(), api_key.into());

    let result = dispatcher
        .dispatch(
            "compiler",
            &prompt,
            &["cowiki-compiler"],
            ws_slug,
            extra_args,
        )
        .await
        .map_err(|e| format!("dispatch: {e}"))?;
    if !result.success {
        return Err(result.error.unwrap_or_else(|| "agent failed".into()));
    }

    let mut written = result.written_files.clone();
    if written.is_empty() {
        for dir in &["wiki", "entities", "concepts"] {
            if let Ok(files) = repo.list_files(branch, dir) {
                written.extend(files.into_iter().filter(|f| f.ends_with(".md")));
            }
        }
    }
    let mut pages = Vec::new();
    for path in &written {
        if let Ok(Some(c)) = repo.read_file(branch, path) {
            pages.push((path.clone(), String::from_utf8_lossy(&c).into_owned()));
        }
    }
    index_compiled_pages(
        embedder,
        db,
        branch,
        ws_slug,
        &pages,
        new_sources,
        compile_state,
    )
    .await
}

fn build_compile_prompt(sources: &[(String, String)], workspace: &str, branch: &str) -> String {
    let list = sources
        .iter()
        .map(|(n, _)| format!("sources/{}", n))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "Your task: compile source pages into a structured knowledge base.\n\
             Workflow: entities → concepts → wiki pages → cross-references.\n\
             Use ONLY the cowiki CLI for all wiki operations.\n\n\
             Sources: {list}\n\nworkspace: {ws}\nbranch: {branch}\n\
             Always pass -w {ws} to cowiki CLI.",
        ws = workspace,
        branch = branch
    )
}

// ── Post-processing ────────────────────────────────────────────

async fn index_compiled_pages(
    embedder: &dyn Embedder,
    db: &sqlx::PgPool,
    branch: &str,
    ws_slug: &str,
    pages: &[(String, String)],
    new_sources: &[(String, String)],
    compile_state: &mut CompileState,
) -> Result<Vec<CompiledPage>, String> {
    let default_user = cowiki_db::users::get_default(db)
        .await
        .map_err(|e| format!("user: {e}"))?;
    let mut result = Vec::new();
    for (path, body) in pages {
        let slug = page_path_to_slug(path);
        let (title, summary) = parse_frontmatter(body);
        let title = title.unwrap_or_else(|| slug.clone());
        let summary = summary.unwrap_or_default();
        let hash = format!("{:x}", Sha256::digest(body.as_bytes()));
        if let Ok(emb) = embedder::embed(embedder, &format!("{title}\n{summary}")).await {
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
        for (sn, _) in new_sources {
            compile_state
                .source_pages
                .entry(sn.clone())
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

// ── Parsing ────────────────────────────────────────────────────

pub fn parse_frontmatter(raw: &str) -> (Option<String>, Option<String>) {
    if !raw.starts_with("---\n") {
        return (None, None);
    }
    let end = match raw[4..].find("\n---\n") {
        Some(i) => i + 4,
        None => return (None, None),
    };
    let yaml = &raw[4..end];
    let mut title = None;
    let mut summary = None;
    for line in yaml.lines() {
        if let Some(v) = line.strip_prefix("title: ") {
            title = Some(v.trim().trim_matches('"').into());
        } else if let Some(v) = line.strip_prefix("summary: ") {
            summary = Some(v.trim().trim_matches('"').into());
        }
    }
    (title, summary)
}

fn page_path_to_slug(file: &str) -> String {
    for d in &["wiki/", "entities/", "concepts/"] {
        if let Some(s) = file.strip_prefix(d) {
            return s.strip_suffix(".md").unwrap_or(s).into();
        }
    }
    file.strip_suffix(".md").unwrap_or(file).into()
}

// ── State ──────────────────────────────────────────────────────

fn load_state(repo: &crate::git::WikiRepo, branch: &str) -> CompileState {
    repo.read_file(branch, ".cowiki/state.json")
        .ok()
        .flatten()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save_state(repo: &crate::git::WikiRepo, branch: &str, state: &CompileState) {
    if let Ok(j) = serde_json::to_string_pretty(state) {
        let _ = repo.write_file(
            branch,
            ".cowiki/state.json",
            j.as_bytes(),
            "compile state",
            "cowiki",
        );
    }
}
