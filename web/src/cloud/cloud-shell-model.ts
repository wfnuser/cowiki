import { canManageMembers, canMerge, type CloudRole } from './session.ts';
import type { CloudTreeEntry } from './client.ts';

export type CloudNavigationId = 'wiki' | 'reviews' | 'members';

export interface CloudNavigationItem {
  id: CloudNavigationId;
  label: string;
  description: string;
}

const NAVIGATION: CloudNavigationItem[] = [
  { id: 'wiki', label: 'Wiki', description: 'Published knowledge' },
  { id: 'reviews', label: 'Reviews', description: 'Cloud pull requests' },
  { id: 'members', label: 'Members', description: 'People and roles' },
];

export function cloudNavigation(_role: CloudRole): CloudNavigationItem[] {
  return NAVIGATION;
}

export function memberManagementMode(role: CloudRole): 'manage' | 'read' {
  return canManageMembers(role) ? 'manage' : 'read';
}

export function mergeActionVisible(role: CloudRole): boolean {
  return canMerge(role);
}

export function resolveInitialCloudPage(entries: CloudTreeEntry[]): string | null {
  const pages = entries.filter((entry) => entry.kind === 'page');
  return pages.find((entry) => entry.path.toLowerCase() === 'index.md')?.path
    ?? pages[0]?.path
    ?? null;
}
