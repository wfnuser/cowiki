/**
 * File name for a page in a local Space, kept readable like Obsidian:
 * "周记" stays "周记", "Attention Is All You Need" keeps its spaces. Only
 * characters that break file systems or wikilinks are replaced.
 */
export function pageFileName(name: string): string {
  return name
    .normalize('NFC')
    .replace(/[\\/:*?"<>|#^[\]]/g, '-')
    .replace(/\s+/g, ' ')
    .trim()
    .replace(/^\.+/, '')
    .replace(/[. ]+$/, '')
    .slice(0, 180);
}
