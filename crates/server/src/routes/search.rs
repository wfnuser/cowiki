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
    /// "keyword" | "semantic" | "all" (default). The palette fires keyword and
    /// semantic as two parallel requests so fast keyword hits render first.
    pub mode: Option<String>,
}

#[derive(Serialize)]
pub struct KeywordHit {
    pub slug: String,
    pub title: String,
    /// The matched line (or title), trimmed around the first occurrence.
    pub snippet: String,
    /// True when the match was in the title (ranked first).
    pub title_match: bool,
}

#[derive(Serialize)]
pub struct SemanticHit {
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub similarity: f64,
    /// Which branch the hit came from: "draft" (your branch) or "main".
    pub source: String,
}

#[derive(Serialize)]
pub struct SearchResponse {
    /// Full-text matches over the caller's effective view (title + body).
    pub keyword: Vec<KeywordHit>,
    /// Embedding-based matches (only pages that have been compiled/merged are indexed).
    pub semantic: Vec<SemanticHit>,
}

/// Trim `line` to ~`width` chars centered on the first match at `pos`.
fn make_snippet(line: &str, pos: usize, width: usize) -> String {
    let line = line.trim();
    if line.len() <= width {
        return line.to_string();
    }
    let start = pos.saturating_sub(width / 3);
    // Don't slice mid-UTF-8: walk to char boundaries.
    let mut s = start.min(line.len());
    while s > 0 && !line.is_char_boundary(s) {
        s -= 1;
    }
    let mut e = (s + width).min(line.len());
    while e < line.len() && !line.is_char_boundary(e) {
        e += 1;
    }
    let mut out = String::new();
    if s > 0 {
        out.push('…');
    }
    out.push_str(&line[s..e]);
    if e < line.len() {
        out.push('…');
    }
    out
}

/// Workspace-scoped search (#44, #55). Requires membership; searches the caller's
/// effective view only — their own draft branch (which, per ADR 0004, is their full
/// view of the wiki) — never other users' un-reviewed drafts.
///
/// Two result groups:
/// - `keyword`: substring scan over titles and bodies of the git tree (works for
///   every page, no index needed). Title matches rank first.
/// - `semantic`: pgvector similarity over indexed pages (draft ∪ main, draft wins
///   per slug). Embedding failures degrade to an empty group instead of a 500 so
///   keyword search keeps working when the embedder is unreachable.
pub async fn search(
    State(state): State<Arc<AppState>>,
    Path(ws_slug): Path<String>,
    headers: axum::http::HeaderMap,
    Query(params): Query<SearchParams>,
) -> Result<Json<SearchResponse>> {
    let guard = guard::require_membership(&state, &headers, &ws_slug).await?;
    guard::require(&guard, Permission::ViewContent)?;

    let limit = params.limit.unwrap_or(12).clamp(1, 50) as usize;
    let q = params.q.trim().to_string();
    if q.is_empty() {
        return Ok(Json(SearchResponse {
            keyword: Vec::new(),
            semantic: Vec::new(),
        }));
    }
    let q_lower = q.to_lowercase();
    let mode = params.mode.as_deref().unwrap_or("all");
    let want_keyword = mode != "semantic";
    let want_semantic = mode != "keyword";

    let repo = state
        .repo_manager
        .get(&ws_slug)
        .map_err(|e| AppError::Internal(format!("repo error: {e}")))?;
    let user_branch = format!("user/{}", guard.user.id);
    super::pages::ensure_user_branch_if_needed(&repo, &user_branch)?;

    // ── Keyword: scan the user's branch tree (their effective view) ──
    let mut keyword: Vec<KeywordHit> = Vec::new();
    let files = if want_keyword {
        repo.list_files_recursive(&user_branch, "wiki")
            .map_err(|e| AppError::Internal(e.to_string()))?
    } else {
        Vec::new()
    };
    for path in files {
        if keyword.len() >= limit * 2 {
            break; // enough candidates; title matches are sorted first below
        }
        let Ok(Some(content)) = repo.read_file(&user_branch, &path) else {
            continue;
        };
        let text = String::from_utf8_lossy(&content);
        let (title, _) = super::pages::parse_frontmatter(&text);
        let slug = path
            .strip_prefix("wiki/")
            .and_then(|s| s.strip_suffix(".md"))
            .unwrap_or(&path)
            .to_string();
        let title = title.unwrap_or_else(|| slug.clone());

        if title.to_lowercase().contains(&q_lower) {
            keyword.push(KeywordHit {
                slug,
                snippet: title.clone(),
                title,
                title_match: true,
            });
            continue;
        }
        // Body match: first line containing the query (skip the frontmatter block).
        let body = text
            .splitn(3, "---")
            .nth(2)
            .map(str::to_string)
            .unwrap_or_else(|| text.to_string());
        if let Some(line) = body.lines().find(|l| l.to_lowercase().contains(&q_lower)) {
            let pos = line.to_lowercase().find(&q_lower).unwrap_or(0);
            keyword.push(KeywordHit {
                slug,
                title,
                snippet: make_snippet(line, pos, 110),
                title_match: false,
            });
        }
    }
    keyword.sort_by(|a, b| b.title_match.cmp(&a.title_match));
    keyword.truncate(limit);

    // ── Semantic: best-effort (indexed pages only; tolerate embedder failure) ──
    let mut semantic: Vec<SemanticHit> = Vec::new();
    if want_semantic && q.chars().count() >= 2 {
        if let Ok(embedding) = state.compiler.embed(&q).await {
            let drafts = cowiki_db::pages::find_similar(
                &state.db,
                &embedding,
                &user_branch,
                limit as i64,
                0.3,
                Some(&ws_slug),
            )
            .await
            .unwrap_or_default();
            let merged = cowiki_db::pages::find_similar(
                &state.db,
                &embedding,
                "main",
                limit as i64,
                0.3,
                Some(&ws_slug),
            )
            .await
            .unwrap_or_default();

            // Overlay semantics: a draft of a page shadows its merged version.
            let mut seen: HashSet<String> = HashSet::new();
            for (page, score, source) in drafts
                .into_iter()
                .map(|(p, s)| (p, s, "draft"))
                .chain(merged.into_iter().map(|(p, s)| (p, s, "main")))
            {
                if seen.insert(page.slug.clone()) {
                    semantic.push(SemanticHit {
                        slug: page.slug,
                        title: page.title,
                        summary: page.summary,
                        similarity: score,
                        source: source.into(),
                    });
                }
            }
            semantic.sort_by(|a, b| b.similarity.total_cmp(&a.similarity));
            semantic.truncate(limit);
        } else {
            tracing::warn!("semantic search degraded: embedder unavailable");
        }
    }

    Ok(Json(SearchResponse { keyword, semantic }))
}
