/**
 * Back and forward through what you looked at, as in Notion, Obsidian or a
 * browser: every page, journal, tool view and Space you open is one step.
 * Kept in memory for this window only; nothing is saved.
 */
import type { ReviewTarget } from '@/components/review/review-navigation';

export type HistoryView =
  | { kind: 'page'; path: string }
  | { kind: 'source'; filename: string }
  | { kind: 'review-detail'; target: ReviewTarget }
  | { kind: 'home' | 'journals' | 'review-list' | 'members' | 'history' | 'links' | 'tasks' | 'flashcards' };

export interface HistoryEntry {
  workspaceSlug: string;
  view: HistoryView;
}

export interface ViewHistory {
  readonly entries: readonly HistoryEntry[];
  /** The entry on screen; -1 before anything was opened. */
  readonly index: number;
}

export const EMPTY_HISTORY: ViewHistory = { entries: [], index: -1 };

/** Enough steps for a long session; the oldest fall off first. */
export const HISTORY_LIMIT = 100;

export function entryKey(entry: HistoryEntry): string {
  return JSON.stringify([entry.workspaceSlug, entry.view]);
}

function sameEntry(a: HistoryEntry | undefined, b: HistoryEntry): boolean {
  return !!a && entryKey(a) === entryKey(b);
}

/**
 * Opening something new drops the steps ahead of you, as in a browser.
 * Showing the entry already on screen (a reload, or arriving where back or
 * forward pointed) changes nothing.
 */
export function visit(history: ViewHistory, entry: HistoryEntry, limit = HISTORY_LIMIT): ViewHistory {
  if (sameEntry(history.entries[history.index], entry)) return history;
  const entries = [...history.entries.slice(0, history.index + 1), entry];
  const start = Math.max(0, entries.length - limit);
  return { entries: entries.slice(start), index: entries.length - 1 - start };
}

export function canGoBack(history: ViewHistory): boolean {
  return history.index > 0;
}

export function canGoForward(history: ViewHistory): boolean {
  return history.index >= 0 && history.index < history.entries.length - 1;
}

/** Move one step (-1 back, +1 forward); null when there is nowhere to go. */
export function step(history: ViewHistory, delta: -1 | 1): { history: ViewHistory; entry: HistoryEntry } | null {
  const index = history.index + delta;
  const entry = history.entries[index];
  if (!entry || history.index < 0) return null;
  return { history: { ...history, index }, entry };
}

function under(path: string, root: string): boolean {
  return path === root || path.startsWith(`${root}/`);
}

/** Keep the index on the same entry while removing others; merge repeats left behind. */
function rebuild(history: ViewHistory, mapped: (HistoryEntry | null)[]): ViewHistory {
  const entries: HistoryEntry[] = [];
  let index = -1;
  mapped.forEach((entry, position) => {
    if (entry && !sameEntry(entries[entries.length - 1], entry)) entries.push(entry);
    if (position <= history.index && entries.length > 0) index = entries.length - 1;
  });
  return { entries, index: entries.length === 0 ? -1 : index };
}

/** A page or folder was deleted: its steps go, so back never opens a missing page. */
export function forgetPath(history: ViewHistory, workspaceSlug: string, path: string): ViewHistory {
  return rebuild(history, history.entries.map((entry) => (
    entry.workspaceSlug === workspaceSlug && entry.view.kind === 'page' && under(entry.view.path, path) ? null : entry
  )));
}

/** A page or folder was renamed or moved: its steps follow it. */
export function movePath(history: ViewHistory, workspaceSlug: string, from: string, to: string): ViewHistory {
  return rebuild(history, history.entries.map((entry) => {
    if (entry.workspaceSlug !== workspaceSlug || entry.view.kind !== 'page' || !under(entry.view.path, from)) return entry;
    return { ...entry, view: { kind: 'page', path: to + entry.view.path.slice(from.length) } };
  }));
}

/** A Space was removed: none of its steps can be opened again. */
export function forgetSpace(history: ViewHistory, workspaceSlug: string): ViewHistory {
  return rebuild(history, history.entries.map((entry) => (entry.workspaceSlug === workspaceSlug ? null : entry)));
}

/**
 * The key that moves through history, or null. ⌘[ and ⌘] (Notion, Safari,
 * Finder) only outside text, where the editor uses them to outdent and
 * indent; ⌘⌥← and ⌘⌥→ (Obsidian) everywhere; the mouse's side buttons are
 * handled separately.
 */
export function historyShortcut(event: {
  key: string; metaKey: boolean; ctrlKey: boolean; altKey: boolean; shiftKey: boolean;
}, inText: boolean): -1 | 1 | null {
  const mod = event.metaKey || event.ctrlKey;
  if (!mod || event.shiftKey) return null;
  if (event.altKey) {
    if (event.key === 'ArrowLeft') return -1;
    if (event.key === 'ArrowRight') return 1;
    return null;
  }
  if (inText) return null;
  if (event.key === '[') return -1;
  if (event.key === ']') return 1;
  return null;
}
