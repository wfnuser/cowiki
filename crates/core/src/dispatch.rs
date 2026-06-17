//! Agent-based compile dispatch — trait + shared types.
//!
//! The AgentDispatcher trait bridges core with the agents crate.
//! Server implements this trait to dispatch compile tasks to AgentManager.

use crate::git::WikiRepo;

/// Result of an agent-based compile.
#[derive(Debug, Clone)]
pub struct AgentCompileOutput {
    /// Paths of pages written by the agent (e.g. "wiki/docker.md")
    pub written_pages: Vec<String>,
    /// Content of each page as (path, body)
    pub pages: Vec<(String, String)>,
}

/// Trait for dispatching a task to an agent.
/// Server implements this to connect core with the agents crate.
#[async_trait::async_trait]
pub trait AgentDispatcher: Send + Sync {
    /// Dispatch a compile task and return the agent's response.
    async fn dispatch_compile(
        &self,
        agent_name: &str,
        task_type: &str,
        user_input: &str,
        workspace: &str,
        branch: &str,
        source_scope: &[String],
    ) -> Result<AgentDispatchResult, String>;
}

/// Result from an agent dispatch call.
#[derive(Debug, Clone)]
pub struct AgentDispatchResult {
    pub success: bool,
    pub rounds: u32,
    pub written_pages: Vec<String>,
    pub error: Option<String>,
}

/// Build a compile prompt from source files.
pub fn build_compile_prompt(sources: &[(String, String)]) -> String {
    let source_paths: Vec<String> = sources
        .iter()
        .map(|(name, _)| format!("sources/{}", name))
        .collect();
    format!(
        "/skill:compiler Use ONLY cowiki MCP direct tools for all wiki operations. \
         Do NOT use your built-in Write, Edit, Bash, or read tools for wiki content.\n\
         \n\
         Your task: compile the following source pages into a structured knowledge base.\n\
         \n\
         Sources:\n\
         {}\n\
         \n\
         Rules:\n\
         - Use cowiki_list()/cowiki_read()/cowiki_search() to explore (NOT built-in tools):\n\
         {}\n\
         \n\
         - One topic per wiki page. Split multi-topic sources. Max 5 wiki pages.\n\
         - Every page MUST have YAML frontmatter with title and summary.\n\
         - Use cowiki_write() for ALL page creation — never use built-in tools for writing.\n\
         - Use cowiki_list() and cowiki_read() to check before writing to avoid duplicates.",
        source_paths
            .iter()
            .map(|p| format!("- `{}`", p))
            .collect::<Vec<_>>()
            .join("\n"),
        source_paths
            .iter()
            .map(|p| format!("\t - `{}`", p))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

/// Dispatch a compile task via the agent trait and collect results from git.
pub async fn dispatch_and_collect(
    dispatcher: &dyn AgentDispatcher,
    agent_name: &str,
    repo: &WikiRepo,
    branch: &str,
    ws_slug: &str,
    new_sources: &[(String, String)],
    source_scope: &[String],
) -> Result<AgentCompileOutput, String> {
    let user_input = build_compile_prompt(new_sources);

    let result = dispatcher
        .dispatch_compile(
            agent_name,
            "compile",
            &user_input,
            &format!("repo/{}", ws_slug),
            branch,
            source_scope,
        )
        .await
        .map_err(|e| format!("agent dispatch failed: {e}"))?;

    if !result.success {
        return Err(result
            .error
            .unwrap_or_else(|| "agent returned failure".into()));
    }

    // ── Collect pages from MCP server tracking file ───────
    // The MCP server persists written page paths to
    // .cowiki/tracking.json after each tool call. This is the
    // single source of truth for what the agent wrote.
    let repo_root = repo.path();
    let (mcp_written, _mcp_removed) = crate::gateway::WikiFsGateway::read_tracking_file(repo_root);

    let mut pages = Vec::new();
    let mut written_pages: Vec<String> = Vec::new();

    if mcp_written.is_empty() {
        tracing::warn!("MCP tracking file empty or missing — no pages collected");
    } else {
        tracing::info!(
            count = mcp_written.len(),
            paths = ?mcp_written,
            "collecting pages from MCP server tracking"
        );
        for page_path in &mcp_written {
            match repo.read_file(branch, page_path) {
                Ok(Some(content)) => {
                    let text = String::from_utf8_lossy(&content).into_owned();
                    pages.push((page_path.clone(), text));
                    written_pages.push(page_path.clone());
                }
                Ok(None) => {
                    tracing::warn!("mcp tracking: {} reported but not on branch", page_path)
                }
                Err(e) => tracing::warn!("mcp tracking: failed to read {}: {}", page_path, e),
            }
        }
    }

    Ok(AgentCompileOutput {
        written_pages,
        pages,
    })
}
