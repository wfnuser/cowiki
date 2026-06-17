//! cowiki MCP Server — rmcp-based, 5 wiki primitives.
//!
//! Starts a standalone Streamable HTTP server that agents (pi, claude)
//! connect to for tool discovery and execution.
//!
//! No constraints — agents have full tool access within their workspace.
//! Workspace/branch passed per-tool-call via optional _workspace/_branch params.
//! pi sets these from COWIKI_WORKSPACE, COWIKI_BRANCH, COWIKI_EXECUTION_ID env vars.
//! Write/remove tracking via WikiFsGateway (reset before dispatch, take after).

use std::sync::Arc;

use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router, ErrorData, ServerHandler,
};
use serde::{Deserialize, Serialize};

use cowiki_core::gateway::WikiFsGateway;

#[derive(Clone)]
pub struct CowikiMcp {
    gateway: Arc<WikiFsGateway>,
}

impl CowikiMcp {
    pub fn new(gateway: Arc<WikiFsGateway>) -> Self {
        Self { gateway }
    }
}

// ── Agent context (extracted from tool params) ────────────────

#[derive(Debug, Default, Deserialize, Serialize)]
struct CtxParams {
    #[serde(default)]
    _workspace: Option<String>,
    #[serde(default)]
    _branch: Option<String>,
    #[serde(default)]
    _execution_id: Option<String>,
}

fn resolve_ctx<T: Serialize + for<'de> Deserialize<'de>>(p: &T) -> CtxParams {
    serde_json::from_value(serde_json::to_value(p).unwrap_or_default()).unwrap_or_default()
}

// ── Tool params (all include ctx fields for agent to pass) ────

#[derive(Debug, Deserialize, schemars::JsonSchema, Serialize)]
pub struct ListParams {
    pub dir: String,
    #[serde(default)]
    pub recursive: bool,
    #[serde(default)]
    pub _workspace: Option<String>,
    #[serde(default)]
    pub _branch: Option<String>,
    #[serde(default)]
    pub _execution_id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema, Serialize)]
pub struct ReadParams {
    pub path: String,
    #[serde(default)]
    pub _workspace: Option<String>,
    #[serde(default)]
    pub _branch: Option<String>,
    #[serde(default)]
    pub _execution_id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema, Serialize)]
pub struct WriteParams {
    pub path: String,
    pub body: String,
    #[serde(default)]
    pub _workspace: Option<String>,
    #[serde(default)]
    pub _branch: Option<String>,
    #[serde(default)]
    pub _execution_id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema, Serialize)]
pub struct RemoveParams {
    pub path: String,
    #[serde(default)]
    pub _workspace: Option<String>,
    #[serde(default)]
    pub _branch: Option<String>,
    #[serde(default)]
    pub _execution_id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema, Serialize)]
pub struct SearchParams {
    pub query: String,
    #[serde(default)]
    pub top_k: Option<usize>,
    #[serde(default)]
    pub _workspace: Option<String>,
    #[serde(default)]
    pub _branch: Option<String>,
    #[serde(default)]
    pub _execution_id: Option<String>,
}

// ── Tools ─────────────────────────────────────────────────────

#[tool_router]
impl CowikiMcp {
    #[tool(description = "List wiki pages, source folders, or directory contents")]
    async fn list(
        &self,
        Parameters(p): Parameters<ListParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let ctx = resolve_ctx(&p);
        let ws = ctx._workspace.as_deref().unwrap_or("default");
        let branch = ctx._branch.as_deref().unwrap_or("main");
        let eid = ctx._execution_id.as_deref().unwrap_or("-");
        let args = if p.recursive {
            serde_json::json!({"dir": p.dir, "recursive": true})
        } else {
            serde_json::json!({"dir": p.dir})
        };
        tracing::info!(tool="list", dir=%p.dir, recursive=p.recursive, ws, branch, eid, "MCP tool call");
        let r = self.gateway.execute("list", args, ws, branch);
        tracing::info!(tool="list", success=r.success, summary=?r.summary, eid, "MCP tool result");
        Ok(json_result(r.output))
    }

    #[tool(description = "Read a single wiki page or source file")]
    async fn read(
        &self,
        Parameters(p): Parameters<ReadParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let ctx = resolve_ctx(&p);
        let ws = ctx._workspace.as_deref().unwrap_or("default");
        let branch = ctx._branch.as_deref().unwrap_or("main");
        let eid = ctx._execution_id.as_deref().unwrap_or("-");
        tracing::info!(tool="read", path=%p.path, ws, branch, eid, "MCP tool call");
        let r = self
            .gateway
            .execute("read", serde_json::json!({"path": p.path}), ws, branch);
        tracing::info!(tool="read", success=r.success, summary=?r.summary, eid, "MCP tool result");
        Ok(json_result(r.output))
    }

    #[tool(description = "Write or update a wiki page with YAML frontmatter")]
    async fn write(
        &self,
        Parameters(p): Parameters<WriteParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let ctx = resolve_ctx(&p);
        let ws = ctx._workspace.as_deref().unwrap_or("default");
        let branch = ctx._branch.as_deref().unwrap_or("main");
        let eid = ctx._execution_id.as_deref().unwrap_or("-");
        let body_len = p.body.len();
        tracing::info!(tool="write", path=%p.path, body_len, ws, branch, eid, "MCP tool call");
        let r = self.gateway.execute(
            "write",
            serde_json::json!({"path": p.path, "body": p.body}),
            ws,
            branch,
        );
        tracing::info!(tool="write", success=r.success, summary=?r.summary, eid, "MCP tool result");
        Ok(json_result(r.output))
    }

    #[tool(description = "Delete a wiki page or file")]
    async fn remove(
        &self,
        Parameters(p): Parameters<RemoveParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let ctx = resolve_ctx(&p);
        let ws = ctx._workspace.as_deref().unwrap_or("default");
        let branch = ctx._branch.as_deref().unwrap_or("main");
        let eid = ctx._execution_id.as_deref().unwrap_or("-");
        tracing::info!(tool="remove", path=%p.path, ws, branch, eid, "MCP tool call");
        let r = self
            .gateway
            .execute("remove", serde_json::json!({"path": p.path}), ws, branch);
        tracing::info!(tool="remove", success=r.success, summary=?r.summary, eid, "MCP tool result");
        Ok(json_result(r.output))
    }

    #[tool(description = "Full-text or vector search across wiki pages")]
    async fn search(
        &self,
        Parameters(p): Parameters<SearchParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let ctx = resolve_ctx(&p);
        let ws = ctx._workspace.as_deref().unwrap_or("default");
        let branch = ctx._branch.as_deref().unwrap_or("main");
        let eid = ctx._execution_id.as_deref().unwrap_or("-");
        let top_k = p.top_k.unwrap_or(10);
        tracing::info!(tool="search", query=%p.query, top_k, ws, branch, eid, "MCP tool call");
        let r = self.gateway.execute(
            "search",
            serde_json::json!({"query": p.query, "top_k": top_k}),
            ws,
            branch,
        );
        tracing::info!(tool="search", success=r.success, summary=?r.summary, eid, "MCP tool result");
        Ok(json_result(r.output))
    }
}

#[tool_handler]
impl ServerHandler for CowikiMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
    }
}

fn json_result(v: serde_json::Value) -> CallToolResult {
    CallToolResult::success(vec![Content::json(
        serde_json::to_string(&v).unwrap_or_default(),
    )
    .unwrap_or_else(|_| Content::text("{}".to_string()))])
}

/// Start the MCP server on the given bind address (e.g., "127.0.0.1:9380").
/// Runs until the tokio runtime is dropped (no separate Ctrl+C handler).
pub async fn start_mcp_server(gateway: Arc<WikiFsGateway>, bind_addr: &str) -> anyhow::Result<()> {
    use hyper_util::{
        rt::TokioExecutor, server::conn::auto::Builder, service::TowerToHyperService,
    };
    use rmcp::transport::streamable_http_server::{
        session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    };

    tracing::info!("MCP server starting on {bind_addr}");

    let mcp = CowikiMcp::new(gateway);
    let mcp_service: StreamableHttpService<CowikiMcp, LocalSessionManager> =
        StreamableHttpService::new(
            move || Ok(mcp.clone()),
            LocalSessionManager::default().into(),
            StreamableHttpServerConfig::default(),
        );

    let tower_service = TowerToHyperService::new(mcp_service);
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    tracing::info!("MCP server listening on {bind_addr}");

    loop {
        let (stream, addr) = tokio::select! {
            accept = listener.accept() => match accept {
                Ok(c) => c,
                Err(e) => { tracing::error!("accept: {e}"); continue; }
            },
        };
        let svc = tower_service.clone();
        tokio::spawn(async move {
            if let Err(e) = Builder::new(TokioExecutor::default())
                .serve_connection(hyper_util::rt::TokioIo::new(stream), svc)
                .await
            {
                if !e.to_string().contains("connection") {
                    tracing::error!("conn {addr}: {e}");
                }
            }
        });
    }
}
