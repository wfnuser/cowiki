//! Document comment endpoints: Feishu-style inline comments on a wiki page,
//! anchored to source line ranges.
//!
//! All endpoints require workspace membership (Viewer+) — a logged-in non-member
//! must not read comments or the page snapshots they carry (those snapshots are
//! the full page markdown). Any member may comment; `@name` mentions notify only
//! users who are members of the same workspace.

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::sync::Arc;

use crate::error::{AppError, Result};
use crate::routes::guard::require_membership;
use crate::AppState;
use cowiki_db::page_comments::{CommentSnapshot, PageComment};

#[derive(Deserialize)]
pub struct PageQuery {
    /// Page slug, e.g. `roadmap/index`. Passed as a query param so slugs with
    /// slashes don't collide with the `/{comment_id}` mutation routes.
    pub slug: String,
}

#[derive(Serialize)]
pub struct ListResponse {
    pub comments: Vec<PageComment>,
    /// Snapshots referenced by the comments' `content_hash`, for client re-anchoring.
    pub snapshots: Vec<CommentSnapshot>,
}

#[derive(Deserialize)]
pub struct CreateCommentRequest {
    pub slug: String,
    /// Markdown the anchor was created against (top-level comments only).
    pub source: Option<String>,
    /// 1-based inclusive line range (top-level comments only).
    pub start_line: Option<i32>,
    pub end_line: Option<i32>,
    pub body: String,
    pub parent_id: Option<uuid::Uuid>,
}

fn sha256_hex(s: &str) -> String {
    let digest = Sha256::digest(s.as_bytes());
    let mut out = String::with_capacity(64);
    for b in digest {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// Extract distinct `@handle` mentions from a comment body.
fn parse_mentions(body: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'@' {
            let start = i + 1;
            let mut j = start;
            while j < bytes.len()
                && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_' || bytes[j] == b'-')
            {
                j += 1;
            }
            if j > start {
                let name = &body[start..j];
                if !names.iter().any(|n| n == name) {
                    names.push(name.to_string());
                }
            }
            i = j;
        } else {
            i += 1;
        }
    }
    names
}

/// List all comments on a page plus the snapshots needed to re-anchor them.
/// Requires membership — snapshots contain the full page markdown.
pub async fn list_comments(
    State(state): State<Arc<AppState>>,
    Path(ws_slug): Path<String>,
    headers: axum::http::HeaderMap,
    Query(q): Query<PageQuery>,
) -> Result<Json<ListResponse>> {
    require_membership(&state, &headers, &ws_slug).await?;
    let comments = cowiki_db::page_comments::list_for_page(&state.db, &ws_slug, &q.slug).await?;
    let snapshots =
        cowiki_db::page_comments::snapshots_for_page(&state.db, &ws_slug, &q.slug).await?;
    Ok(Json(ListResponse {
        comments,
        snapshots,
    }))
}

/// Create a comment (or reply) and notify any `@`-mentioned **members**.
///
/// Any workspace member (Viewer+) may comment. Shape is validated strictly:
/// a top-level comment must carry `source` + a valid line range; a reply must
/// reference a top-level comment on the same page and carries no anchor.
pub async fn create_comment(
    State(state): State<Arc<AppState>>,
    Path(ws_slug): Path<String>,
    headers: axum::http::HeaderMap,
    Json(input): Json<CreateCommentRequest>,
) -> Result<Json<PageComment>> {
    let guard = require_membership(&state, &headers, &ws_slug).await?;
    let user = &guard.user;

    // Validate the body before any write, so a rejected comment never leaves a
    // snapshot row behind (the snapshot upsert below happens for top-level ones).
    if input.body.trim().is_empty() {
        return Err(AppError::BadRequest("comment body is empty".into()));
    }

    let (content_hash, start_line, end_line) = match input.parent_id {
        // Reply: must target a top-level comment on the same page; no anchor.
        Some(parent_id) => {
            let parent = cowiki_db::page_comments::find(&state.db, parent_id)
                .await?
                .ok_or_else(|| AppError::NotFound("parent comment not found".into()))?;
            if parent.parent_id.is_some() {
                return Err(AppError::BadRequest("cannot reply to a reply".into()));
            }
            if parent.workspace_slug != ws_slug || parent.page_slug != input.slug {
                return Err(AppError::BadRequest(
                    "parent comment belongs to a different page".into(),
                ));
            }
            (None, None, None)
        }
        // Top-level: requires a page snapshot + a valid 1-based line range.
        None => {
            let source = input
                .source
                .as_deref()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| AppError::BadRequest("a comment requires source".into()))?;
            let (s, e) = match (input.start_line, input.end_line) {
                (Some(s), Some(e)) if s >= 1 && s <= e => (s, e),
                _ => return Err(AppError::BadRequest("invalid line range".into())),
            };
            let hash = sha256_hex(source);
            cowiki_db::page_comments::upsert_snapshot(
                &state.db,
                &ws_slug,
                &input.slug,
                &hash,
                source,
            )
            .await?;
            (Some(hash), Some(s), Some(e))
        }
    };

    let comment = cowiki_db::page_comments::create(
        &state.db,
        &ws_slug,
        &input.slug,
        user.id,
        content_hash.as_deref(),
        start_line,
        end_line,
        &input.body,
        input.parent_id,
    )
    .await?;

    // Fan out @mention notifications — only to members of this workspace.
    let link = format!("/{}/{}", ws_slug, input.slug);
    for name in parse_mentions(&input.body) {
        if name == user.name {
            continue;
        }
        let Ok(Some(target)) = cowiki_db::users::find_by_name(&state.db, &name).await else {
            continue;
        };
        if target.id == user.id {
            continue;
        }
        let is_member = cowiki_db::workspaces::is_member(&state.db, guard.workspace.id, target.id)
            .await
            .unwrap_or(false);
        if !is_member {
            continue;
        }
        let _ = cowiki_db::notifications::create(
            &state.db,
            target.id,
            "mention",
            &format!("{} mentioned you in a comment", user.name),
            Some(&input.body),
            Some(guard.workspace.id),
            Some(&link),
        )
        .await;
    }

    Ok(Json(comment))
}

/// Load a comment, 404ing unless it belongs to `ws_slug` (prevents cross-workspace
/// mutation by guessable comment id).
async fn load_in_workspace(
    state: &Arc<AppState>,
    ws_slug: &str,
    comment_id: uuid::Uuid,
) -> Result<PageComment> {
    let c = cowiki_db::page_comments::find(&state.db, comment_id)
        .await?
        .filter(|c| c.workspace_slug == ws_slug)
        .ok_or_else(|| AppError::NotFound("comment not found".into()))?;
    Ok(c)
}

pub async fn resolve_comment(
    State(state): State<Arc<AppState>>,
    Path((ws_slug, comment_id)): Path<(String, uuid::Uuid)>,
    headers: axum::http::HeaderMap,
) -> Result<Json<PageComment>> {
    require_membership(&state, &headers, &ws_slug).await?;
    load_in_workspace(&state, &ws_slug, comment_id).await?;
    let comment = cowiki_db::page_comments::set_resolved(&state.db, comment_id, true).await?;
    Ok(Json(comment))
}

pub async fn unresolve_comment(
    State(state): State<Arc<AppState>>,
    Path((ws_slug, comment_id)): Path<(String, uuid::Uuid)>,
    headers: axum::http::HeaderMap,
) -> Result<Json<PageComment>> {
    require_membership(&state, &headers, &ws_slug).await?;
    load_in_workspace(&state, &ws_slug, comment_id).await?;
    let comment = cowiki_db::page_comments::set_resolved(&state.db, comment_id, false).await?;
    Ok(Json(comment))
}

/// Delete a comment. Member of the workspace + author of the comment only.
pub async fn delete_comment(
    State(state): State<Arc<AppState>>,
    Path((ws_slug, comment_id)): Path<(String, uuid::Uuid)>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>> {
    let guard = require_membership(&state, &headers, &ws_slug).await?;
    let comment = load_in_workspace(&state, &ws_slug, comment_id).await?;
    if comment.user_id != guard.user.id {
        return Err(AppError::Forbidden(
            "you can only delete your own comment".into(),
        ));
    }
    cowiki_db::page_comments::delete(&state.db, comment_id).await?;
    Ok(Json(serde_json::json!({"ok": true})))
}

#[cfg(test)]
mod tests {
    use super::{parse_mentions, sha256_hex};

    #[test]
    fn parses_distinct_handles() {
        let got = parse_mentions("hey @wfnuser and @Deadpoolmine, also @wfnuser again");
        assert_eq!(got, vec!["wfnuser".to_string(), "Deadpoolmine".to_string()]);
    }

    #[test]
    fn handles_punctuation_and_emails() {
        // Trailing punctuation is excluded; an email's local part still parses
        // its `@domain` as a handle candidate (resolution later just finds nobody).
        let got = parse_mentions("ping @alice! see foo@example, end");
        assert_eq!(got, vec!["alice".to_string(), "example".to_string()]);
    }

    #[test]
    fn lone_at_yields_nothing() {
        assert!(parse_mentions("an @ sign alone").is_empty());
    }

    #[test]
    fn sha256_is_stable_and_hex() {
        let h = sha256_hex("hello");
        assert_eq!(
            h,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }
}
