/**
 * Render-time compatibility for Obsidian Markdown in the reader. The page file
 * is never rewritten; this only prepares text for react-markdown.
 *
 * Every transform keeps the line count, so comment anchors and task toggles
 * that refer to source lines still line up with the file.
 */
import { isImagePath, parseWikilink } from './wikilinks.ts';

/** Fragment prefix react-markdown keeps; the reader turns it into navigation. */
export const WIKILINK_HREF_PREFIX = '#cowiki-wiki:';

export function wikilinkHref(target: string): string {
  return `${WIKILINK_HREF_PREFIX}${encodeURIComponent(target)}`;
}

export function wikilinkFromHref(href: string | undefined | null): string | null {
  if (!href || !href.startsWith(WIKILINK_HREF_PREFIX)) return null;
  try {
    return decodeURIComponent(href.slice(WIKILINK_HREF_PREFIX.length));
  } catch {
    return null;
  }
}

function escapeLabel(value: string): string {
  return value.replace(/([\\[\]*_`])/g, '\\$1');
}

function angleTarget(value: string): string {
  return `<${value.replace(/[<>]/g, (char) => encodeURIComponent(char))}>`;
}

const CALLOUT_ICONS: Record<string, string> = {
  note: '📝', info: 'ℹ️', tip: '💡', hint: '💡', important: '❗', success: '✅', check: '✅', done: '✅',
  question: '❓', help: '❓', faq: '❓', warning: '⚠️', caution: '⚠️', attention: '⚠️',
  failure: '❌', fail: '❌', missing: '❌', danger: '⛔', error: '⛔', bug: '🐞',
  example: '📋', quote: '❝', cite: '❝', abstract: '📄', summary: '📄', tldr: '📄', todo: '☑️',
};

function convertInline(segment: string): string {
  let text = segment.replace(/%%.*?%%/g, '').replace(/\{\{cloze:?\s+(.*?)\}\}/g, '==$1==');
  text = text.replace(/(!?)\[\[([^\]\n]+?)\]\]/g, (_match, bang: string, inner: string) => {
    const target = parseWikilink(inner);
    const page = target.page;
    if (bang && page && isImagePath(page)) {
      const alt = target.alias && !/^\d+(x\d+)?$/.test(target.alias) ? target.alias : '';
      return `![${escapeLabel(alt)}](${angleTarget(page)})`;
    }
    const pipe = inner.indexOf('|');
    const shown = pipe >= 0
      ? inner.slice(pipe + 1).trim()
      : (target.heading && !page ? target.heading : inner.replace(/#\^.*$/, '').replace(/#/, ' › ').trim());
    const label = `${bang ? '↪ ' : ''}${shown || page}`;
    return `[${escapeLabel(label)}](${wikilinkHref(inner.trim())})`;
  });
  text = text.replace(/==([^=\n]+)==/g, '**$1**');
  return text.replace(/\s\^[A-Za-z0-9-]+\s*$/, '');
}

/** Convert one line outside code, leaving inline code spans untouched. */
/**
 * Show Obsidian Tasks statuses and Logseq markers as GFM checkboxes so they
 * can be ticked in the reader: in progress reads as open, cancelled as
 * checked and struck through.
 */
function convertTaskMarker(line: string): string {
  const checkbox = /^(\s*(?:[-*+]|\d+[.)])\s+)\[([/-])\](\s+)(.*)$/.exec(line);
  if (checkbox) {
    const [, prefix, status, gap, text] = checkbox;
    return status === '/' ? `${prefix}[ ]${gap}${text}` : `${prefix}[x]${gap}~~${text}~~`;
  }
  const logseq = /^(\s*[-*+]\s+)(TODO|LATER|NOW|DOING|DONE|CANCELED|CANCELLED)\s+(.*)$/.exec(line);
  if (logseq) {
    const [, prefix, marker, text] = logseq;
    if (marker === 'DONE') return `${prefix}[x] ${text}`;
    if (marker.startsWith('CANCEL')) return `${prefix}[x] ~~${text}~~`;
    return `${prefix}[ ] ${text}`;
  }
  return line;
}

/** Logseq bookkeeping properties: flashcard schedules, block ids, folding. */
const HIDDEN_PROPERTY = /^\s*(?:card-[a-z-]+|id|collapsed)::/i;

function convertLine(raw: string): string {
  // Blank instead of removed, so rendered lines still match the file's lines.
  if (HIDDEN_PROPERTY.test(raw)) return '';
  const line = convertTaskMarker(raw);
  const callout = /^(\s*(?:>\s*)+)\[!([A-Za-z-]+)\]([+-]?)\s*(.*)$/.exec(line);
  if (callout) {
    const type = callout[2].toLowerCase();
    const icon = CALLOUT_ICONS[type] ?? '📌';
    const title = callout[4].trim() || `${callout[2][0].toUpperCase()}${callout[2].slice(1).toLowerCase()}`;
    return `${callout[1]}**${icon} ${convertInline(title)}**`;
  }
  const parts = line.split(/(`+[^`]*`+)/);
  return parts.map((part, index) => (index % 2 === 1 ? part : convertInline(part))).join('');
}

export function obsidianToReaderMarkdown(body: string): string {
  const lines = body.split('\n');
  let fence: string | null = null;
  let comment = false;
  return lines.map((line) => {
    const marker = /^\s*(`{3,}|~{3,})/.exec(line)?.[1];
    if (fence) {
      if (marker && marker[0] === fence[0] && marker.length >= fence.length) fence = null;
      return line;
    }
    if (marker) {
      fence = marker;
      return line;
    }
    if (comment) {
      if (line.includes('%%')) {
        comment = false;
        return convertLine(line.slice(line.indexOf('%%') + 2));
      }
      return '';
    }
    const opens = (line.match(/%%/g) ?? []).length;
    if (opens % 2 === 1) {
      comment = true;
      return convertLine(line.slice(0, line.lastIndexOf('%%')));
    }
    return convertLine(line);
  }).join('\n');
}

/** Number of lines that precede the rendered body inside the full file. */
export function renderedLineOffset(fullBody: string, rendered: string): number {
  if (!rendered) return 0;
  const index = fullBody.indexOf(rendered);
  if (index < 0) return 0;
  return fullBody.slice(0, index).split('\n').length - 1;
}
