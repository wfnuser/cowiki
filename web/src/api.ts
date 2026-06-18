import { authHeaders } from './auth';

const BASE = `${import.meta.env.VITE_API_BASE || ''}/api`;

function h(extra: Record<string, string> = {}): Record<string, string> {
  return { ...authHeaders(), ...extra };
}

// ── Types ──

export interface PageMeta {
  slug: string;
  path: string;
  title: string;
  summary: string;
  branch: string;
  kind: 'page' | 'folder';
  children: PageMeta[];
}

export interface PageFull extends PageMeta {
  body: string;
  /** Last editor of the page (git history), for the byline. */
  edited_by?: string | null;
  /** Unix timestamp (seconds) of that last edit. */
  edited_at?: number | null;
}

export interface Submission {
  id: string;
  user_id: string;
  status: string;
  summary: string;
  paths: string[];
  source_branch: string;
  created_at: string;
  reviewed_by: string | null;
  reviewed_at: string | null;
  author_name: string | null;
  reviewer_name: string | null;
}

export interface DiffLine {
  kind: 'add' | 'del' | 'ctx';
  old_line: number | null;
  new_line: number | null;
  text: string;
}

export interface DiffHunk {
  header: string;
  lines: DiffLine[];
}

export interface FileDiff {
  path: string;
  old_content: string | null;
  new_content: string | null;
  hunks: DiffHunk[];
  additions: number;
  deletions: number;
}

export interface ReviewComment {
  id: string;
  submission_id: string;
  user_id: string;
  file_path: string;
  line_number: number | null;
  body: string;
  parent_id: string | null;
  resolved: boolean;
  created_at: string;
  updated_at: string;
}

export interface ReviewDetail {
  submission: Submission;
  diffs: FileDiff[];
  comments: ReviewComment[];
}

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
  /** "draft" (your branch) or "main" */
  source: string;
}

export interface SearchResponse {
  keyword: KeywordHit[];
  semantic: SemanticHit[];
}

export interface Workspace {
  id: string;
  name: string;
  slug: string;
  role: string;
  visibility: string;
}

export interface MemberInfo {
  id: string;
  name: string;
  email: string | null;
  role: string;
  last_active_at: string | null;
  joined_via: string;
}

export interface PendingInvitation {
  id: string;
  workspace_id: string;
  workspace_name: string;
  workspace_slug: string;
  role: string;
  invited_by_name: string;
  created_at: string;
}

export interface SourceItem {
  filename: string;
  compiled: boolean;
  compiled_pages: string[];
}

export interface SourceContent {
  filename: string;
  content: string;
  compiled: boolean;
  compiled_pages: string[];
}

// ── Workspaces ──

export async function listWorkspaces(): Promise<Workspace[]> {
  const res = await fetch(`${BASE}/workspaces`, { headers: h() });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || `Request failed: ${res.status}`);
  }
  return res.json();
}

export async function createWorkspace(name: string, slug: string, visibility = 'private'): Promise<Workspace> {
  const res = await fetch(`${BASE}/workspaces`, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({ name, slug, visibility }),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || `Request failed: ${res.status}`);
  }
  return res.json();
}

export async function listPublicWorkspaces(): Promise<Workspace[]> {
  const res = await fetch(`${BASE}/workspaces/public`, { headers: h() });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || `Request failed: ${res.status}`);
  }
  return res.json();
}

export async function joinWorkspace(slug: string): Promise<Workspace> {
  const res = await fetch(`${BASE}/workspaces/${slug}/join`, {
    method: 'POST',
    headers: h(),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || `Request failed: ${res.status}`);
  }
  return res.json();
}

export async function renameWorkspace(slug: string, name: string): Promise<Workspace> {
  const res = await fetch(`${BASE}/workspaces/${slug}/rename`, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({ name }),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || `Request failed: ${res.status}`);
  }
  return res.json();
}

export async function inviteToWorkspace(workspaceSlug: string, user: string, role = 'viewer', _expiresInDays = 7) {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/invite`, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({
      invitations: [{ user, role }],
    }),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || `Request failed: ${res.status}`);
  }
  const data = await res.json();
  // Check per-item status — the first failed result gives the reason
  const firstFailed = data.results?.find((r: { status: string; reason?: string }) => r.status === 'failed');
  if (firstFailed) {
    throw new Error(firstFailed.reason || 'Invitation was not sent');
  }
  return data;
}

export async function listPendingInvitations(): Promise<PendingInvitation[]> {
  const res = await fetch(`${BASE}/invitations/pending`, { headers: h() });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || `Request failed: ${res.status}`);
  }
  return res.json();
}

export async function acceptInvitation(invitationId: string): Promise<Workspace> {
  const res = await fetch(`${BASE}/invitations/${invitationId}/accept`, {
    method: 'POST',
    headers: h(),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || `Request failed: ${res.status}`);
  }
  return res.json();
}

export async function rejectInvitation(invitationId: string) {
  const res = await fetch(`${BASE}/invitations/${invitationId}/reject`, {
    method: 'POST',
    headers: h(),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || `Request failed: ${res.status}`);
  }
  return res.json();
}

export async function removeMember(workspaceSlug: string, userId: string) {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/members/remove`, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({ user_id: userId }),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || `Request failed: ${res.status}`);
  }
  return res.json();
}

export async function changeMemberRole(workspaceSlug: string, userId: string, role: string) {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/members/role`, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({ user_id: userId, role }),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || `Request failed: ${res.status}`);
  }
  return res.json();
}

export async function deleteWorkspace(workspaceSlug: string) {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}`, {
    method: 'DELETE',
    headers: h(),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || `Request failed: ${res.status}`);
  }
  return res.json();
}

export async function listMembers(workspaceSlug: string): Promise<MemberInfo[]> {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/members`, { headers: h() });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || `Request failed: ${res.status}`);
  }
  return res.json();
}

// ── Pages ──

export async function listPages(branch = 'main', workspaceSlug: string, dir = 'all'): Promise<PageMeta[]> {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/pages?branch=${encodeURIComponent(branch)}&dir=${encodeURIComponent(dir)}`, { headers: h() });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || `Request failed: ${res.status}`);
  }
  return res.json();
}

export async function getPage(slug: string, branch = 'main', workspaceSlug: string, dir = 'wiki'): Promise<PageFull> {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/pages/${encodeURIComponent(slug)}?branch=${encodeURIComponent(branch)}&dir=${encodeURIComponent(dir)}`, { headers: h() });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || `Request failed: ${res.status}`);
  }
  return res.json();
}

export async function writePage(slug: string, body: string, branch: string, workspaceSlug: string, dir = 'wiki'): Promise<void> {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/pages`, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({ slug, body, branch, dir }),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || `Request failed: ${res.status}`);
  }
}

// ── Ingest & Compile ──

export async function ingest(sourceType: string, content: string, branch: string, filename: string | undefined, workspaceSlug: string) {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/ingest`, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({ source_type: sourceType, content, branch, filename }),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || `Request failed: ${res.status}`);
  }
  return res.json();
}

export async function compile(branch: string, workspaceSlug: string) {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/compile`, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({ branch }),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || `Request failed: ${res.status}`);
  }
  return res.json();
}

// ── Submit & Review ──

export async function submit(branch: string, paths: string[], skipReview: boolean, workspaceSlug: string) {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/submit`, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({ branch, paths, skip_review: skipReview }),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || `Request failed: ${res.status}`);
  }
  return res.json();
}

export async function listReviews(workspaceSlug: string): Promise<Submission[]> {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/reviews`, { headers: h() });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || `Request failed: ${res.status}`);
  }
  return res.json();
}

export async function getReview(workspaceSlug: string, id: string): Promise<ReviewDetail> {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/reviews/${id}`, { headers: h() });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || `Request failed: ${res.status}`);
  }
  return res.json();
}

export async function reviewAction(workspaceSlug: string, id: string, action: string) {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/reviews/${id}`, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({ action }),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || `Request failed: ${res.status}`);
  }
  return res.json();
}

export async function createFolder(name: string, branch: string, parent: string | undefined, workspaceSlug: string) {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/folders`, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({ name, branch, parent }),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || `Request failed: ${res.status}`);
  }
  return res.json();
}

// ── Search ──

/** Workspace-scoped search: keyword (full-text over your view) and/or semantic. */
export async function searchWorkspace(
  workspaceSlug: string,
  q: string,
  mode: 'all' | 'keyword' | 'semantic' = 'all',
  limit = 12,
): Promise<SearchResponse> {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/search?q=${encodeURIComponent(q)}&limit=${limit}&mode=${mode}`, { headers: h() });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || `Request failed: ${res.status}`);
  }
  return res.json();
}

// ── API Keys ──

export interface ApiKeyInfo {
  id: string;
  name: string;
  key_prefix: string;   // masked: "cw_****xxxx"
  last_used_at: string | null;
  created_at: string;
}

export interface ApiKeyCreated {
  id: string;
  name: string;
  key_prefix: string;
  raw_key: string;       // ONLY returned on creation — store now!
  created_at: string;
}

export async function listApiKeys(): Promise<ApiKeyInfo[]> {
  const res = await fetch(`${BASE}/keys`, { headers: h() });
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

export async function createApiKey(name: string): Promise<ApiKeyCreated> {
  const res = await fetch(`${BASE}/keys`, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({ name }),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

export async function revokeApiKey(id: string): Promise<void> {
  const res = await fetch(`${BASE}/keys/${id}`, {
    method: 'DELETE',
    headers: h(),
  });
  if (!res.ok) throw new Error(await res.text());
}

// ── Sources ──

export async function listSources(workspaceSlug: string, branch = 'main'): Promise<SourceItem[]> {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/sources?branch=${encodeURIComponent(branch)}`, { headers: h() });
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

export async function getSource(workspaceSlug: string, filename: string, branch = 'main'): Promise<SourceContent> {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/sources/${encodeURIComponent(filename)}?branch=${encodeURIComponent(branch)}`, { headers: h() });
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

// ── Review Comments ──

export async function listComments(workspaceSlug: string, submissionId: string): Promise<ReviewComment[]> {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/reviews/${submissionId}/comments`, { headers: h() });
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

export async function createComment(
  workspaceSlug: string,
  submissionId: string,
  body: string,
  filePath: string,
  lineNumber?: number,
  parentId?: string,
): Promise<ReviewComment> {
  const payload: Record<string, unknown> = { file_path: filePath, body };
  if (lineNumber != null) payload.line_number = lineNumber;
  if (parentId != null) payload.parent_id = parentId;
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/reviews/${submissionId}/comments`, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify(payload),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

export async function resolveComment(workspaceSlug: string, submissionId: string, commentId: string): Promise<void> {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/reviews/${submissionId}/comments/${commentId}/resolve`, {
    method: 'POST',
    headers: h(),
  });
  if (!res.ok) throw new Error(await res.text());
}

export async function unresolveComment(workspaceSlug: string, submissionId: string, commentId: string): Promise<void> {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/reviews/${submissionId}/comments/${commentId}/unresolve`, {
    method: 'POST',
    headers: h(),
  });
  if (!res.ok) throw new Error(await res.text());
}

export async function deleteComment(workspaceSlug: string, submissionId: string, commentId: string): Promise<void> {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/reviews/${submissionId}/comments/${commentId}`, {
    method: 'DELETE',
    headers: h(),
  });
  if (!res.ok) throw new Error(await res.text());
}

// ── User Search ──

export interface UserSearchResult {
  id: string;
  name: string;
  email: string | null;
}

export async function searchUsers(q: string, limit = 10): Promise<UserSearchResult[]> {
  const res = await fetch(`${BASE}/users/search?q=${encodeURIComponent(q)}&limit=${limit}`, { headers: h() });
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

// ── Notifications ──

export interface Notification {
  id: string;
  kind: string;
  title: string;
  body: string | null;
  workspace_id: string | null;
  link: string | null;
  read: boolean;
  created_at: string;
}

export async function listNotifications(limit = 50): Promise<Notification[]> {
  const res = await fetch(`${BASE}/notifications?limit=${limit}`, { headers: h() });
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

export async function notificationUnreadCount(): Promise<number> {
  const res = await fetch(`${BASE}/notifications/unread-count`, { headers: h() });
  if (!res.ok) throw new Error(await res.text());
  const data = await res.json();
  return data.count;
}

export async function markNotificationRead(id: string): Promise<void> {
  const res = await fetch(`${BASE}/notifications/${id}/read`, { method: 'POST', headers: h() });
  if (!res.ok) throw new Error(await res.text());
}

export async function markAllNotificationsRead(): Promise<void> {
  const res = await fetch(`${BASE}/notifications/read-all`, { method: 'POST', headers: h() });
  if (!res.ok) throw new Error(await res.text());
}

// ── Ownership Transfers ──

export interface TransferResponse {
  id: string;
  status: string;
  from_user_id: string;
  to_user_id: string;
  previous_owner_new_role: string;
  created_at: string;
}

export async function initiateTransfer(
  workspaceSlug: string,
  newOwnerUserId: string,
  previousOwnerRole = 'manager',
): Promise<TransferResponse> {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/transfer-ownership`, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({ new_owner_user_id: newOwnerUserId, previous_owner_role: previousOwnerRole }),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || `Request failed: ${res.status}`);
  }
  return res.json();
}

export async function acceptTransfer(transferId: string): Promise<TransferResponse> {
  const res = await fetch(`${BASE}/transfers/${transferId}/accept`, { method: 'POST', headers: h() });
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

export async function rejectTransfer(transferId: string): Promise<TransferResponse> {
  const res = await fetch(`${BASE}/transfers/${transferId}/reject`, { method: 'POST', headers: h() });
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

export interface SyncResult {
  /** "up_to_date" | "updated" | "conflict" */
  status: string;
  conflicts: string[];
}

/** Rebase a draft branch onto main (the "Sync with main" button). */
export async function syncBranch(workspaceSlug: string, branch: string): Promise<SyncResult> {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/rebase`, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({ branch }),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || `Request failed: ${res.status}`);
  }
  return res.json();
}

/** Rename/move a file or folder on your draft branch. Paths are repo paths (wiki/...). */
export async function renamePath(workspaceSlug: string, branch: string, from: string, to: string): Promise<void> {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/paths/rename`, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({ branch, from, to }),
  });
  if (!res.ok) throw new Error(await res.text().catch(() => `Request failed: ${res.status}`));
}

/** Delete a file or an entire folder on your draft branch. Path is a repo path (wiki/...). */
export async function deletePath(workspaceSlug: string, branch: string, path: string): Promise<void> {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/paths/delete`, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({ branch, path }),
  });
  if (!res.ok) throw new Error(await res.text().catch(() => `Request failed: ${res.status}`));
}

/** Apply a review-requested change to a submission's pr/{id} snapshot (review screen edit). */
export async function editReview(workspaceSlug: string, submissionId: string, path: string, content: string): Promise<void> {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/reviews/${submissionId}/edit`, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({ path, content }),
  });
  if (!res.ok) throw new Error(await res.text().catch(() => `Request failed: ${res.status}`));
}

// ── Document Comments ──

export interface PageComment {
  id: string;
  workspace_slug: string;
  page_slug: string;
  user_id: string;
  content_hash: string | null;
  start_line: number | null;
  end_line: number | null;
  body: string;
  parent_id: string | null;
  resolved: boolean;
  created_at: string;
  updated_at: string;
}

export interface CommentSnapshot {
  content_hash: string;
  source: string;
}

export interface PageCommentsResponse {
  comments: PageComment[];
  snapshots: CommentSnapshot[];
}

/** All comments on a page plus the snapshots needed to re-anchor them. */
export async function listPageComments(workspaceSlug: string, slug: string): Promise<PageCommentsResponse> {
  const res = await fetch(
    `${BASE}/workspaces/${workspaceSlug}/page-comments?slug=${encodeURIComponent(slug)}`,
    { headers: h() },
  );
  if (!res.ok) throw new Error(await res.text().catch(() => `Request failed: ${res.status}`));
  return res.json();
}

/**
 * Create a comment or reply. Top-level comments pass `source` (the rendered,
 * frontmatter-stripped markdown the anchor was made against) plus a 1-based
 * `startLine`/`endLine`; replies pass only `parentId` + `body`.
 */
export async function createPageComment(
  workspaceSlug: string,
  input: {
    slug: string;
    body: string;
    source?: string;
    startLine?: number;
    endLine?: number;
    parentId?: string;
  },
): Promise<PageComment> {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/page-comments`, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({
      slug: input.slug,
      body: input.body,
      source: input.source,
      start_line: input.startLine,
      end_line: input.endLine,
      parent_id: input.parentId,
    }),
  });
  if (!res.ok) throw new Error(await res.text().catch(() => `Request failed: ${res.status}`));
  return res.json();
}

export async function setPageCommentResolved(
  workspaceSlug: string,
  commentId: string,
  resolved: boolean,
): Promise<PageComment> {
  const action = resolved ? 'resolve' : 'unresolve';
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/page-comments/${commentId}/${action}`, {
    method: 'POST',
    headers: h(),
  });
  if (!res.ok) throw new Error(await res.text().catch(() => `Request failed: ${res.status}`));
  return res.json();
}

export async function deletePageComment(workspaceSlug: string, commentId: string): Promise<void> {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/page-comments/${commentId}`, {
    method: 'DELETE',
    headers: h(),
  });
  if (!res.ok) throw new Error(await res.text().catch(() => `Request failed: ${res.status}`));
}
