import { isDesktopClient } from './runtime';

const API_KEY_STORAGE = 'cowiki_api_key';
const USER_STORAGE = 'cowiki_user';

export interface AuthUser {
  id: string;
  name: string;
  api_key: string | null;
  mode: 'remote' | 'local';
}

export function getStoredAuth(): AuthUser | null {
  const key = localStorage.getItem(API_KEY_STORAGE);
  const user = localStorage.getItem(USER_STORAGE);
  if (!key || !user) return null;
  try {
    return { ...JSON.parse(user), api_key: key, mode: 'remote' };
  } catch {
    return null;
  }
}

export function getCurrentAuth(): AuthUser | null {
  const remote = getStoredAuth();
  if (remote) return remote;
  if (!isDesktopClient()) return null;
  return { id: 'local', name: 'Local User', api_key: null, mode: 'local' };
}

export function isRemoteAuth(auth: AuthUser | null): boolean {
  return auth?.mode === 'remote' && !!auth.api_key;
}

export function storeAuth(apiKey: string, userName: string, userId: string) {
  localStorage.setItem(API_KEY_STORAGE, apiKey);
  localStorage.setItem(USER_STORAGE, JSON.stringify({ id: userId, name: userName }));
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
