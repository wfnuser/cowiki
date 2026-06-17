//! CowikiServer — MCP tools proxy to cowiki-server REST API.

use std::collections::HashMap;
use std::sync::Arc;

use rmcp::{
    ErrorData, ServerHandler,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
    service::RequestContext,
    RoleServer,
};
use serde::Deserialize;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct CowikiServer {
    client: reqwest::Client,
    api_base: Arc<String>,
    /// Cache: auth_header_hash → user_branch
    branch_cache: Arc<Mutex<HashMap<String, String>>>,
}

impl CowikiServer {
    pub fn new(api_base: String) -> Self {
        let api_base = api_base.trim_end_matches('/').to_string();
        Self {
            client: reqwest::Client::new(),
            api_base: Arc::new(api_base),
            branch_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn bearer(&self, ctx: &RequestContext<RoleServer>) -> Result<String, ErrorData> {
        ctx.extensions.get::<http::request::Parts>()
            .and_then(|p| p.headers.get("authorization"))
            .and_then(|v: &http::HeaderValue| v.to_str().ok())
            .map(|s| s.to_string())
            .ok_or_else(|| ErrorData::internal_error("missing Authorization", None))
    }

    /// Resolve user's personal branch, cached per auth token.
    async fn user_branch(&self, auth: &str) -> Result<String, ErrorData> {
        // Use first 32 chars as cache key (enough to be unique)
        let key = &auth[..auth.len().min(64)];

        {
            let cache = self.branch_cache.lock().await;
            if let Some(branch) = cache.get(key) {
                return Ok(branch.clone());
            }
        }

        let resp = self.client.get(format!("{}/api/auth/me", self.api_base))
            .header("Authorization", auth)
            .send().await.map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        if !resp.status().is_success() {
            return Err(ErrorData::internal_error("failed to get user info", None));
        }
        let user: serde_json::Value = resp.json().await.unwrap_or_default();
        let uid = user["id"].as_str().unwrap_or("default");
        let branch = format!("user/{uid}");

        self.branch_cache.lock().await.insert(key.to_string(), branch.clone());
        Ok(branch)
    }

    async fn get(&self, auth: &str, path: &str) -> Result<serde_json::Value, ErrorData> {
        let resp = self.client.get(format!("{}/{path}", self.api_base))
            .header("Authorization", auth).send().await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        let s = resp.status();
        let body: serde_json::Value = resp.json().await.unwrap_or_default();
        if !s.is_success() { return Err(ErrorData::internal_error(format!("API {s}: {body}"), None)); }
        Ok(body)
    }

    async fn post(&self, auth: &str, path: &str, body: &serde_json::Value) -> Result<serde_json::Value, ErrorData> {
        let resp = self.client.post(format!("{}/{path}", self.api_base))
            .header("Authorization", auth).json(body).send().await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        let s = resp.status();
        let b: serde_json::Value = resp.json().await.unwrap_or_default();
        if !s.is_success() { return Err(ErrorData::internal_error(format!("API {s}: {b}"), None)); }
        Ok(b)
    }

    async fn delete(&self, auth: &str, path: &str) -> Result<serde_json::Value, ErrorData> {
        let resp = self.client.delete(format!("{}/{path}", self.api_base))
            .header("Authorization", auth).send().await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        let s = resp.status();
        let body: serde_json::Value = resp.json().await.unwrap_or_default();
        if !s.is_success() { return Err(ErrorData::internal_error(format!("API {s}: {body}"), None)); }
        Ok(body)
    }

    fn json_result(v: serde_json::Value) -> Result<CallToolResult, ErrorData> {
        Ok(CallToolResult::success(vec![Content::json(serde_json::to_string(&v).unwrap_or_default())?]))
    }
}

// ── Params (no branch — derived from user) ────────────────────

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IngestParams { pub source_type: String, pub content: String, pub filename: Option<String>, }

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CompileParams {} // no params needed

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReadParams { pub slug: String, }

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WriteParams { pub slug: String, pub body: String, pub title: Option<String>, pub summary: Option<String>, }

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListParams {} // no params — always main

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchParams { pub query: String, pub limit: Option<i64>, }

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SubmitParams { pub paths: Vec<String>, }

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReviewGetParams { pub id: String, }

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReviewDecideParams { pub id: String, pub action: String, }

// ── Workspace params ──────────────────────────────────────────

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WorkspaceCreateParams {
    pub name: String,
    pub slug: String,
    pub visibility: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WorkspaceSlugParams { pub workspace_slug: String, }

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WorkspaceRenameParams {
    pub workspace_slug: String,
    pub name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WorkspaceInviteParams {
    pub workspace_slug: String,
    pub email: String,
    pub role: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WorkspaceRemoveMemberParams {
    pub workspace_slug: String,
    pub user_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WorkspaceChangeRoleParams {
    pub workspace_slug: String,
    pub user_id: String,
    pub role: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct InvitationIdParams { pub invitation_id: String, }

// ── Workspace-scoped wiki params ──────────────────────────────

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WorkspacePageReadParams {
    pub workspace_slug: String,
    pub page_slug: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WorkspacePageWriteParams {
    pub workspace_slug: String,
    pub slug: String,
    pub body: String,
    pub title: Option<String>,
    pub summary: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WorkspaceIngestParams {
    pub workspace_slug: String,
    pub source_type: String,
    pub content: String,
    pub filename: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WorkspaceCompileParams { pub workspace_slug: String, }

// ── Tools ─────────────────────────────────────────────────────

#[tool_router]
impl CowikiServer {
    #[tool(description = "Ingest a source document (URL, text, or file) into your personal wiki space")]
    async fn cowiki_ingest(&self, ctx: RequestContext<RoleServer>, Parameters(p): Parameters<IngestParams>) -> Result<CallToolResult, ErrorData> {
        let auth = self.bearer(&ctx)?;
        let branch = self.user_branch(&auth).await?;
        self.post(&auth, "api/ingest", &serde_json::json!({
            "source_type": p.source_type, "content": p.content,
            "filename": p.filename, "branch": branch,
        })).await.and_then(Self::json_result)
    }

    #[tool(description = "Compile source documents into structured wiki pages using LLM")]
    async fn cowiki_compile(&self, ctx: RequestContext<RoleServer>, Parameters(_p): Parameters<CompileParams>) -> Result<CallToolResult, ErrorData> {
        let auth = self.bearer(&ctx)?;
        let branch = self.user_branch(&auth).await?;
        self.post(&auth, "api/compile", &serde_json::json!({"branch": branch})).await.and_then(Self::json_result)
    }

    #[tool(description = "Read a wiki page by its slug (from main space)")]
    async fn cowiki_read(&self, ctx: RequestContext<RoleServer>, Parameters(p): Parameters<ReadParams>) -> Result<CallToolResult, ErrorData> {
        let auth = self.bearer(&ctx)?;
        self.get(&auth, &format!("api/pages/{}", p.slug)).await.and_then(Self::json_result)
    }

    #[tool(description = "Create or edit a wiki page in your personal space")]
    async fn cowiki_write(&self, ctx: RequestContext<RoleServer>, Parameters(p): Parameters<WriteParams>) -> Result<CallToolResult, ErrorData> {
        let auth = self.bearer(&ctx)?;
        let branch = self.user_branch(&auth).await?;
        self.post(&auth, "api/pages", &serde_json::json!({
            "slug": p.slug, "body": p.body,
            "title": p.title, "summary": p.summary,
            "branch": branch,
        })).await.and_then(Self::json_result)
    }

    #[tool(description = "List all wiki pages in main space")]
    async fn cowiki_list(&self, ctx: RequestContext<RoleServer>, Parameters(_p): Parameters<ListParams>) -> Result<CallToolResult, ErrorData> {
        let auth = self.bearer(&ctx)?;
        self.get(&auth, "api/pages").await.and_then(Self::json_result)
    }

    #[tool(description = "Semantic search across wiki pages in main space")]
    async fn cowiki_search(&self, ctx: RequestContext<RoleServer>, Parameters(p): Parameters<SearchParams>) -> Result<CallToolResult, ErrorData> {
        let auth = self.bearer(&ctx)?;
        let limit = p.limit.unwrap_or(10);
        self.get(&auth, &format!("api/search?q={q}&limit={limit}", q = p.query)).await.and_then(Self::json_result)
    }

    #[tool(description = "Submit pages from personal space to review queue")]
    async fn cowiki_submit(&self, ctx: RequestContext<RoleServer>, Parameters(p): Parameters<SubmitParams>) -> Result<CallToolResult, ErrorData> {
        let auth = self.bearer(&ctx)?;
        let branch = self.user_branch(&auth).await?;
        self.post(&auth, "api/submit", &serde_json::json!({"paths": p.paths, "branch": branch})).await.and_then(Self::json_result)
    }

    #[tool(description = "List all pending submissions in the review queue")]
    async fn cowiki_review_list(&self, ctx: RequestContext<RoleServer>) -> Result<CallToolResult, ErrorData> {
        let auth = self.bearer(&ctx)?;
        self.get(&auth, "api/reviews").await.and_then(Self::json_result)
    }

    #[tool(description = "Get review details with file diffs")]
    async fn cowiki_review_get(&self, ctx: RequestContext<RoleServer>, Parameters(p): Parameters<ReviewGetParams>) -> Result<CallToolResult, ErrorData> {
        let auth = self.bearer(&ctx)?;
        self.get(&auth, &format!("api/reviews/{}", p.id)).await.and_then(Self::json_result)
    }

    #[tool(description = "Approve or reject a pending submission")]
    async fn cowiki_review_decide(&self, ctx: RequestContext<RoleServer>, Parameters(p): Parameters<ReviewDecideParams>) -> Result<CallToolResult, ErrorData> {
        let auth = self.bearer(&ctx)?;
        self.post(&auth, &format!("api/reviews/{}", p.id), &serde_json::json!({"action": p.action})).await.and_then(Self::json_result)
    }

    // ── Workspace management ──────────────────────────────────

    #[tool(description = "List all workspaces you belong to")]
    async fn cowiki_workspace_list(&self, ctx: RequestContext<RoleServer>) -> Result<CallToolResult, ErrorData> {
        let auth = self.bearer(&ctx)?;
        self.get(&auth, "api/workspaces").await.and_then(Self::json_result)
    }

    #[tool(description = "Create a new workspace")]
    async fn cowiki_workspace_create(&self, ctx: RequestContext<RoleServer>, Parameters(p): Parameters<WorkspaceCreateParams>) -> Result<CallToolResult, ErrorData> {
        let auth = self.bearer(&ctx)?;
        self.post(&auth, "api/workspaces", &serde_json::json!({
            "name": p.name, "slug": p.slug, "visibility": p.visibility,
        })).await.and_then(Self::json_result)
    }

    #[tool(description = "Join a public workspace by its slug")]
    async fn cowiki_workspace_join(&self, ctx: RequestContext<RoleServer>, Parameters(p): Parameters<WorkspaceSlugParams>) -> Result<CallToolResult, ErrorData> {
        let auth = self.bearer(&ctx)?;
        self.post(&auth, &format!("api/workspaces/{}/join", p.workspace_slug), &serde_json::json!({})).await.and_then(Self::json_result)
    }

    #[tool(description = "Rename a workspace (owner only)")]
    async fn cowiki_workspace_rename(&self, ctx: RequestContext<RoleServer>, Parameters(p): Parameters<WorkspaceRenameParams>) -> Result<CallToolResult, ErrorData> {
        let auth = self.bearer(&ctx)?;
        self.post(&auth, &format!("api/workspaces/{}/rename", p.workspace_slug), &serde_json::json!({"name": p.name})).await.and_then(Self::json_result)
    }

    #[tool(description = "Delete a workspace (owner only)")]
    async fn cowiki_workspace_delete(&self, ctx: RequestContext<RoleServer>, Parameters(p): Parameters<WorkspaceSlugParams>) -> Result<CallToolResult, ErrorData> {
        let auth = self.bearer(&ctx)?;
        self.delete(&auth, &format!("api/workspaces/{}", p.workspace_slug)).await.and_then(Self::json_result)
    }

    // ── Member management ─────────────────────────────────────

    #[tool(description = "Invite a user to a workspace by email (owner only)")]
    async fn cowiki_workspace_invite(&self, ctx: RequestContext<RoleServer>, Parameters(p): Parameters<WorkspaceInviteParams>) -> Result<CallToolResult, ErrorData> {
        let auth = self.bearer(&ctx)?;
        self.post(&auth, &format!("api/workspaces/{}/invite", p.workspace_slug), &serde_json::json!({
            "email": p.email, "role": p.role,
        })).await.and_then(Self::json_result)
    }

    #[tool(description = "List members of a workspace")]
    async fn cowiki_workspace_members(&self, ctx: RequestContext<RoleServer>, Parameters(p): Parameters<WorkspaceSlugParams>) -> Result<CallToolResult, ErrorData> {
        let auth = self.bearer(&ctx)?;
        self.get(&auth, &format!("api/workspaces/{}/members", p.workspace_slug)).await.and_then(Self::json_result)
    }

    #[tool(description = "Remove a member from a workspace (owner only)")]
    async fn cowiki_workspace_remove_member(&self, ctx: RequestContext<RoleServer>, Parameters(p): Parameters<WorkspaceRemoveMemberParams>) -> Result<CallToolResult, ErrorData> {
        let auth = self.bearer(&ctx)?;
        self.post(&auth, &format!("api/workspaces/{}/members/remove", p.workspace_slug), &serde_json::json!({
            "user_id": p.user_id,
        })).await.and_then(Self::json_result)
    }

    #[tool(description = "Change a member's role in a workspace (owner only)")]
    async fn cowiki_workspace_change_role(&self, ctx: RequestContext<RoleServer>, Parameters(p): Parameters<WorkspaceChangeRoleParams>) -> Result<CallToolResult, ErrorData> {
        let auth = self.bearer(&ctx)?;
        self.post(&auth, &format!("api/workspaces/{}/members/role", p.workspace_slug), &serde_json::json!({
            "user_id": p.user_id, "role": p.role,
        })).await.and_then(Self::json_result)
    }

    // ── Invitations ───────────────────────────────────────────

    #[tool(description = "List your pending workspace invitations")]
    async fn cowiki_invitation_list(&self, ctx: RequestContext<RoleServer>) -> Result<CallToolResult, ErrorData> {
        let auth = self.bearer(&ctx)?;
        self.get(&auth, "api/invitations/pending").await.and_then(Self::json_result)
    }

    #[tool(description = "Accept a pending workspace invitation")]
    async fn cowiki_invitation_accept(&self, ctx: RequestContext<RoleServer>, Parameters(p): Parameters<InvitationIdParams>) -> Result<CallToolResult, ErrorData> {
        let auth = self.bearer(&ctx)?;
        self.post(&auth, &format!("api/invitations/{}/accept", p.invitation_id), &serde_json::json!({})).await.and_then(Self::json_result)
    }

    #[tool(description = "Reject a pending workspace invitation")]
    async fn cowiki_invitation_reject(&self, ctx: RequestContext<RoleServer>, Parameters(p): Parameters<InvitationIdParams>) -> Result<CallToolResult, ErrorData> {
        let auth = self.bearer(&ctx)?;
        self.post(&auth, &format!("api/invitations/{}/reject", p.invitation_id), &serde_json::json!({})).await.and_then(Self::json_result)
    }

    // ── Workspace-scoped wiki operations ──────────────────────

    #[tool(description = "List all wiki pages in a workspace")]
    async fn cowiki_workspace_pages(&self, ctx: RequestContext<RoleServer>, Parameters(p): Parameters<WorkspaceSlugParams>) -> Result<CallToolResult, ErrorData> {
        let auth = self.bearer(&ctx)?;
        self.get(&auth, &format!("api/workspaces/{}/pages", p.workspace_slug)).await.and_then(Self::json_result)
    }

    #[tool(description = "Read a wiki page in a workspace by its slug")]
    async fn cowiki_workspace_read(&self, ctx: RequestContext<RoleServer>, Parameters(p): Parameters<WorkspacePageReadParams>) -> Result<CallToolResult, ErrorData> {
        let auth = self.bearer(&ctx)?;
        self.get(&auth, &format!("api/workspaces/{}/pages/{}", p.workspace_slug, p.page_slug)).await.and_then(Self::json_result)
    }

    #[tool(description = "Create or edit a wiki page in a workspace")]
    async fn cowiki_workspace_write(&self, ctx: RequestContext<RoleServer>, Parameters(p): Parameters<WorkspacePageWriteParams>) -> Result<CallToolResult, ErrorData> {
        let auth = self.bearer(&ctx)?;
        self.post(&auth, &format!("api/workspaces/{}/pages", p.workspace_slug), &serde_json::json!({
            "slug": p.slug, "body": p.body,
            "title": p.title, "summary": p.summary,
        })).await.and_then(Self::json_result)
    }

    #[tool(description = "Ingest a source document into a workspace")]
    async fn cowiki_workspace_ingest(&self, ctx: RequestContext<RoleServer>, Parameters(p): Parameters<WorkspaceIngestParams>) -> Result<CallToolResult, ErrorData> {
        let auth = self.bearer(&ctx)?;
        self.post(&auth, &format!("api/workspaces/{}/ingest", p.workspace_slug), &serde_json::json!({
            "source_type": p.source_type, "content": p.content,
            "filename": p.filename,
        })).await.and_then(Self::json_result)
    }

    #[tool(description = "Compile source documents into wiki pages in a workspace using LLM")]
    async fn cowiki_workspace_compile(&self, ctx: RequestContext<RoleServer>, Parameters(p): Parameters<WorkspaceCompileParams>) -> Result<CallToolResult, ErrorData> {
        let auth = self.bearer(&ctx)?;
        self.post(&auth, &format!("api/workspaces/{}/compile", p.workspace_slug), &serde_json::json!({})).await.and_then(Self::json_result)
    }
}

#[tool_handler(router = CowikiServer::tool_router())]
impl ServerHandler for CowikiServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
    }
}
