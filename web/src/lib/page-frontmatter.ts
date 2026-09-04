export type EditablePage = {
  systemFrontmatter: string;
  body: string;
};

/** Keep CoWiki-owned metadata out of the editor while preserving it byte-for-byte. */
export function splitSystemFrontmatter(document: string): EditablePage {
  const match = document.match(/^(---\r?\n[\s\S]*?\r?\n---(?:\r?\n)*)/);
  if (!match) return { systemFrontmatter: '', body: document };
  return {
    systemFrontmatter: match[1],
    body: document.slice(match[1].length),
  };
}

export function restoreSystemFrontmatter(systemFrontmatter: string, body: string): string {
  return `${systemFrontmatter}${body}`;
}

/** Return the safe HTTP(S) origin link recorded for a captured web Source. */
export function sourceUrlFromDocument(document: string): string | null {
  const { systemFrontmatter } = splitSystemFrontmatter(document);
  const line = systemFrontmatter
    .split(/\r?\n/)
    .find((candidate) => candidate.startsWith('source_url:'));
  if (!line) return null;

  const encoded = line.slice('source_url:'.length).trim();
  let value = encoded.replace(/^['"]|['"]$/g, '');
  if (encoded.startsWith('"') && encoded.endsWith('"')) {
    try {
      value = JSON.parse(encoded) as string;
    } catch {
      return null;
    }
  }

  try {
    const url = new URL(value);
    if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password) return null;
    return url.toString();
  } catch {
    return null;
  }
}
