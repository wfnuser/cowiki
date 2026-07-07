const API_KEY_STORAGE = 'cowiki_api_key';
const USER_STORAGE = 'cowiki_user';

export interface AuthUser {
  id: string;
  name: string;
  api_key: string;
}

export function getStoredAuth(): AuthUser | null {
  const key = localStorage.getItem(API_KEY_STORAGE);
  const user = localStorage.getItem(USER_STORAGE);
  if (!key || !user) return null;
  try {
    return { ...JSON.parse(user), api_key: key };
  } catch {
    return null;
  }
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
  return localStorage.getItem(API_KEY_STORAGE);
}

/** Add auth header to fetch requests */
export function authHeaders(): Record<string, string> {
  const key = getApiKey();
  if (!key) return {};
  return { Authorization: `Bearer ${key}` };
}

/**
 * Local-mode sign-in: single-user installs expose /api/auth/local, which
 * returns credentials for the machine's local user without OAuth. Returns
 * true when local mode is active and auth was stored; false on hosted
 * deploys (endpoint disabled) or network failure.
 */
export async function tryLocalLogin(): Promise<boolean> {
  try {
    const res = await fetch(`${import.meta.env.VITE_API_BASE || ''}/api/auth/local`, { method: 'POST' });
    if (!res.ok) return false;
    const data = await res.json();
    if (!data?.api_key || !data?.user?.id) return false;
    storeAuth(data.api_key, data.user.name, data.user.id);
    return true;
  } catch {
    return false;
  }
}
