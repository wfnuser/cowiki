use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::{AppError, Result};

use crate::AppState;
use cowiki_core::models::DuplicateWarning;

#[derive(Deserialize)]
pub struct SubmitRequest {
    pub branch: String,
    pub page_slugs: Vec<String>,
    /// If true, skip review and merge directly to main (for Personal Space)
    #[serde(default)]
    pub skip_review: bool,
}

#[derive(Serialize)]
pub struct SubmitResponse {
    pub submission_id: uuid::Uuid,
    pub summary: String,
    pub duplicates: Vec<DuplicateWarning>,
}

pub async fn submit(
    State(state): State<Arc<AppState>>,
    Path(ws_slug): Path<String>,
    headers: axum::http::HeaderMap,
    Json(input): Json<SubmitRequest>,
) -> Result<Json<SubmitResponse>> {
    // Membership + write permission, and the branch must be the caller's own draft —
    // submit force-rewrites the named ref (rebase), so an arbitrary branch here would
    // let any user rewrite another user's branch or a frozen pr/* snapshot.
    let guard = crate::routes::guard::require_membership(&state, &headers, &ws_slug).await?;
    crate::routes::guard::require(&guard, crate::routes::guard::Permission::EditContent)?;
    super::pages::require_own_branch(&input.branch, guard.user.id)?;
    let user = guard.user.clone();
    let repo = state.repo_manager.get(&ws_slug).map_err(|e| {
        tracing::error!(
            ws_slug = %ws_slug,
            error = %e,
            "submit: failed to get repo"
        );
        AppError::Internal(format!("repo error: {e}"))
    })?;
    super::pages::ensure_user_branch_if_needed(&repo, &input.branch)?;

    // Mandatory pre-submit rebase: bring the branch up to date with main. A conflict
    // blocks submit — the author rebases (resolves) first, then resubmits.
    let rebase_result = repo.rebase_onto_main(&input.branch).map_err(|e| {
        tracing::error!(
            branch = %input.branch,
            error = %e,
            "submit: rebase failed"
        );
        AppError::Internal(e.to_string())
    })?;
    if let cowiki_core::git::RebaseOutcome::Conflict(paths) = &rebase_result {
        return Err(AppError::Conflict(format!(
            "your branch conflicts with main; rebase and resolve first: {}",
            paths.join(", ")
        )));
    }

    let diffs = match repo.diff_files(&input.branch, &input.page_slugs) {
        Ok(d) => d,
        Err(e) => {
            tracing::error!(
                "failed to diff files for branch={} slugs={:?}: {}",
                &input.branch,
                &input.page_slugs,
                e
            );
            return Err(AppError::Internal(e.to_string()));
        }
    };

    // Generate embeddings for dedup.
    // _index slugs represent folders; they may be synthetic (no backing .md file)
    // when the folder exists but has no _index.md. Skip them gracefully.
    let mut embeddings = Vec::new();
    for slug in &input.page_slugs {
        let path = format!("wiki/{slug}.md");
        let content = match repo.read_file(&input.branch, &path) {
            Ok(Some(c)) => c,
            Ok(None) => {
                if slug.ends_with("/_index") {
                    continue;
                }
                tracing::error!(
                    slug = %slug,
                    branch = %input.branch,
                    "submit: page not found"
                );
                return Err(AppError::BadRequest(format!("page {slug} not found")));
            }
            Err(e) => {
                tracing::error!(
                    slug = %slug,
                    branch = %input.branch,
                    error = %e,
                    "submit: failed to read page file"
                );
                return Err(AppError::Internal(e.to_string()));
            }
        };
        let text = String::from_utf8_lossy(&content);
        super::pages::require_page_title(&text)?;
        match state.compiler.embed(&text).await {
            Ok(emb) => {
                embeddings.push((slug.clone(), emb));
            }
            Err(e) => {
                tracing::warn!(
                    slug = %slug,
                    error = %e,
                    "submit: embedding failed, skipping dedup"
                );
            }
        }
    }

    // Check for duplicates
    let mut duplicates = Vec::new();
    for (slug, emb) in &embeddings {
        match cowiki_db::pages::find_similar(&state.db, emb, "main", 3, 0.85, Some(&ws_slug)).await
        {
            Ok(similar) => {
                for (page, score) in similar {
                    if page.slug != *slug {
                        duplicates.push(cowiki_core::models::DuplicateWarning {
                            new_slug: slug.clone(),
                            existing_slug: page.slug,
                            similarity: score,
                        });
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    slug = %slug,
                    error = %e,
                    "submit: duplicate check failed"
                );
            }
        }
    }

    // Submit returns immediately with a diff-based summary; the AI one-liner is generated
    // in the background after the submission is created (see below), so a slow or
    // unreachable LLM never blocks submit.
    let summary = diffs
        .iter()
        .map(|d| {
            if d.is_new() {
                format!("+ new: {}", d.path)
            } else {
                format!("~ modified: {}", d.path)
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    if input.skip_review {
        // Authorization: skip_review only allowed for personal workspaces (private + owner)
        let ws = cowiki_db::workspaces::find_by_slug(&state.db, &ws_slug)
            .await
            .map_err(|e| {
                tracing::error!(
                    ws_slug = %ws_slug,
                    error = %e,
                    "submit: failed to find workspace"
                );
                e
            })?
            .ok_or_else(|| {
                tracing::error!(
                    ws_slug = %ws_slug,
                    "submit: workspace not found"
                );
                AppError::NotFound("workspace not found".into())
            })?;
        if ws.visibility != "private" {
            return Err(AppError::Forbidden(
                "skip_review is only allowed for personal (private) workspaces".into(),
            ));
        }
        let role = cowiki_db::workspaces::get_member_role(&state.db, ws.id, user.id)
            .await
            .map_err(|e| {
                tracing::error!(
                    ws_id = %ws.id,
                    user_id = %user.id,
                    error = %e,
                    "submit: failed to get member role"
                );
                e
            })?
            .unwrap_or_default();
        if role != "owner" {
            return Err(AppError::Forbidden(
                "skip_review is only allowed for the workspace owner".into(),
            ));
        }

        // Personal Space: snapshot the whole branch and merge straight to main. One
        // writer, so this never conflicts — every submit is effectively a commit.
        // Unique per submit: a branch-derived id would collide across concurrent submits
        // (double-click/retry) — one request's cleanup would delete the ref the other is
        // about to merge.
        let pr_id = format!("personal-{}", uuid::Uuid::new_v4());
        repo.create_pr_snapshot(&input.branch, &pr_id)
            .map_err(|e| {
                tracing::error!(
                    pr_id = %pr_id,
                    branch = %input.branch,
                    error = %e,
                    "submit: failed to create PR snapshot"
                );
                AppError::Internal(e.to_string())
            })?;
        let outcome = repo
            .merge_pr(&pr_id, &user.name, &format!("commit: {summary}"))
            .map_err(|e| {
                tracing::error!(
                    pr_id = %pr_id,
                    error = %e,
                    "submit: merge_pr failed"
                );
                AppError::Internal(e.to_string())
            })?;
        tracing::debug!(pr_id = %pr_id, "submit: cleaning up submission");
        repo.cleanup_submission(&pr_id);
        if let cowiki_core::git::MergeOutcome::Conflict(paths) = &outcome {
            tracing::warn!(
                pr_id = %pr_id,
                conflicts = ?paths,
                "submit: merge conflict"
            );
            return Err(AppError::Conflict(format!(
                "branch conflicts with main: {}",
                paths.join(", ")
            )));
        }
        // Catch the branch up so its untouched pages reflect the new main.
        let _ = repo.rebase_onto_main(&input.branch);

        return Ok(Json(SubmitResponse {
            submission_id: uuid::Uuid::nil(),
            summary,
            duplicates,
        }));
    }

    // Team Space: create a review submission, then freeze its reviewable snapshot
    // (`pr/{id}`) from the just-rebased branch. Review and merge read the snapshot, never
    // the live branch, so edits after submit can't change what was reviewed.
    let submission = cowiki_db::submissions::create(
        &state.db,
        user.id,
        &summary,
        &input.page_slugs,
        &input.branch,
        &ws_slug,
    )
    .await
    .map_err(|e| {
        tracing::error!(
            user_id = %user.id,
            ws_slug = %ws_slug,
            error = %e,
            "submit: failed to create submission in db"
        );
        e
    })?;
    repo.create_pr_snapshot(&input.branch, &submission.id.to_string())
        .map_err(|e| {
            tracing::error!(
                submission_id = %submission.id,
                branch = %input.branch,
                error = %e,
                "submit: failed to create PR snapshot"
            );
            AppError::Internal(e.to_string())
        })?;

    // Replace the diff-based placeholder summary with an AI one-liner in the background, so
    // submit itself never waits on the LLM. Failure just leaves the placeholder in place.
    {
        let state = state.clone();
        let sub_id = submission.id;
        let content = summary.clone();
        tokio::spawn(async move {
            tracing::debug!(
                submission_id = %sub_id,
                "submit: spawning background summary generation"
            );
            match state
                .compiler
                .generate_summary(&format!("Submission changes:\n{content}"))
                .await
            {
                Ok(s) => {
                    if let Err(e) =
                        cowiki_db::submissions::update_summary(&state.db, sub_id, &s).await
                    {
                        tracing::warn!("failed to store async summary for {sub_id}: {e}");
                    }
                }
                Err(e) => {
                    tracing::warn!("async summary generation failed for {sub_id}: {e}");
                }
            }
        });
    }

    Ok(Json(SubmitResponse {
        submission_id: submission.id,
        summary,
        duplicates,
    }))
}

#[derive(Deserialize)]
pub struct RebaseRequest {
    pub branch: String,
}

#[derive(Serialize)]
pub struct RebaseResponse {
    /// "up_to_date" | "updated" | "conflict"
    pub status: String,
    pub conflicts: Vec<String>,
}

/// Bring a user branch up to date with `main` (the "sync with main" button). Returns the
/// outcome; on conflict the branch is left untouched and the conflicting paths are listed
/// so the UI can guide the author to resolve.
pub async fn rebase(
    State(state): State<Arc<AppState>>,
    Path(ws_slug): Path<String>,
    headers: axum::http::HeaderMap,
    Json(input): Json<RebaseRequest>,
) -> Result<Json<RebaseResponse>> {
    // Same gate as submit: members with write permission only, and only on the caller's
    // own draft branch — rebase force-rewrites the ref, so pr/*, main, and other users'
    // branches must be unreachable from here.
    let guard = crate::routes::guard::require_membership(&state, &headers, &ws_slug).await?;
    crate::routes::guard::require(&guard, crate::routes::guard::Permission::EditContent)?;
    super::pages::require_own_branch(&input.branch, guard.user.id)?;
    let repo = state
        .repo_manager
        .get(&ws_slug)
        .map_err(|e| AppError::Internal(format!("repo error: {e}")))?;
    super::pages::ensure_user_branch_if_needed(&repo, &input.branch)?;

    let (status, conflicts) = match repo
        .rebase_onto_main(&input.branch)
        .map_err(|e| AppError::Internal(e.to_string()))?
    {
        cowiki_core::git::RebaseOutcome::UpToDate => ("up_to_date".to_string(), Vec::new()),
        cowiki_core::git::RebaseOutcome::Updated => ("updated".to_string(), Vec::new()),
        cowiki_core::git::RebaseOutcome::Conflict(paths) => ("conflict".to_string(), paths),
    };
    Ok(Json(RebaseResponse { status, conflicts }))
}
