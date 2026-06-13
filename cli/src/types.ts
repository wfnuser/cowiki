// ── Auth ──────────────────────────────────────────────

export interface RegisterRequest {
  name: string;
  email?: string;
}

export interface AuthResponse {
  user: UserInfo;
  api_key: string;
}

export interface UserInfo {
  id: string;
  name: string;
  email?: string;
  avatar_url?: string;
}

// ── Pages ─────────────────────────────────────────────

export interface PageMeta {
  slug: string;
  title: string;
  summary: string;
  branch: string;
  kind?: string; // "page" | "folder"
  children?: PageMeta[];
}

export interface PageFull {
  slug: string;
  title: string;
  summary: string;
  body: string;
  branch: string;
}

export interface WritePageRequest {
  slug: string;
  body: string;
  branch: string;
  /** Content directory: "wiki" (default), "entities", "concepts", or subdir */
  dir?: string;
  /** Page title — server will prepend YAML frontmatter */
  title?: string;
  /** Page summary for YAML frontmatter */
  summary?: string;
}

export interface WriteResponse {
  ok: boolean;
  slug: string;
  /** Full path including directory, e.g. "entities/my-page" */
  path: string;
}

// ── Workspaces ────────────────────────────────────────

export interface WorkspaceInfo {
  id: string;
  name: string;
  slug: string;
  role: string;
  visibility: string;
}

// ── Ingest ────────────────────────────────────────────

export interface IngestRequest {
  source_type: string;
  content: string;
  filename?: string;
  branch: string;
}

export interface IngestResponse {
  filename: string;
  content_hash: string;
}

// ── Compile ───────────────────────────────────────────

export interface CompileRequest {
  branch: string;
}

export interface CompileResponse {
  pages: CompiledPage[];
  skipped: number;
}

export interface CompiledPage {
  slug: string;
  title: string;
  summary: string;
}

// ── Search ────────────────────────────────────────────

export interface KeywordHit {
  slug: string;
  title: string;
  snippet: string;
  title_match: boolean;
}

export interface SemanticHit {
  slug: string;
  title: string;
  summary: string;
  similarity: number;
  source: string; // "draft" or "main"
}

export interface SearchResponse {
  keyword: KeywordHit[];
  semantic: SemanticHit[];
}

// ── Submit ────────────────────────────────────────────

export interface SubmitRequest {
  branch: string;
  page_slugs: string[];
}

export interface SubmitResponse {
  submission_id: string;
  summary: string;
  duplicates: DuplicateWarning[];
}

export interface DuplicateWarning {
  new_slug: string;
  existing_slug: string;
  similarity: number;
}

// ── Review ────────────────────────────────────────────

export interface Submission {
  id: string;
  user_id: string;
  status: string;
  summary: string;
  page_slugs: string[];
  source_branch: string;
  created_at: string;
  reviewed_by?: string;
  reviewed_at?: string;
}

export interface ReviewDetail {
  submission: Submission;
  diffs: FileDiff[];
}

export interface FileDiff {
  path: string;
  old_content?: string;
  new_content?: string;
}

// ── API Keys ──────────────────────────────────────────

export interface KeyResponse {
  id: string;
  name: string;
  key_prefix: string;
  last_used_at?: string;
  created_at: string;
}

export interface CreateKeyResponse {
  id: string;
  name: string;
  key_prefix: string;
  raw_key: string;
  created_at: string;
}
