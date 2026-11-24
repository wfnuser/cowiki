/**
 * Obsidian callouts (`> [!tip] Title`), shared by the reader and the editor so
 * both draw the same icon, colour and title (DESIGN_SYSTEM.md, "Callouts").
 * Icons are lucide's, drawn as CSS masks (`.callout-icon[data-icon]` in
 * index.css), so the editor needs no React to show them.
 */
export type CalloutFamily = 'note' | 'tip' | 'warning' | 'danger' | 'quote';

const TYPES: Record<string, { icon: string; family: CalloutFamily }> = {
  note: { icon: 'pencil', family: 'note' },
  info: { icon: 'info', family: 'note' },
  todo: { icon: 'circle-check', family: 'note' },
  abstract: { icon: 'clipboard-list', family: 'note' },
  summary: { icon: 'clipboard-list', family: 'note' },
  tldr: { icon: 'clipboard-list', family: 'note' },
  tip: { icon: 'lightbulb', family: 'tip' },
  hint: { icon: 'lightbulb', family: 'tip' },
  important: { icon: 'flame', family: 'tip' },
  success: { icon: 'circle-check', family: 'tip' },
  check: { icon: 'circle-check', family: 'tip' },
  done: { icon: 'circle-check', family: 'tip' },
  question: { icon: 'circle-question-mark', family: 'warning' },
  help: { icon: 'circle-question-mark', family: 'warning' },
  faq: { icon: 'circle-question-mark', family: 'warning' },
  warning: { icon: 'triangle-alert', family: 'warning' },
  caution: { icon: 'triangle-alert', family: 'warning' },
  attention: { icon: 'triangle-alert', family: 'warning' },
  failure: { icon: 'circle-x', family: 'danger' },
  fail: { icon: 'circle-x', family: 'danger' },
  missing: { icon: 'circle-x', family: 'danger' },
  danger: { icon: 'zap', family: 'danger' },
  error: { icon: 'zap', family: 'danger' },
  bug: { icon: 'bug', family: 'danger' },
  example: { icon: 'list', family: 'quote' },
  quote: { icon: 'quote', family: 'quote' },
  cite: { icon: 'quote', family: 'quote' },
};

export interface Callout {
  type: string;
  icon: string;
  family: CalloutFamily;
  /** The title as written, or the type's name ("Tip") when none is. */
  title: string;
}

export function calloutOf(type: string, title = ''): Callout {
  const key = type.toLowerCase();
  const known = TYPES[key] ?? TYPES.note;
  return { type: key, ...known, title: title.trim() || `${key[0]?.toUpperCase() ?? ''}${key.slice(1)}` };
}

/** `> [!tip]- Title` → the callout and where its `[!tip]-` marker sits in the line. */
export const CALLOUT_LINE = /^(\s*(?:>\s*)+)(\[!([A-Za-z-]+)\][+-]?)\s*(.*)$/;

/**
 * In reader Markdown a callout title is `**<SEP>type<SEP> Title**`; the
 * reader's `strong` turns it back into an icon and title. SEP is an invisible
 * separator, so a title that is not handled still reads cleanly.
 */
export const CALLOUT_SEP = '⁣';
