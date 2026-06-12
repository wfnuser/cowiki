import { ApiError, NetworkError } from './error.js';
import { urlencode } from './utils/urlencode.js';
import type {
  UserInfo,
  PageMeta,
  PageFull,
  WritePageRequest,
  WriteResponse,
  WorkspaceInfo,
  IngestRequest,
  IngestResponse,
  CompileRequest,
  CompileResponse,
  SearchResponse,
  SubmitRequest,
  SubmitResponse,
  Submission,
  ReviewDetail,
  KeyResponse,
  CreateKeyResponse,
} from './types.js';

export class CowikiClient {
  private baseUrl: string;
  private apiKey?: string;

  constructor(baseUrl: string, apiKey?: string) {
    // Normalize trailing slash
    this.baseUrl = baseUrl.replace(/\/+$/, '');
    this.apiKey = apiKey;

    // Warn if Bearer token would be sent over non-HTTPS remote connection
    const isLocal =
      this.baseUrl.startsWith('http://localhost') ||
      this.baseUrl.startsWith('http://127.') ||
      this.baseUrl.startsWith('http://[::1]');
    const isHttps = this.baseUrl.startsWith('https://');
    if (!isLocal && !isHttps && apiKey) {
      console.error(
        `\u{26a0}\u{fe0f}  WARNING: Server URL '${this.baseUrl}' is not HTTPS. ` +
          'Your API key will be sent in cleartext.',
      );
    }
  }

  private authHeaders(): Record<string, string> {
    const headers: Record<string, string> = {};
    if (this.apiKey) {
      headers['Authorization'] = `Bearer ${this.apiKey}`;
    }
    return headers;
  }

  private async get<T>(path: string): Promise<T> {
    const url = `${this.baseUrl}${path}`;
    const headers: Record<string, string> = {
      Accept: 'application/json',
      ...this.authHeaders(),
    };

    let resp: Response;
    try {
      resp = await fetch(url, { headers });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      if (msg.includes('connect') || (e as NodeJS.ErrnoException).code === 'ECONNREFUSED') {
        throw new NetworkError('Cannot connect to server. Is cowiki running?');
      }
      throw new NetworkError(`Network error: ${msg}`);
    }

    if (!resp.ok) throw await ApiError.fromResponse(resp);
    return resp.json() as Promise<T>;
  }

  private async post<T>(path: string, body: unknown): Promise<T> {
    const url = `${this.baseUrl}${path}`;
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
      Accept: 'application/json',
      ...this.authHeaders(),
    };

    let resp: Response;
    try {
      resp = await fetch(url, {
        method: 'POST',
        headers,
        body: JSON.stringify(body),
      });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      if (msg.includes('connect') || (e as NodeJS.ErrnoException).code === 'ECONNREFUSED') {
        throw new NetworkError('Cannot connect to server. Is cowiki running?');
      }
      throw new NetworkError(`Network error: ${msg}`);
    }

    if (!resp.ok) throw await ApiError.fromResponse(resp);
    return resp.json() as Promise<T>;
  }

  private async del(path: string): Promise<void> {
    const url = `${this.baseUrl}${path}`;
    const headers: Record<string, string> = {
      Accept: 'application/json',
      ...this.authHeaders(),
    };

    let resp: Response;
    try {
      resp = await fetch(url, { method: 'DELETE', headers });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      throw new NetworkError(`Network error: ${msg}`);
    }

    if (!resp.ok && resp.status !== 204) {
      throw await ApiError.fromResponse(resp);
    }
  }

  // ── Auth ──────────────────────────────────────────

  async getMe(): Promise<UserInfo> {
    return this.get('/api/auth/me');
  }

  async register(name: string, email?: string): Promise<{ user: UserInfo; api_key: string }> {
    return this.post('/api/auth/register', { name, email });
  }

  // ── Workspaces ────────────────────────────────────

  async listWorkspaces(): Promise<WorkspaceInfo[]> {
    return this.get('/api/workspaces');
  }

  // ── Pages (workspace-scoped) ──────────────────────

  async listPages(ws: string, branch: string, dir?: string): Promise<PageMeta[]> {
    let url = `/api/workspaces/${urlencode(ws)}/pages?branch=${urlencode(branch)}`;
    if (dir) {
      url += `&dir=${urlencode(dir)}`;
    }
    return this.get(url);
  }

  async getPage(ws: string, slug: string, branch: string, dir?: string): Promise<PageFull> {
    let url = `/api/workspaces/${urlencode(ws)}/pages/${urlencode(slug)}?branch=${urlencode(branch)}`;
    if (dir) {
      url += `&dir=${urlencode(dir)}`;
    }
    return this.get(url);
  }

  async writePage(ws: string, req: WritePageRequest): Promise<WriteResponse> {
    return this.post(`/api/workspaces/${urlencode(ws)}/pages`, req);
  }

  // ── Ingest (workspace-scoped) ─────────────────────

  async ingest(ws: string, req: IngestRequest): Promise<IngestResponse> {
    return this.post(`/api/workspaces/${urlencode(ws)}/ingest`, req);
  }

  // ── Compile (workspace-scoped) ────────────────────

  async compile(ws: string, req: CompileRequest): Promise<CompileResponse> {
    return this.post(`/api/workspaces/${urlencode(ws)}/compile`, req);
  }

  // ── Search (workspace-scoped) ─────────────────────

  async search(ws: string, query: string, limit: number): Promise<SearchResponse> {
    return this.get(
      `/api/workspaces/${urlencode(ws)}/search?q=${urlencode(query)}&limit=${limit}`,
    );
  }

  // ── Submit (workspace-scoped) ─────────────────────

  async submit(ws: string, req: SubmitRequest): Promise<SubmitResponse> {
    return this.post(`/api/workspaces/${urlencode(ws)}/submit`, req);
  }

  // ── Reviews (workspace-scoped) ────────────────────

  async listReviews(ws: string): Promise<Submission[]> {
    return this.get(`/api/workspaces/${urlencode(ws)}/reviews`);
  }

  async getReview(ws: string, id: string): Promise<ReviewDetail> {
    return this.get(`/api/workspaces/${urlencode(ws)}/reviews/${id}`);
  }

  async approveReview(ws: string, id: string): Promise<void> {
    await this.post(`/api/workspaces/${urlencode(ws)}/reviews/${id}`, { action: 'approve' });
  }

  async rejectReview(ws: string, id: string): Promise<void> {
    await this.post(`/api/workspaces/${urlencode(ws)}/reviews/${id}`, { action: 'reject' });
  }

  // ── API Keys ──────────────────────────────────────

  async listKeys(): Promise<KeyResponse[]> {
    return this.get('/api/keys');
  }

  async createKey(name: string): Promise<CreateKeyResponse> {
    return this.post('/api/keys', { name });
  }

  async revokeKey(id: string): Promise<void> {
    await this.del(`/api/keys/${id}`);
  }
}
