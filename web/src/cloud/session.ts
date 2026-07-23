export type CloudRole = 'owner' | 'manager' | 'editor' | 'viewer';

export interface CloudSession {
  baseUrl: string;
  apiKey: string;
  userId: string;
  userName: string;
}

const UUID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

export function normalizeCloudSession(session: CloudSession): CloudSession {
  const baseUrl = normalizeCloudOrigin(session.baseUrl);
  const apiKey = session.apiKey.trim();
  const userId = session.userId.trim();
  const userName = session.userName.trim();
  if (!apiKey) throw new Error('Cloud API key is required');
  if (!isUuid(userId)) throw new Error('Cloud user id must be a UUID');
  if (!userName) throw new Error('Cloud user name is required');
  return { baseUrl, apiKey, userId, userName };
}

export function normalizeCloudOrigin(value: string): string {
  const url = new URL(value.trim());
  if (!['http:', 'https:'].includes(url.protocol)) {
    throw new Error('Cloud URL must use http or https');
  }
  if (url.username || url.password || url.search || url.hash || !['', '/'].includes(url.pathname)) {
    throw new Error('Cloud URL must be an origin without credentials, path, query, or fragment');
  }
  return url.origin;
}

export function isUuid(value: string): boolean {
  return UUID_PATTERN.test(value);
}

export function canPush(role: CloudRole): boolean {
  return role === 'owner' || role === 'manager' || role === 'editor';
}

export function canMerge(role: CloudRole): boolean {
  return role === 'owner' || role === 'manager';
}

export function canManageMembers(role: CloudRole): boolean {
  return canMerge(role);
}
