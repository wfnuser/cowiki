export interface OAuthCredential {
  apiKey: string;
  userName: string;
  userId: string;
}

export interface WebAuthBootstrapDependencies {
  readOAuthCode(): string | null;
  exchangeOAuthCode(code: string): Promise<OAuthCredential>;
  storeCredential(credential: OAuthCredential): void;
  hasStoredCredential(): boolean;
  validateStoredCredential(): Promise<Response>;
  clearCredential(): void;
  finishOAuth(): void;
  failOAuth(): void;
}

export const AUTH_RETURN_PATH_STORAGE = 'cowiki.authReturnPath';

export function buildWebGithubLoginUrl(apiBase: string): string {
  return `${apiBase.replace(/\/$/, '')}/auth/github`;
}

export function buildDesktopGithubLoginUrl(apiBase: string, callbackUrl: string): string {
  return buildLoopbackGithubLoginUrl(apiBase, 'desktop', callbackUrl);
}

export function buildLoopbackGithubLoginUrl(
  apiBase: string,
  client: 'desktop' | 'cli',
  callbackUrl: string,
): string {
  const url = new URL(buildWebGithubLoginUrl(apiBase));
  url.searchParams.set('client', client);
  url.searchParams.set('callback', callbackUrl);
  return url.toString();
}

export function parseWebOAuthFragment(hash: string): string | null {
  const params = new URLSearchParams(hash.replace(/^#/, ''));
  const code = params.get('auth_code')?.trim();
  return code || null;
}

export function safeAuthReturnPath(value: string | null): string {
  if (!value || !value.startsWith('/') || value.startsWith('//')) return '/cloud';
  try {
    const parsed = new URL(value, 'https://cowiki.local');
    if (parsed.origin !== 'https://cowiki.local') return '/cloud';
    return `${parsed.pathname}${parsed.search}${parsed.hash}`;
  } catch {
    return '/cloud';
  }
}

export async function exchangeOAuthCode(
  apiBase: string,
  code: string,
  fetchImpl: typeof fetch = fetch,
): Promise<OAuthCredential> {
  const response = await fetchImpl(`${apiBase.replace(/\/$/, '')}/auth/exchange`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', Accept: 'application/json' },
    body: JSON.stringify({ code, client: 'web' }),
  });
  if (!response.ok) {
    const payload = await response.json().catch(() => null) as { error?: string } | null;
    throw new Error(payload?.error || `Cloud sign-in failed (${response.status})`);
  }
  const payload = await response.json() as {
    apiKey?: string;
    userName?: string;
    userId?: string;
  };
  if (!payload.apiKey || !payload.userName || !payload.userId) {
    throw new Error('Cloud returned an invalid sign-in response');
  }
  return {
    apiKey: payload.apiKey,
    userName: payload.userName,
    userId: payload.userId,
  };
}

export function validateWebCredential(
  apiBase: string,
  headers: HeadersInit,
  fetchImpl: typeof fetch = fetch,
): Promise<Response> {
  return fetchImpl(`${apiBase.replace(/\/$/, '')}/me`, { headers });
}

export function createWebAuthBootstrap(dependencies: WebAuthBootstrapDependencies) {
  let inFlight: Promise<void> | null = null;

  const bootstrap = async () => {
    const code = dependencies.readOAuthCode();
    if (code) {
      try {
        const credential = await dependencies.exchangeOAuthCode(code);
        dependencies.storeCredential(credential);
        dependencies.finishOAuth();
      } catch {
        dependencies.clearCredential();
        dependencies.failOAuth();
        return;
      }
    }

    if (!dependencies.hasStoredCredential()) return;
    try {
      const response = await dependencies.validateStoredCredential();
      if (response.status === 401) dependencies.clearCredential();
    } catch {
      // Keep the stored session while offline.
    }
  };

  return () => {
    if (!inFlight) inFlight = bootstrap();
    return inFlight;
  };
}

export function parseDesktopOAuthCallback(callbackUrl: string): OAuthCredential | null {
  const url = new URL(callbackUrl);
  const apiKey = url.searchParams.get('api_key');
  const userName = url.searchParams.get('user_name');
  const userId = url.searchParams.get('user_id');
  if (!apiKey || !userName || !userId) return null;
  return { apiKey, userName, userId };
}

/** Turn a desktop OAuth failure into one clear sentence for the sign-in screen. */
export function describeDesktopOAuthError(caught: unknown): string {
  const raw = (caught instanceof Error ? caught.message : typeof caught === 'string' ? caught : '').trim();
  if (!raw) return 'Could not sign in with GitHub. Try again.';
  if (/timed out/i.test(raw)) return 'Sign-in timed out before GitHub responded. Try again.';
  if (/cancel/i.test(raw)) return 'Sign-in was cancelled. Try again when ready.';
  if (/open.*browser|browser.*open/i.test(raw)) return 'Could not open your browser for GitHub sign-in. Try again.';
  const cleaned = raw.replace(/^Error:\s*/i, '');
  const sentence = /[.!?]$/.test(cleaned) ? cleaned : `${cleaned}.`;
  return `GitHub sign-in failed: ${sentence}`;
}
