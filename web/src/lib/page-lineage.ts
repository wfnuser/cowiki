export interface CommitProvenance {
  oid: string;
  summary: string;
  author: string;
  committedAt: number;
}

export interface AgentProvenance {
  name: string;
  changeId: string;
  task: string;
}

export interface ReviewProvenance {
  id: string;
  number: number;
  title: string;
}

export interface DocumentProvenance {
  commit?: CommitProvenance | null;
  agents?: AgentProvenance[] | null;
  review?: ReviewProvenance | null;
}

export interface PageLineage {
  sources: string[];
  commit: CommitProvenance | null;
  agents: AgentProvenance[];
  review: ReviewProvenance | null;
}

export function pageLineage(document: string, provenance?: DocumentProvenance | null): PageLineage {
  return {
    sources: frontmatterSources(document),
    commit: provenance?.commit ?? null,
    agents: provenance?.agents ?? [],
    review: provenance?.review ?? null,
  };
}

export function sourceFilename(path: string): string {
  return path.replace(/^\.cowiki\/sources\//, '');
}

export function sourceOriginalUrl(document: string): string | null {
  const match = document.match(/^---\r?\n([\s\S]*?)\r?\n---(?:\r?\n|$)/);
  const value = match?.[1]
    .split(/\r?\n/)
    .find((line) => /^source_url\s*:/.test(line))
    ?.replace(/^source_url\s*:\s*/, '')
    .trim()
    .replace(/^['"]|['"]$/g, '');
  if (!value) return null;
  try {
    const url = new URL(value);
    return url.protocol === 'http:' || url.protocol === 'https:' ? url.toString() : null;
  } catch {
    return null;
  }
}

function frontmatterSources(document: string): string[] {
  const match = document.match(/^---\r?\n([\s\S]*?)\r?\n---(?:\r?\n|$)/);
  if (!match) return [];
  const lines = match[1].split(/\r?\n/);
  const values: string[] = [];
  for (let index = 0; index < lines.length; index += 1) {
    const field = lines[index].match(/^sources\s*:\s*(.*?)\s*$/);
    if (!field) continue;
    if (field[1]) {
      const inline = field[1].trim();
      const entries = inline.startsWith('[') && inline.endsWith(']')
        ? inline.slice(1, -1).split(',')
        : [inline];
      values.push(...entries);
    } else {
      for (let item = index + 1; item < lines.length; item += 1) {
        const entry = lines[item].match(/^\s+-\s+(.+?)\s*$/);
        if (!entry) break;
        values.push(entry[1]);
      }
    }
    break;
  }
  return [...new Set(values.map(normalizeSourceRef).filter((value): value is string => !!value))];
}

function normalizeSourceRef(raw: string): string | null {
  const value = raw.trim().replace(/^['"]|['"]$/g, '');
  const relative = value.startsWith('.cowiki/sources/')
    ? value.slice('.cowiki/sources/'.length)
    : value.startsWith('sources/')
      ? value.slice('sources/'.length)
      : value;
  if (!relative.toLowerCase().endsWith('.md')) return null;
  const parts = relative.split('/');
  if (parts.some((part) => !part || part === '.' || part === '..' || part.startsWith('.'))) return null;
  return `.cowiki/sources/${parts.join('/')}`;
}
