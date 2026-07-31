import { normalizeServerOrigin } from './config.mjs';

export class CloudError extends Error {
  constructor(status, message) {
    super(message);
    this.name = 'CloudError';
    this.status = status;
  }
}

export class CloudClient {
  constructor(credential, fetchImpl = fetch) {
    this.credential = {
      ...credential,
      server: normalizeServerOrigin(credential.server),
    };
    this.fetchImpl = fetchImpl;
  }

  async request(pathname, init = {}) {
    const headers = new Headers(init.headers);
    headers.set('Accept', 'application/json');
    headers.set('Authorization', `Bearer ${this.credential.apiKey}`);
    if (init.body != null) headers.set('Content-Type', 'application/json');
    const response = await this.fetchImpl(
      `${this.credential.server}${pathname}`,
      { ...init, headers },
    );
    if (!response.ok) {
      const payload = await response.json().catch(() => null);
      throw new CloudError(
        response.status,
        payload?.error || `Cloud request failed (${response.status})`,
      );
    }
    if (response.status === 204) return undefined;
    return response.json();
  }

  getSpace(spaceId) {
    return this.request(`/api/spaces/${encodeURIComponent(spaceId)}`);
  }

  createOrUpdatePullRequest(spaceId, title, body = '') {
    return this.request(`/api/spaces/${encodeURIComponent(spaceId)}/pull-requests`, {
      method: 'POST',
      body: JSON.stringify({ title, body }),
    });
  }
}
