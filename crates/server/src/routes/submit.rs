use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::{AppError, Result};
use crate::routes::auth::extract_user;
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
    let user = extract_user(&state.db, &headers).await?;
    let repo = state
        .repo_manager
        .get(&ws_slug)
        .map_err(|e| AppError::Internal(format!("repo error: {e}")))?;
    super::pages::ensure_user_branch_if_needed(&repo, &input.branch)?;

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

    // Generate embeddings for dedup
    let mut embeddings = Vec::new();
    for slug in &input.page_slugs {
        let path = format!("wiki/{slug}.md");
        if let Some(content) = repo
            .read_file(&input.branch, &path)
            .map_err(|e| AppError::Internal(e.to_string()))?
        {
            let text = String::from_utf8_lossy(&content);
            if let Ok(emb) = state.compiler.embed(&text).await {
                embeddings.push((slug.clone(), emb));
            }
        }
    }

    // Check for duplicates
    let mut duplicates = Vec::new();
    for (slug, emb) in &embeddings {
        if let Ok(similar) = cowiki_db::pages::find_similar(&state.db, emb, "main", 3, 0.85).await {
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
    }

    let diff_desc = diffs
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

    let summary = match state
        .compiler
        .generate_summary(&format!("Submission changes:\n{diff_desc}"))
        .await
    {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("summary generation failed, falling back to diff description: {e}");
            diff_desc
        }
    };

    if input.skip_review {
        // Authorization: skip_review only allowed for personal workspaces (private + owner)
        let ws = cowiki_db::workspaces::find_by_slug(&state.db, &ws_slug)
            .await?
            .ok_or_else(|| AppError::NotFound("workspace not found".into()))?;
        if ws.visibility != "private" {
            return Err(AppError::Forbidden(
                "skip_review is only allowed for personal (private) workspaces".into(),
            ));
        }
        let role = cowiki_db::workspaces::get_member_role(&state.db, ws.id, user.id)
            .await?
            .unwrap_or_default();
        if role != "owner" {
            return Err(AppError::Forbidden(
                "skip_review is only allowed for the workspace owner".into(),
            ));
        }

        // Personal Space: merge directly to main, no review needed
        let file_paths: Vec<String> = input
            .page_slugs
            .iter()
            .map(|s| format!("wiki/{s}.md"))
            .collect();
        repo.merge_to_main(
            &input.branch,
            &file_paths,
            &user.name,
            &format!("commit: {summary}"),
        )
        .map_err(|e| AppError::Internal(e.to_string()))?;

        return Ok(Json(SubmitResponse {
            submission_id: uuid::Uuid::nil(),
            summary,
            duplicates,
        }));
    }

    // Team Space: create a review submission
    let submission = cowiki_db::submissions::create(
        &state.db,
        user.id,
        &summary,
        &input.page_slugs,
        &input.branch,
        &ws_slug,
    )
    .await?;

    Ok(Json(SubmitResponse {
        submission_id: submission.id,
        summary,
        duplicates,
    }))
}
