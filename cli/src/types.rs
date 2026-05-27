use serde::{Deserialize, Serialize};

// ── Auth ──────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct RegisterRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AuthResponse {
    pub user: UserInfo,
    pub api_key: String,
}

#[derive(Debug, Deserialize)]
pub struct UserInfo {
    pub id: String,
    pub name: String,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
}

// ── Pages ─────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize)]
pub struct PageMeta {
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub branch: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PageFull {
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub body: String,
    pub branch: String,
}

#[derive(Debug, Serialize)]
pub struct WritePageRequest {
    pub slug: String,
    pub body: String,
    pub branch: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WriteResponse {
    pub ok: bool,
    pub slug: String,
}

// ── Workspaces ────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize)]
pub struct WorkspaceInfo {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub role: String,
    pub visibility: String,
}

// ── Ingest ────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct IngestRequest {
    pub source_type: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    pub branch: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IngestResponse {
    pub filename: String,
    pub content_hash: String,
}

// ── Compile ───────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct CompileRequest {
    pub branch: String,
}

#[derive(Debug, Deserialize)]
pub struct CompileResponse {
    pub pages: Vec<CompiledPage>,
    pub skipped: usize,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CompiledPage {
    pub slug: String,
    pub title: String,
    pub summary: String,
}

// ── Search ────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize)]
pub struct SearchResult {
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub similarity: f64,
}

// ── Submit ────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct SubmitRequest {
    pub branch: String,
    pub page_slugs: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SubmitResponse {
    pub submission_id: String,
    pub summary: String,
    pub duplicates: Vec<DuplicateWarning>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DuplicateWarning {
    pub new_slug: String,
    pub existing_slug: String,
    pub similarity: f64,
}

// ── Review ────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize)]
pub struct Submission {
    pub id: String,
    pub user_id: String,
    pub status: String,
    pub summary: String,
    pub page_slugs: Vec<String>,
    pub source_branch: String,
    pub created_at: String,
    pub reviewed_by: Option<String>,
    pub reviewed_at: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ReviewDetail {
    pub submission: Submission,
    pub diffs: Vec<FileDiff>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct FileDiff {
    pub path: String,
    pub old_content: Option<String>,
    pub new_content: Option<String>,
}
