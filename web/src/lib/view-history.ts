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
  /** Flashcards; `deck` (`Japanese/N2`) when one deck is open, `studying` during a session. */
  | { kind: 'flashcards'; deck?: string; studying?: true }
  | { kind: 'home' | 'journals' | 'review-list' | 'members' | 'history' | 'links' | 'tasks' };

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

/** Steps that can no longer be shown (their Space removed, their module turned off) leave history. */
export function forgetWhere(history: ViewHistory, gone: (entry: HistoryEntry) => boolean): ViewHistory {
  return rebuild(history, history.entries.map((entry) => (gone(entry) ? null : entry)));
}

/** A Space was removed: none of its steps can be opened again. */
export function forgetSpace(history: ViewHistory, workspaceSlug: string): ViewHistory {
  return forgetWhere(history, (entry) => entry.workspaceSlug === workspaceSlug);
}

/** A local Agent Change was merged or discarded: its detail steps leave history. */
export function forgetAgentChange(history: ViewHistory, workspaceSlug: string, changeId: string): ViewHistory {
  return forgetWhere(history, (entry) => (
    entry.workspaceSlug === workspaceSlug
    && entry.view.kind === 'review-detail'
    && entry.view.target.kind === 'local-agent'
    && entry.view.target.changeId === changeId
  ));
}

/**
 * Drop entries that can no longer be shown and step by `delta`.
 * When the current entry itself was removed (for example, stepping Back
 * from a just-merged Agent Change), `delta === -1` lands on the nearest
 * surviving entry before it instead of skipping two steps, and `delta === 1`
 * lands on the nearest surviving entry after it.
 */
export function stepAvailable(
  history: ViewHistory,
  delta: -1 | 1,
  gone: (entry: HistoryEntry) => boolean,
): { history: ViewHistory; entry: HistoryEntry } | null {
  const current = history.index >= 0 ? history.entries[history.index] : undefined;
  const currentGone = !!current && gone(current);
  const cleaned = forgetWhere(history, gone);
  if (!currentGone) return step(cleaned, delta);
  if (delta === -1) {
    const entry = cleaned.index >= 0 ? cleaned.entries[cleaned.index] : undefined;
    return entry ? { history: cleaned, entry } : null;
  }
  const nextIndex = cleaned.index + 1;
  if (nextIndex < cleaned.entries.length) {
    return { history: { entries: cleaned.entries, index: nextIndex }, entry: cleaned.entries[nextIndex] };
  }
  const hasSurvivingAfter = history.entries.slice(history.index + 1).some((entry) => !gone(entry));
  if (hasSurvivingAfter && cleaned.index >= 0) {
    return { history: cleaned, entry: cleaned.entries[cleaned.index] };
  }
  return null;
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
