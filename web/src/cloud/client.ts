import {
  normalizeCloudOrigin,
  normalizeCloudSession,
  type CloudRole,
  type CloudSession,
  type CloudVisibility,
} from './session.ts';
import type { DocumentProvenance } from '../lib/page-lineage';

export type CloudFetch = (input: RequestInfo | URL, init?: RequestInit) => Promise<Response>;

export interface CloudUser {
  id: string;
  githubId: number;
  handle: string;
  displayName: string;
  avatarUrl: string | null;
}

export interface CloudSpace {
  id: string;
  name: string;
  slug: string;
  visibility: CloudVisibility;
  role: CloudRole;
  gitUrl: string;
  mainRef: 'main';
  userRef: string;
  publicUrl: string;
}

export type CloudSpaceCreationReason = 'invite_required' | 'limit_reached' | null;

export interface CloudSpaceCreationCapability {
  authorized: boolean;
  createdCount: number;
  limit: number;
  canCreate: boolean;
  reason: CloudSpaceCreationReason;
}

export interface PublicCloudSpace {
  id: string;
  name: string;
  slug: string;
  visibility: 'public';
  mainRef: 'main';
  publicUrl: string;
}

export interface CloudTreeEntry {
  path: string;
  kind: 'folder' | 'page';
}

export interface CloudTree {
  ref: 'main';
  oid: string;
  entries: CloudTreeEntry[];
}

export interface CloudContent {
  ref: 'main';
  oid: string;
  path: string;
  content: string;
  provenance?: DocumentProvenance | null;
}

export interface CloudMember {
  userId: string;
  handle: string;
  displayName: string;
  avatarUrl: string | null;
  role: CloudRole;
}

export type CloudInvitableRole = 'editor' | 'viewer';

export interface CloudInvitationPreview {
  spaceId: string;
  spaceName: string;
  spaceSlug: string;
  role: CloudInvitableRole;
  expiresAt: string;
}

export interface CloudInvitation {
  id: string;
  spaceId: string;
  role: CloudInvitableRole;
  expiresAt: string;
  acceptedCount: number;
  createdAt: string;
  token?: string;
  inviteUrl?: string;
}

export type CloudPullRequestStatus = 'open' | 'merged' | 'closed';

export interface CloudPullRequest {
  id: string;
  spaceId: string;
  number: number;
  authorId: string;
  title: string;
  body: string;
  baseRef: string;
  headRef: string;
  baseOid: string;
  headOid: string;
  status: CloudPullRequestStatus;
  mergedBy: string | null;
  approvalCount: number;
  authorHandle: string;
  authorName: string;
  authorAvatarUrl: string | null;
}

export interface CloudPullRequestDiff {
  baseOid: string;
  headOid: string;
  files: Array<{
    path: string;
    status: string;
    additions: number;
    deletions: number;
    oldContent?: string | null;
    newContent?: string | null;
  }>;
  patch: string;
}

export class CloudApiError extends Error {
  readonly status: number;
  readonly code: string | null;

  constructor(status: number, message: string, code: string | null = null) {
    super(message);
    this.name = 'CloudApiError';
    this.status = status;
    this.code = code;
  }
}

export interface CloudClient {
  readonly session: CloudSession;
  currentUser(): Promise<CloudUser>;
  logout(): Promise<void>;
  listSpaces(): Promise<CloudSpace[]>;
  getSpaceCreationCapability(): Promise<CloudSpaceCreationCapability>;
  redeemSpaceCreationInvite(code: string): Promise<CloudSpaceCreationCapability>;
  createSpace(name: string, slug: string, visibility?: CloudVisibility): Promise<CloudSpace>;
  updateSpaceVisibility(spaceId: string, visibility: CloudVisibility): Promise<CloudSpace>;
  getSpace(spaceId: string): Promise<CloudSpace>;
  getTree(spaceId: string): Promise<CloudTree>;
  getContent(spaceId: string, path: string): Promise<CloudContent>;
  getSourceContent(spaceId: string, path: string): Promise<CloudContent>;
  listMembers(spaceId: string): Promise<CloudMember[]>;
  setMember(spaceId: string, handle: string, role: CloudRole): Promise<CloudMember>;
  removeMember(spaceId: string, memberId: string): Promise<void>;
  acceptInvitation(token: string): Promise<CloudSpace>;
  listInvitations(spaceId: string): Promise<CloudInvitation[]>;
  createInvitation(
    spaceId: string,
    role: CloudInvitableRole,
    expiresInHours: number,
  ): Promise<CloudInvitation>;
  revokeInvitation(spaceId: string, invitationId: string): Promise<void>;
  listPullRequests(spaceId: string): Promise<CloudPullRequest[]>;
  getPullRequest(spaceId: string, pullRequestId: string): Promise<CloudPullRequest>;
  getPullRequestDiff(spaceId: string, pullRequestId: string): Promise<CloudPullRequestDiff>;
  approvePullRequest(spaceId: string, pullRequestId: string): Promise<CloudPullRequest>;
  mergePullRequest(spaceId: string, pullRequestId: string, expectedHeadOid: string): Promise<CloudPullRequest>;
}

export function createCloudClient(
  inputSession: CloudSession,
  fetchImpl: CloudFetch = fetch,
): CloudClient {
  const session = normalizeCloudSession(inputSession);

  async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
    const headers = new Headers(init.headers);
    headers.set('Accept', 'application/json');
    headers.set('Authorization', `Bearer ${session.apiKey}`);
    if (init.body != null) headers.set('Content-Type', 'application/json');
    const response = await fetchImpl(`${session.baseUrl}${path}`, { ...init, headers });
    if (!response.ok) {
      const payload = await response.json().catch(() => null) as {
        error?: string;
        code?: string;
      } | null;
      throw new CloudApiError(
        response.status,
        payload?.error || `Cloud request failed (${response.status})`,
        payload?.code ?? null,
      );
    }
    if (response.status === 204) return undefined as T;
    return response.json() as Promise<T>;
  }

  const spacePath = (spaceId: string, suffix = '') =>
    `/api/spaces/${encodeURIComponent(spaceId)}${suffix}`;
  const pullRequestPath = (spaceId: string, pullRequestId: string, suffix = '') =>
    `${spacePath(spaceId, '/pull-requests')}/${encodeURIComponent(pullRequestId)}${suffix}`;

  return {
    session,
    async currentUser() {
      const response = await request<{ user: CloudUser }>('/api/me');
      return response.user;
    },
    logout: () => request('/api/auth/logout', { method: 'POST' }),
    listSpaces: () => request('/api/spaces'),
    getSpaceCreationCapability: () => request('/api/space-creation-capability'),
    redeemSpaceCreationInvite: (code) => request('/api/space-creation-capability/redeem', {
      method: 'POST',
      body: JSON.stringify({ code: code.trim() }),
    }),
    createSpace: (name, slug, visibility = 'private') => request('/api/spaces', {
      method: 'POST',
      body: JSON.stringify({ name, slug, visibility }),
    }),
    updateSpaceVisibility: (spaceId, visibility) => request(spacePath(spaceId), {
      method: 'PATCH',
      body: JSON.stringify({ visibility }),
    }),
    getSpace: (spaceId) => request(spacePath(spaceId)),
    getTree: (spaceId) => request(`${spacePath(spaceId, '/tree')}?ref=main`),
    getContent: (spaceId, path) => {
      const query = new URLSearchParams({ ref: 'main', path });
      return request(`${spacePath(spaceId, '/content')}?${query}`);
    },
    getSourceContent: (spaceId, path) => {
      const query = new URLSearchParams({ ref: 'main', path });
      return request(`${spacePath(spaceId, '/sources/content')}?${query}`);
    },
    listMembers: (spaceId) => request(spacePath(spaceId, '/members')),
    setMember: (spaceId, handle, role) => request(spacePath(spaceId, '/members'), {
      method: 'POST',
      body: JSON.stringify({ handle, role }),
    }),
    removeMember: (spaceId, memberId) => request(
      `${spacePath(spaceId, '/members')}/${encodeURIComponent(memberId)}`,
      { method: 'DELETE' },
    ),
    acceptInvitation: (token) => request(
      `/api/invitations/${encodeURIComponent(token)}/accept`,
      { method: 'POST' },
    ),
    listInvitations: (spaceId) => request(spacePath(spaceId, '/invitations')),
    createInvitation: (spaceId, role, expiresInHours) => request(
      spacePath(spaceId, '/invitations'),
      {
        method: 'POST',
        body: JSON.stringify({ role, expiresInHours }),
      },
    ),
    revokeInvitation: (spaceId, invitationId) => request(
      `${spacePath(spaceId, '/invitations')}/${encodeURIComponent(invitationId)}`,
      { method: 'DELETE' },
    ),
    listPullRequests: (spaceId) => request(spacePath(spaceId, '/pull-requests')),
    getPullRequest: (spaceId, pullRequestId) => request(pullRequestPath(spaceId, pullRequestId)),
    getPullRequestDiff: (spaceId, pullRequestId) => request(
      pullRequestPath(spaceId, pullRequestId, '/diff'),
    ),
    approvePullRequest: (spaceId, pullRequestId) => request(
      pullRequestPath(spaceId, pullRequestId, '/approve'),
      { method: 'POST' },
    ),
    mergePullRequest: (spaceId, pullRequestId, expectedHeadOid) => request(
      pullRequestPath(spaceId, pullRequestId, '/merge'),
      { method: 'POST', body: JSON.stringify({ expectedHeadOid }) },
    ),
  };
}

export interface PublicCloudClient {
  listSpaces(): Promise<PublicCloudSpace[]>;
  getSpace(slug: string): Promise<PublicCloudSpace>;
  getTree(slug: string): Promise<CloudTree>;
  getContent(slug: string, path: string): Promise<CloudContent>;
}

export function createPublicCloudClient(
  baseUrl: string,
  fetchImpl: CloudFetch = fetch,
): PublicCloudClient {
  const origin = normalizeCloudOrigin(baseUrl);

  async function request<T>(path: string): Promise<T> {
    const response = await fetchImpl(`${origin}${path}`, {
      headers: { Accept: 'application/json' },
    });
    if (!response.ok) {
      const payload = await response.json().catch(() => null) as { error?: string } | null;
      throw new CloudApiError(
        response.status,
        payload?.error || `Cloud request failed (${response.status})`,
      );
    }
    return response.json() as Promise<T>;
  }

  const spacePath = (slug: string, suffix = '') =>
    `/api/public/spaces/${encodeURIComponent(slug)}${suffix}`;

  return {
    listSpaces: () => request('/api/public/spaces'),
    getSpace: (slug) => request(spacePath(slug)),
    getTree: (slug) => request(`${spacePath(slug, '/tree')}?ref=main`),
    getContent: (slug, path) => {
      const query = new URLSearchParams({ ref: 'main', path });
      return request(`${spacePath(slug, '/content')}?${query}`);
    },
  };
}

export async function previewCloudInvitation(
  baseUrl: string,
  token: string,
  fetchImpl: CloudFetch = fetch,
): Promise<CloudInvitationPreview> {
  const origin = baseUrl.replace(/\/$/, '');
  const response = await fetchImpl(
    `${origin}/api/invitations/${encodeURIComponent(token)}`,
    { headers: { Accept: 'application/json' } },
  );
  if (!response.ok) {
    const payload = await response.json().catch(() => null) as { error?: string } | null;
    throw new CloudApiError(
      response.status,
      payload?.error || `Invitation request failed (${response.status})`,
    );
  }
  return response.json() as Promise<CloudInvitationPreview>;
}
