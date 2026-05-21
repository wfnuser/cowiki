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
