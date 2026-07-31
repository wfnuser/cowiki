import type { PageMeta } from '../api';

const MARKDOWN_EXTENSION = /\.md$/i;

export function normalizeRepoPath(value: string): string {
  return value.replaceAll('\\', '/').replace(/^\.\//, '').replace(/^\/+|\/+$/g, '');
}

export function conceptIdFromPath(path: string): string {
  return normalizeRepoPath(path).replace(MARKDOWN_EXTENSION, '');
}

export function conceptPath(conceptId: string): string {
  return `${normalizeRepoPath(conceptId).replace(MARKDOWN_EXTENSION, '')}.md`;
}

export function isReservedDocument(path: string): boolean {
  const name = normalizeRepoPath(path).split('/').at(-1);
  return name === 'index.md' || name === 'log.md';
}

export function isHiddenRepoPath(path: string): boolean {
  return normalizeRepoPath(path).split('/').some((part) => part.startsWith('.'));
}

export function isSearchableConceptPath(path: string): boolean {
  const normalized = normalizeRepoPath(path);
  return MARKDOWN_EXTENSION.test(normalized)
    && !isHiddenRepoPath(normalized)
    && !isReservedDocument(normalized);
}

export function isConceptPage(page: PageMeta): boolean {
  const path = normalizeRepoPath(page.path);
  return page.kind === 'page' && isSearchableConceptPath(path);
}

function sortPages(pages: PageMeta[]): PageMeta[] {
  return [...pages].sort((left, right) => {
    const kind = Number(left.kind !== 'folder') - Number(right.kind !== 'folder');
    if (kind !== 0) return kind;
    return (left.title || left.path).localeCompare(right.title || right.path, undefined, { sensitivity: 'base' });
  });
}

/**
 * Present an OKF bundle as one tree. Generated index/log files and CoWiki's
 * hidden storage remain addressable on disk but are not editable concepts.
 */
function visibleNodes(pages: PageMeta[]): PageMeta[] {
  const visible = pages.flatMap((page): PageMeta[] => {
    const path = normalizeRepoPath(page.path);
    if (!path || isHiddenRepoPath(path)) return [];
    if (page.kind === 'page') {
      const normalized = { ...page, slug: conceptIdFromPath(path), path };
      return isConceptPage(normalized) ? [normalized] : [];
    }
    return [{
      ...page,
      slug: path,
      path,
      children: visibleNodes(page.children || []),
    }];
  });
  return sortPages(visible);
}

export function visiblePageTree(pages: PageMeta[]): PageMeta[] {
  // The local API contract is already canonical. Never infer or strip a
  // transport root from user-controlled folder names or index titles.
  return visibleNodes(pages);
}

export function findConcept(pages: PageMeta[], conceptId: string): PageMeta | undefined {
  const expected = conceptPath(conceptId);
  for (const page of pages) {
    if (page.kind === 'page' && normalizeRepoPath(page.path) === expected) return page;
    const nested = findConcept(page.children || [], conceptId);
    if (nested) return nested;
  }
  return undefined;
}

export function firstConcept(pages: PageMeta[]): PageMeta | undefined {
  for (const page of pages) {
    if (page.kind === 'page') return page;
    const nested = firstConcept(page.children || []);
    if (nested) return nested;
  }
  return undefined;
}

export function submitConceptPaths(pages: PageMeta[]): string[] {
  return pages.flatMap((page) => page.kind === 'folder'
    ? submitConceptPaths(page.children || [])
    : [normalizeRepoPath(page.path)]);
}

/**
 * Local Git owns the complete working tree, including hidden Source files, so
 * an empty visible Concept list must still reach the desktop submit command.
 */
export function shouldAttemptSubmit(desktop: boolean, conceptPaths: string[]): boolean {
  return desktop || conceptPaths.length > 0;
}

export function pageRoute(workspaceSlug: string, conceptId: string): string {
  const encodedId = normalizeRepoPath(conceptId)
    .split('/')
    .map(encodeURIComponent)
    .join('/');
  return `/${encodeURIComponent(workspaceSlug)}/${encodedId}`;
}
