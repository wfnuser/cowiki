import { apiBase, isDesktopClient } from './runtime';

const API_KEY_STORAGE = 'cowiki_api_key';
const USER_STORAGE = 'cowiki_user';

export interface AuthUser {
  id: string;
  name: string;
  api_key: string | null;
  /**
   * `local` — account minted by the local backend's `/api/auth/local` (no
   * OAuth; full wiki features against the local server). `remote` — GitHub
   * sign-in on a hosted deploy; unlocks hosted-only surfaces (notifications,
   * publishing/sync).
   */
  mode: 'remote' | 'local';
}

export function getStoredAuth(): AuthUser | null {
  const key = localStorage.getItem(API_KEY_STORAGE);
  const user = localStorage.getItem(USER_STORAGE);
  if (!key || !user) return null;
  try {
    const parsed = JSON.parse(user);
    return { id: parsed.id, name: parsed.name, api_key: key, mode: parsed.mode === 'local' ? 'local' : 'remote' };
  } catch {
    return null;
  }
}

export function getCurrentAuth(): AuthUser | null {
  const stored = getStoredAuth();
  if (stored) return stored;
  // Desktop with no session yet (e.g. local backend not running): a
  // credential-less placeholder so the shell still opens instead of
  // bouncing to the login page.
  if (!isDesktopClient()) return null;
  return { id: 'local', name: 'Local User', api_key: null, mode: 'local' };
}

/** True when signed in at all (local or remote) — gates workspace features. */
export function hasAuth(auth: AuthUser | null): boolean {
  return !!auth?.api_key;
}

/** True only for hosted sign-ins — gates notifications, publishing, sync. */
export function isRemoteAuth(auth: AuthUser | null): boolean {
  return auth?.mode === 'remote' && !!auth.api_key;
}

export function storeAuth(apiKey: string, userName: string, userId: string, mode: 'remote' | 'local' = 'remote') {
  localStorage.setItem(API_KEY_STORAGE, apiKey);
  localStorage.setItem(USER_STORAGE, JSON.stringify({ id: userId, name: userName, mode }));
}

export function clearAuth() {
  localStorage.removeItem(API_KEY_STORAGE);
  localStorage.removeItem(USER_STORAGE);
}

export function getApiKey(): string | null {
  return getStoredAuth()?.api_key ?? null;
}

/** Add auth header to fetch requests */
export function authHeaders(): Record<string, string> {
  const key = getApiKey();
  if (!key) return {};
  return { Authorization: `Bearer ${key}` };
}

/**
 * Local-mode sign-in: single-user backends expose /api/auth/local, which
 * returns credentials for the machine's local user without OAuth. Returns
 * true when local mode is active and auth was stored; false on hosted
 * deploys (endpoint disabled) or when the backend is unreachable.
 */
export async function tryLocalLogin(): Promise<boolean> {
  try {
    const res = await fetch(`${apiBase()}/auth/local`, { method: 'POST' });
    if (!res.ok) return false;
    const data = await res.json();
    if (!data?.api_key || !data?.user?.id) return false;
    storeAuth(data.api_key, data.user.name, data.user.id, 'local');
    return true;
  } catch {
    return false;
  }
}
