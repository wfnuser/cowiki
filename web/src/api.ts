import { authHeaders } from './auth';

const BASE = '/api';

function h(extra: Record<string, string> = {}): Record<string, string> {
  return { ...authHeaders(), ...extra };
}

// ── Types ──

export interface PageMeta {
  slug: string;
  title: string;
  summary: string;
  branch: string;
  kind: 'page' | 'folder';
  children: PageMeta[];
}

export interface PageFull extends PageMeta {
  body: string;
}

export interface Submission {
  id: string;
  user_id: string;
  status: string;
  summary: string;
  page_slugs: string[];
  source_branch: string;
  created_at: string;
  reviewed_by: string | null;
  reviewed_at: string | null;
}

export interface FileDiff {
  path: string;
  old_content: string | null;
  new_content: string | null;
}

export interface ReviewDetail {
  submission: Submission;
  diffs: FileDiff[];
}

export interface SearchResult {
  slug: string;
  title: string;
  summary: string;
  similarity: number;
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
  return res.json();
}

export async function createWorkspace(name: string, slug: string, visibility = 'private'): Promise<Workspace> {
  const res = await fetch(`${BASE}/workspaces`, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({ name, slug, visibility }),
  });
  return res.json();
}

export async function listPublicWorkspaces(): Promise<Workspace[]> {
  const res = await fetch(`${BASE}/workspaces/public`, { headers: h() });
  return res.json();
}

export async function joinWorkspace(slug: string): Promise<Workspace> {
  const res = await fetch(`${BASE}/workspaces/${slug}/join`, {
    method: 'POST',
    headers: h(),
  });
  return res.json();
}

export async function renameWorkspace(slug: string, name: string): Promise<Workspace> {
  const res = await fetch(`${BASE}/workspaces/${slug}/rename`, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({ name }),
  });
  return res.json();
}

export async function inviteToWorkspace(workspaceSlug: string, email: string, role = 'writer') {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/invite`, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({ email, role }),
  });
  return res.json();
}

export async function listPendingInvitations(): Promise<PendingInvitation[]> {
  const res = await fetch(`${BASE}/invitations/pending`, { headers: h() });
  return res.json();
}

export async function acceptInvitation(invitationId: string): Promise<Workspace> {
  const res = await fetch(`${BASE}/invitations/${invitationId}/accept`, {
    method: 'POST',
    headers: h(),
  });
  return res.json();
}

export async function rejectInvitation(invitationId: string) {
  const res = await fetch(`${BASE}/invitations/${invitationId}/reject`, {
    method: 'POST',
    headers: h(),
  });
  return res.json();
}

export async function removeMember(workspaceSlug: string, userId: string) {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/members/remove`, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({ user_id: userId }),
  });
  return res.json();
}

export async function changeMemberRole(workspaceSlug: string, userId: string, role: string) {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/members/role`, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({ user_id: userId, role }),
  });
  return res.json();
}

export async function deleteWorkspace(workspaceSlug: string) {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}`, {
    method: 'DELETE',
    headers: h(),
  });
  return res.json();
}

export async function listMembers(workspaceSlug: string): Promise<MemberInfo[]> {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/members`, { headers: h() });
  return res.json();
}

// ── Pages ──

export async function listPages(branch = 'main', workspaceSlug?: string): Promise<PageMeta[]> {
  const url = workspaceSlug
    ? `${BASE}/workspaces/${workspaceSlug}/pages?branch=${branch}`
    : `${BASE}/pages?branch=${branch}`;
  const res = await fetch(url, { headers: h() });
  return res.json();
}

export async function getPage(slug: string, branch = 'main', workspaceSlug?: string): Promise<PageFull> {
  const url = workspaceSlug
    ? `${BASE}/workspaces/${workspaceSlug}/pages/${slug}?branch=${branch}`
    : `${BASE}/pages/${slug}?branch=${branch}`;
  const res = await fetch(url, { headers: h() });
  return res.json();
}

export async function writePage(slug: string, body: string, branch: string, workspaceSlug?: string): Promise<void> {
  const url = workspaceSlug
    ? `${BASE}/workspaces/${workspaceSlug}/pages`
    : `${BASE}/pages`;
  await fetch(url, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({ slug, body, branch }),
  });
}

// ── Ingest & Compile ──

export async function ingest(sourceType: string, content: string, branch: string, filename?: string, workspaceSlug?: string) {
  const url = workspaceSlug
    ? `${BASE}/workspaces/${workspaceSlug}/ingest`
    : `${BASE}/ingest`;
  const res = await fetch(url, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({ source_type: sourceType, content, branch, filename }),
  });
  return res.json();
}

export async function compile(branch: string, workspaceSlug?: string) {
  const url = workspaceSlug
    ? `${BASE}/workspaces/${workspaceSlug}/compile`
    : `${BASE}/compile`;
  const res = await fetch(url, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({ branch }),
  });
  return res.json();
}

// ── Submit & Review ──

export async function submit(branch: string, pageSlugs: string[], skipReview = false) {
  const res = await fetch(`${BASE}/submit`, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({ branch, page_slugs: pageSlugs, skip_review: skipReview }),
  });
  return res.json();
}

export async function listReviews(): Promise<Submission[]> {
  const res = await fetch(`${BASE}/reviews`, { headers: h() });
  return res.json();
}

export async function getReview(id: string): Promise<ReviewDetail> {
  const res = await fetch(`${BASE}/reviews/${id}`, { headers: h() });
  return res.json();
}

export async function reviewAction(id: string, action: string) {
  const res = await fetch(`${BASE}/reviews/${id}`, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({ action }),
  });
  return res.json();
}

export async function createFolder(name: string, branch: string, parent?: string, workspaceSlug?: string) {
  const url = workspaceSlug
    ? `${BASE}/workspaces/${workspaceSlug}/folders`
    : `${BASE}/folders`;
  const res = await fetch(url, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({ name, branch, parent }),
  });
  return res.json();
}

// ── Search ──

export async function search(q: string, branch = 'main'): Promise<SearchResult[]> {
  const res = await fetch(`${BASE}/search?q=${encodeURIComponent(q)}&branch=${branch}`, { headers: h() });
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
