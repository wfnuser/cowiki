import { isUuid } from './session.ts';

export type CloudSpaceView = 'wiki' | 'reviews' | 'members';

export interface ParsedCloudRoute {
  spaceId: string;
  view: CloudSpaceView;
  documentPath?: string;
}

export function cloudHomeRoute(): string {
  return '/cloud';
}

export function cloudSpaceRoute(
  spaceId: string,
  view: CloudSpaceView = 'wiki',
  documentPath?: string,
): string {
  if (!isUuid(spaceId)) throw new Error('Cloud Space id must be a UUID');
  const base = `/cloud/spaces/${spaceId}/${view}`;
  if (!documentPath) return base;
  const encodedPath = documentPath.split('/').map(encodeURIComponent).join('/');
  return `${base}/${encodedPath}`;
}

export function parseCloudRoute(pathname: string): ParsedCloudRoute | null {
  const segments = pathname.split('/').filter(Boolean);
  if (segments.length < 4 || segments[0] !== 'cloud' || segments[1] !== 'spaces') return null;
  const [, , spaceId, view, ...pathSegments] = segments;
  if (!isUuid(spaceId) || !isCloudSpaceView(view)) return null;
  try {
    const documentPath = pathSegments.length > 0
      ? pathSegments.map(decodeURIComponent).join('/')
      : undefined;
    return { spaceId, view, ...(documentPath ? { documentPath } : {}) };
  } catch {
    return null;
  }
}

function isCloudSpaceView(value: string): value is CloudSpaceView {
  return value === 'wiki' || value === 'reviews' || value === 'members';
}
