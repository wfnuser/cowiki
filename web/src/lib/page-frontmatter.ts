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
