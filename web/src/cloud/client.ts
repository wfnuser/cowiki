import {
  normalizeCloudSession,
  type CloudRole,
  type CloudSession,
} from './session.ts';

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
  role: CloudRole;
  gitUrl: string;
  mainRef: 'main';
  userRef: string;
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
}

export interface CloudMember {
  userId: string;
  handle: string;
  displayName: string;
  avatarUrl: string | null;
  role: CloudRole;
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
}

export class CloudApiError extends Error {
  readonly status: number;

  constructor(status: number, message: string) {
    super(message);
    this.name = 'CloudApiError';
    this.status = status;
  }
}

export interface CloudClient {
  readonly session: CloudSession;
  currentUser(): Promise<CloudUser>;
  listSpaces(): Promise<CloudSpace[]>;
  createSpace(name: string, slug: string): Promise<CloudSpace>;
  getSpace(spaceId: string): Promise<CloudSpace>;
  getTree(spaceId: string): Promise<CloudTree>;
  getContent(spaceId: string, path: string): Promise<CloudContent>;
  listMembers(spaceId: string): Promise<CloudMember[]>;
  setMember(spaceId: string, handle: string, role: CloudRole): Promise<CloudMember>;
  removeMember(spaceId: string, memberId: string): Promise<void>;
  listPullRequests(spaceId: string): Promise<CloudPullRequest[]>;
  getPullRequest(spaceId: string, pullRequestId: string): Promise<CloudPullRequest>;
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
      const payload = await response.json().catch(() => null) as { error?: string } | null;
      throw new CloudApiError(response.status, payload?.error || `Cloud request failed (${response.status})`);
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
    listSpaces: () => request('/api/spaces'),
    createSpace: (name, slug) => request('/api/spaces', {
      method: 'POST',
      body: JSON.stringify({ name, slug }),
    }),
    getSpace: (spaceId) => request(spacePath(spaceId)),
    getTree: (spaceId) => request(`${spacePath(spaceId, '/tree')}?ref=main`),
    getContent: (spaceId, path) => {
      const query = new URLSearchParams({ ref: 'main', path });
      return request(`${spacePath(spaceId, '/content')}?${query}`);
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
    listPullRequests: (spaceId) => request(spacePath(spaceId, '/pull-requests')),
    getPullRequest: (spaceId, pullRequestId) => request(pullRequestPath(spaceId, pullRequestId)),
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
