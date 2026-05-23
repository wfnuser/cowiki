use axum::extract::State;
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
    headers: axum::http::HeaderMap,
    Json(input): Json<SubmitRequest>,
) -> Result<Json<SubmitResponse>> {
    let user = extract_user(&state.db, &headers).await?;

    let diffs = state
        .wiki_repo
        .diff_files(&input.branch, &input.page_slugs)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // Generate embeddings for dedup
    let mut embeddings = Vec::new();
    for slug in &input.page_slugs {
        let path = format!("wiki/{slug}.md");
        if let Some(content) = state
            .wiki_repo
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
        if let Ok(similar) =
            cowiki_db::pages::find_similar(&state.db, emb, "main", 3, 0.85).await
        {
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

    let summary = state
        .compiler
        .generate_summary(&format!("Submission changes:\n{diff_desc}"))
        .await
        .unwrap_or(diff_desc);

    if input.skip_review {
        // Personal Space: merge directly to main, no review needed
        let file_paths: Vec<String> = input.page_slugs.iter()
            .map(|s| format!("wiki/{s}.md"))
            .collect();
        state.wiki_repo
            .merge_to_main(&input.branch, &file_paths, &user.name, &format!("commit: {summary}"))
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
    )
    .await?;

    Ok(Json(SubmitResponse {
        submission_id: submission.id,
        summary,
        duplicates,
    }))
}
