import { buildWebGithubLoginUrl } from './auth-flow';
import { apiBase, isDesktopClient } from './runtime';

export interface DesktopOAuthCredential {
  apiKey: string;
  userName: string;
  userId: string;
}

export async function startDesktopGithubOAuth(): Promise<DesktopOAuthCredential> {
  if (!isDesktopClient()) {
    throw new Error('Desktop OAuth can only run inside the CoWiki desktop client.');
  }
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<DesktopOAuthCredential>('start_desktop_oauth', {
    authBaseUrl: buildWebGithubLoginUrl(apiBase()),
  });
}
