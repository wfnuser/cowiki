import type { PageMeta } from '../api';
import { conceptIdFromPath, firstConcept } from './okf-pages.ts';

export interface WorkspaceSwitchTarget {
  conceptId: string;
  path: string;
}

export async function resolveWorkspaceSwitchTarget(
  cachedPages: PageMeta[] | undefined,
  loadPages: () => Promise<PageMeta[]>,
): Promise<WorkspaceSwitchTarget | null> {
  const pages = cachedPages ?? await loadPages();
  const first = firstConcept(pages);
  return first
    ? { conceptId: conceptIdFromPath(first.path), path: first.path }
    : null;
}
