import { authHeaders } from './auth';

const BASE = '/api';

function h(extra: Record<string, string> = {}): Record<string, string> {
  return { ...authHeaders(), ...extra };
}

export interface PageMeta {
  id: string;
  slug: string;
  title: string;
  summary: string;
  branch: string;
  updated_at: string;
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

export async function listPages(branch = 'main'): Promise<PageMeta[]> {
  const res = await fetch(`${BASE}/pages?branch=${branch}`, { headers: h() });
  return res.json();
}

export async function getPage(slug: string, branch = 'main'): Promise<PageFull> {
  const res = await fetch(`${BASE}/pages/${slug}?branch=${branch}`, { headers: h() });
  return res.json();
}

export async function writePage(slug: string, body: string, branch: string): Promise<void> {
  await fetch(`${BASE}/pages`, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({ slug, body, branch }),
  });
}

export async function ingest(sourceType: string, content: string, branch: string, filename?: string) {
  const res = await fetch(`${BASE}/ingest`, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({ source_type: sourceType, content, branch, filename }),
  });
  return res.json();
}

export async function compile(branch: string) {
  const res = await fetch(`${BASE}/compile`, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({ branch }),
  });
  return res.json();
}

export async function submit(branch: string, pageSlugs: string[]) {
  const res = await fetch(`${BASE}/submit`, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({ branch, page_slugs: pageSlugs }),
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

export async function search(q: string, branch = 'main'): Promise<SearchResult[]> {
  const res = await fetch(`${BASE}/search?q=${encodeURIComponent(q)}&branch=${branch}`, { headers: h() });
  return res.json();
}
