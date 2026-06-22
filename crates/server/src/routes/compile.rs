use axum::extract::{Path, State};
use axum::Json;
use std::collections::HashMap;
use std::sync::Arc;

use cowiki_core::compile::{CompileRequest, CompileResponse};
use cowiki_core::dispatch::{AgentDispatcher, AgentResult};

use crate::error::{AppError, Result};
use crate::AppState;

// ── AgentDispatcher impl for AppState ──────────────────────────

#[async_trait::async_trait]
impl AgentDispatcher for AppState {
    async fn dispatch(
        &self,
        agent_name: &str,
        prompt: &str,
        skills: &[&str],
        workspace: &str,
        extra_args: HashMap<String, String>,
    ) -> std::result::Result<AgentResult, String> {
        // Resolve agent type to prepend correct slash command
        let agent_type = self
            .agent_manager
            .load_or_create_manifest(workspace, agent_name)
            .await
            .map(|m| m.agent.agent_type)
            .unwrap_or_else(|_| "pi".into());

        let final_prompt = match agent_type.as_str() {
            "copilot" => format!("/cowiki-compiler\n\n{}", prompt),
            _ => prompt.to_string(),
        };

        let result = self
            .agent_manager
            .execute(agent_name, &final_prompt, skills, workspace, extra_args)
            .await
            .map_err(|e| format!("agent dispatch: {e}"))?;

        Ok(result)
    }
}

// ── Route handler ──────────────────────────────────────────────

pub async fn compile_ws(
    State(state): State<Arc<AppState>>,
    Path(ws_slug): Path<String>,
    headers: axum::http::HeaderMap,
    Json(input): Json<CompileRequest>,
) -> Result<Json<CompileResponse>> {
    // Auth: membership + EditContent + own branch only.
    let guard = crate::routes::guard::require_membership(&state, &headers, &ws_slug).await?;
    crate::routes::guard::require(&guard, crate::routes::guard::Permission::EditContent)?;
    super::guard::require_own_branch(&input.branch, guard.user.id)?;
    let repo = state
        .repo_manager
        .get(&ws_slug)
        .map_err(|e| AppError::Internal(format!("repo error: {e}")))?;
    super::pages::ensure_user_branch_if_needed(&repo, &input.branch)?;

    let api_key = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");

    let compile_mode = std::env::var("COMPILE_MODE").unwrap_or_else(|_| "agent".into());
    let result = cowiki_core::compile::do_compile(
        state.as_ref(),
        &**state.llm,
        &**state.embedder,
        &repo,
        &ws_slug,
        &state.db,
        &input.branch,
        api_key,
        &compile_mode,
    )
    .await
    .map_err(AppError::Internal)?;

    Ok(Json(result))
}
