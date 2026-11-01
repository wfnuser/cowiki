/**
 * Obsidian-compatible wikilink resolution. `[[Note]]` names a file anywhere in
 * the vault by its basename; a path (`[[folder/Note]]`) narrows it. When
 * several files share a name, a file next to the linking page wins, then the
 * shortest path, matching Obsidian's "shortest path when possible" links.
 */

export interface WikilinkTarget {
  /** Page part without `.md`, heading, block or alias; empty for same-page links. */
  page: string;
  heading: string | null;
  block: string | null;
  alias: string | null;
}

export interface LinkablePage {
  path: string;
  title?: string;
}

const IMAGE_EXTENSIONS = /\.(png|jpe?g|gif|webp|svg|bmp|avif|ico|tiff?)$/i;

export function isImagePath(path: string): boolean {
  return IMAGE_EXTENSIONS.test(path.split(/[?#]/)[0]);
}

function key(value: string): string {
  return value.normalize('NFC').toLowerCase();
}

function stripMarkdownExtension(value: string): string {
  return value.replace(/\.md$/i, '');
}

export function parseWikilink(raw: string): WikilinkTarget {
  let value = raw.trim().replace(/^!?\[\[/, '').replace(/\]\]$/, '');
  let alias: string | null = null;
  const pipe = value.indexOf('|');
  if (pipe >= 0) {
    alias = value.slice(pipe + 1).trim() || null;
    // Inside tables Obsidian escapes the alias pipe: [[Note\|alias]].
    value = value.slice(0, pipe).replace(/\\$/, '');
  }
  let heading: string | null = null;
  let block: string | null = null;
  const hash = value.indexOf('#');
  if (hash >= 0) {
    const fragment = value.slice(hash + 1).trim();
    value = value.slice(0, hash);
    if (fragment.startsWith('^')) block = fragment.slice(1) || null;
    else heading = fragment || null;
  }
  return {
    page: stripMarkdownExtension(value.trim().replaceAll('\\', '/').replace(/^\/+/, '')),
    heading,
    block,
    alias,
  };
}

function directoryOf(path: string): string {
  const index = path.lastIndexOf('/');
  return index < 0 ? '' : path.slice(0, index);
}

function basename(path: string): string {
  return path.slice(path.lastIndexOf('/') + 1);
}

function joinRelative(directory: string, relative: string): string {
  const parts = directory ? directory.split('/') : [];
  for (const part of relative.split('/')) {
    if (!part || part === '.') continue;
    if (part === '..') parts.pop();
    else parts.push(part);
  }
  return parts.join('/');
}

function pickClosest(candidates: string[], fromPath: string | null): string | null {
  if (candidates.length === 0) return null;
  const fromDirectory = fromPath ? key(directoryOf(fromPath)) : null;
  return [...candidates].sort((left, right) => {
    if (fromDirectory !== null) {
      const leftSame = key(directoryOf(left)) === fromDirectory ? 0 : 1;
      const rightSame = key(directoryOf(right)) === fromDirectory ? 0 : 1;
      if (leftSame !== rightSame) return leftSame - rightSame;
    }
    const depth = left.split('/').length - right.split('/').length;
    if (depth !== 0) return depth;
    return left.localeCompare(right);
  })[0];
}

/**
 * Resolve a wikilink to a page path among `pages`. Returns null when nothing
 * matches, so the caller can offer to create the page instead.
 */
export function resolveWikilinkPath(
  raw: string,
  pages: LinkablePage[],
  fromPath: string | null = null,
): string | null {
  const target = parseWikilink(raw);
  if (!target.page) return null;
  const wanted = key(target.page);
  const markdown = pages.filter((page) => /\.md$/i.test(page.path));
  const byPath = new Map(markdown.map((page) => [key(stripMarkdownExtension(page.path)), page.path] as const));

  if (target.page.includes('/')) {
    const exact = byPath.get(wanted);
    if (exact) return exact;
    if (fromPath) {
      const relative = byPath.get(key(joinRelative(directoryOf(fromPath), target.page)));
      if (relative) return relative;
    }
    const suffix = markdown
      .map((page) => page.path)
      .filter((path) => key(stripMarkdownExtension(path)).endsWith(`/${wanted}`));
    return pickClosest(suffix, fromPath);
  }

  const byName = markdown
    .map((page) => page.path)
    .filter((path) => key(stripMarkdownExtension(basename(path))) === wanted);
  const named = pickClosest(byName, fromPath);
  if (named) return named;

  const byTitle = markdown
    .filter((page) => page.title && key(page.title.trim()) === wanted)
    .map((page) => page.path);
  return pickClosest(byTitle, fromPath);
}

/** Flatten a page tree (folders with children) into linkable pages. */
export function flattenPages<T extends { path: string; title?: string; kind?: string; children?: T[] }>(
  pages: T[],
): LinkablePage[] {
  const result: LinkablePage[] = [];
  const visit = (nodes: T[]) => {
    for (const node of nodes) {
      if (node.kind === 'folder') {
        visit(node.children ?? []);
      } else {
        result.push({ path: node.path, title: node.title });
        if (node.children?.length) visit(node.children);
      }
    }
  };
  visit(pages);
  return result;
}
