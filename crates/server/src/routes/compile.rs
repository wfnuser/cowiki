use axum::extract::{Path, State};
use axum::Json;
use std::sync::Arc;

use cowiki_core::compile::{CompileRequest, CompileResponse};
use cowiki_core::dispatch::{AgentDispatchResult, AgentDispatcher};

use crate::error::{AppError, Result};
use crate::AppState;

// ── AgentDispatcher impl for AppState ──────────────────────────

#[async_trait::async_trait]
impl AgentDispatcher for AppState {
    async fn dispatch_compile(
        &self,
        agent_name: &str,
        task_type: &str,
        user_input: &str,
        workspace: &str,
        branch: &str,
        source_scope: &[String],
    ) -> std::result::Result<AgentDispatchResult, String> {
        let ws = workspace.strip_prefix("repo/").unwrap_or(workspace);

        let task = cowiki_agents::types::protocol::AgentRequest {
            task_type: task_type.to_string(),
            session_id: None,
            system_prompt: String::new(),
            user_input: user_input.to_string(),
            workspace_path: ws.to_string(),
            branch: branch.to_string(),
            tools: vec![],
            output_schema: None,
            config: Default::default(),
            source_scope: source_scope.to_vec(),
        };

        let response = self
            .agent_manager
            .dispatch_agent(agent_name, task)
            .await
            .map_err(|e| format!("agent dispatch: {e}"))?;

        Ok(AgentDispatchResult {
            success: response.success,
            rounds: response.rounds,
            written_pages: response.written_pages,
            error: response.error,
        })
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
    // Compile writes pages to the named branch — members with write permission
    // only, and only onto the caller's own draft branch.
    let guard = crate::routes::guard::require_membership(&state, &headers, &ws_slug).await?;
    crate::routes::guard::require(&guard, crate::routes::guard::Permission::EditContent)?;
    super::guard::require_own_branch(&input.branch, guard.user.id)?;
    let repo = state
        .repo_manager
        .get(&ws_slug)
        .map_err(|e| AppError::Internal(format!("repo error: {e}")))?;
    super::pages::ensure_user_branch_if_needed(&repo, &input.branch)?;

    let result = cowiki_core::compile::do_compile(
        state.as_ref(),
        &state.compiler,
        &repo,
        &ws_slug,
        &state.wiki_fs_gateway,
        &state.db,
        &input.branch,
    )
    .await
    .map_err(AppError::Internal)?;

    Ok(Json(result))
}
