export interface OAuthCredential {
  apiKey: string;
  userName: string;
  userId: string;
}

export function buildWebGithubLoginUrl(apiBase: string): string {
  return `${apiBase.replace(/\/$/, '')}/auth/github`;
}

export function buildDesktopGithubLoginUrl(apiBase: string, callbackUrl: string): string {
  const url = new URL(buildWebGithubLoginUrl(apiBase));
  url.searchParams.set('client', 'desktop');
  url.searchParams.set('callback', callbackUrl);
  return url.toString();
}

export function parseDesktopOAuthCallback(callbackUrl: string): OAuthCredential | null {
  const url = new URL(callbackUrl);
  const apiKey = url.searchParams.get('api_key');
  const userName = url.searchParams.get('user_name');
  const userId = url.searchParams.get('user_id');
  if (!apiKey || !userName || !userId) return null;
  return { apiKey, userName, userId };
}
