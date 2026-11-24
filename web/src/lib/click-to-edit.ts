/**
 * Click to edit, as in Logseq: a click in rendered Markdown opens the editor
 * with the caret where the click landed. The reader and the editor render the
 * same source differently, so the click is mapped through its text, not its
 * pixel position.
 */

/** The text under a click in the rendered page. */
export interface RenderedClick {
  /** Text of the clicked text node. */
  text: string;
  /** Caret offset inside that text. */
  offset: number;
  /** Where that text sits in the whole rendered page, from 0 (top) to 1 (end). */
  ratio: number;
  /** Viewport top of the clicked character, to open the editor with it in place. */
  clientTop?: number;
}

const CONTEXT = 24;
const MIN_RUN = 3;

function occurrences(source: string, needle: string): number[] {
  const found: number[] = [];
  if (!needle) return found;
  for (let index = source.indexOf(needle); index >= 0; index = source.indexOf(needle, index + 1)) {
    found.push(index);
    if (found.length > 200) break;
  }
  return found;
}

/**
 * Offset in `source` that matches a click in its rendered form, or null when
 * the clicked text cannot be found (the caller then puts the caret at the end).
 */
export function sourceOffsetForClick(source: string, click: RenderedClick): number | null {
  const { text } = click;
  const offset = Math.max(0, Math.min(click.offset, text.length));
  if (!text.trim()) return null;
  const target = Math.max(0, Math.min(1, click.ratio)) * source.length;
  const nearest = (needle: string): number | null => {
    if (!needle.trim()) return null;
    const found = occurrences(source, needle);
    if (!found.length) return null;
    return found.reduce((best, index) => (Math.abs(index - target) < Math.abs(best - target) ? index : best));
  };
  const whole = nearest(text);
  if (whole !== null) return whole + offset;
  // Markup can change the text (entities, aliases): match the longest run of
  // words right before the caret, then right after it.
  const before = text.slice(Math.max(0, offset - CONTEXT), offset);
  for (let length = before.length; length >= MIN_RUN; length--) {
    const at = nearest(before.slice(-length));
    if (at !== null) return at + length;
  }
  const after = text.slice(offset, offset + CONTEXT);
  for (let length = after.length; length >= MIN_RUN; length--) {
    const at = nearest(after.slice(0, length));
    if (at !== null) return at;
  }
  return null;
}

const NOT_EDITABLE = 'a, button, input, textarea, select, label, summary, [role="button"], [contenteditable], [data-no-edit]';

/**
 * The rendered text under a click inside `root`, or `undefined` when the click
 * should not start editing (a link, a control, or the end of a text selection).
 * `null` means the click was on blank space: edit with the caret at the end.
 */
export function renderedClick(event: MouseEvent, root: HTMLElement): RenderedClick | null | undefined {
  if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return undefined;
  const target = event.target instanceof Element ? event.target : null;
  if (target?.closest(NOT_EDITABLE)) return undefined;
  const selection = window.getSelection();
  if (selection && !selection.isCollapsed && selection.toString().trim()) return undefined;

  const caret = caretAt(event.clientX, event.clientY);
  if (!caret || !root.contains(caret.node)) return null;
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  let before = 0;
  let total = 0;
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    if (node === caret.node) before = total;
    total += node.textContent?.length ?? 0;
  }
  return {
    text: caret.node.textContent ?? '',
    offset: caret.offset,
    ratio: total ? (before + caret.offset) / total : 1,
    clientTop: characterTop(caret.node, caret.offset),
  };
}

/** Top of the character at `offset` (or the one before it at the end of the text). */
function characterTop(node: Text, offset: number): number | undefined {
  const length = node.textContent?.length ?? 0;
  if (!length) return undefined;
  const from = offset < length ? offset : length - 1;
  const range = document.createRange();
  range.setStart(node, from);
  range.setEnd(node, from + 1);
  const rect = range.getClientRects()[0] ?? range.getBoundingClientRect();
  return rect.height ? rect.top : undefined;
}

function caretAt(x: number, y: number): { node: Text; offset: number } | null {
  const doc = document as Document & {
    caretPositionFromPoint?: (x: number, y: number) => { offsetNode: Node; offset: number } | null;
    caretRangeFromPoint?: (x: number, y: number) => Range | null;
  };
  if (doc.caretPositionFromPoint) {
    const position = doc.caretPositionFromPoint(x, y);
    if (position?.offsetNode.nodeType === Node.TEXT_NODE) return { node: position.offsetNode as Text, offset: position.offset };
  }
  if (doc.caretRangeFromPoint) {
    const range = doc.caretRangeFromPoint(x, y);
    if (range?.startContainer.nodeType === Node.TEXT_NODE) return { node: range.startContainer as Text, offset: range.startOffset };
  }
  return null;
}

/**
 * A journal entry's frontmatter and leading `# Title`, kept apart so the
 * journal shows each day under its date instead of repeating the title.
 */
export function splitLeadingTitle(full: string): { head: string; content: string } {
  const frontmatter = /^---\r?\n[\s\S]*?\r?\n---(?:\r?\n|$)/.exec(full);
  let index = frontmatter ? frontmatter[0].length : 0;
  const title = /^(?:[ \t]*\r?\n)*#[ \t]+[^\r\n]*(?:\r?\n|$)(?:[ \t]*\r?\n)*/.exec(full.slice(index));
  if (title) index += title[0].length;
  return { head: full.slice(0, index), content: full.slice(index) };
}

/** Put an edited entry back together under its unchanged head. */
export function joinLeadingTitle(head: string, content: string): string {
  if (!head || !content || /\n$/.test(head)) return head + content;
  return `${head}\n${content}`;
}
