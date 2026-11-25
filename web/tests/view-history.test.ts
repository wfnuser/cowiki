import assert from 'node:assert/strict';
import test from 'node:test';

import {
  EMPTY_HISTORY,
  canGoBack,
  canGoForward,
  forgetPath,
  forgetSpace,
  historyShortcut,
  movePath,
  step,
  visit,
  type HistoryEntry,
  type ViewHistory,
} from '../src/lib/view-history.ts';

const page = (path: string, workspaceSlug = 'mine'): HistoryEntry => ({ workspaceSlug, view: { kind: 'page', path } });
const tool = (kind: 'home' | 'journals' | 'tasks', workspaceSlug = 'mine'): HistoryEntry => ({ workspaceSlug, view: { kind } });
const paths = (history: ViewHistory) => history.entries.map((entry) => (entry.view.kind === 'page' ? entry.view.path : entry.view.kind));

function walk(...entries: HistoryEntry[]): ViewHistory {
  return entries.reduce((history, entry) => visit(history, entry), EMPTY_HISTORY);
}

test('back and forward step through what was opened', () => {
  let history = walk(tool('home'), page('a.md'), page('b.md'));
  assert.equal(canGoBack(history), true);
  assert.equal(canGoForward(history), false);

  const back = step(history, -1);
  assert.ok(back);
  assert.deepEqual(back.entry, page('a.md'));
  history = back.history;
  // Arriving where back pointed records nothing.
  history = visit(history, page('a.md'));
  assert.deepEqual(paths(history), ['home', 'a.md', 'b.md']);
  assert.equal(history.index, 1);
  assert.equal(canGoForward(history), true);

  const forward = step(history, 1);
  assert.deepEqual(forward?.entry, page('b.md'));
});

test('nowhere to go at either end, or before anything opened', () => {
  assert.equal(step(EMPTY_HISTORY, -1), null);
  assert.equal(step(EMPTY_HISTORY, 1), null);
  const one = walk(page('a.md'));
  assert.equal(canGoBack(one), false);
  assert.equal(step(one, -1), null);
  assert.equal(step(one, 1), null);
});

test('opening something new after going back drops the steps ahead', () => {
  let history = walk(page('a.md'), page('b.md'), page('c.md'));
  history = step(history, -1)!.history;
  history = step(history, -1)!.history;
  history = visit(history, page('d.md'));
  assert.deepEqual(paths(history), ['a.md', 'd.md']);
  assert.equal(canGoForward(history), false);
});

test('reopening the page on screen is not a step', () => {
  const history = walk(page('a.md'), page('a.md'), page('a.md'));
  assert.deepEqual(paths(history), ['a.md']);
});

test('switching Spaces is a step, and the same view in another Space is a different one', () => {
  const history = walk(tool('home', 'mine'), tool('home', 'team'));
  assert.equal(history.entries.length, 2);
  assert.deepEqual(step(history, -1)?.entry, tool('home', 'mine'));
});

test('history keeps the newest steps', () => {
  let history = EMPTY_HISTORY;
  for (let i = 0; i < 8; i++) history = visit(history, page(`${i}.md`), 5);
  assert.deepEqual(paths(history), ['3.md', '4.md', '5.md', '6.md', '7.md']);
  assert.equal(history.index, 4);
});

test('a deleted page or folder leaves history, and the repeats it leaves are merged', () => {
  let history = walk(page('a.md'), page('notes/x.md'), page('a.md'), page('notes/y.md'), page('b.md'));
  history = step(history, -1)!.history; // on notes/y.md
  history = forgetPath(history, 'mine', 'notes');
  assert.deepEqual(paths(history), ['a.md', 'b.md']);
  // The removed entry on screen hands over to the step before it.
  assert.equal(history.index, 0);
  // Another Space's page with the same path stays.
  const other = forgetPath(walk(page('a.md', 'team'), page('a.md')), 'mine', 'a.md');
  assert.deepEqual(other.entries, [page('a.md', 'team')]);
});

test('a renamed page or folder keeps its steps under the new name', () => {
  const history = movePath(walk(page('notes/x.md'), page('notes.md'), page('b.md')), 'mine', 'notes', 'ideas');
  assert.deepEqual(paths(history), ['ideas/x.md', 'notes.md', 'b.md']);
  const file = movePath(walk(page('a.md'), page('b.md')), 'mine', 'a.md', 'c.md');
  assert.deepEqual(paths(file), ['c.md', 'b.md']);
});

test('a removed Space leaves history', () => {
  let history = walk(tool('home', 'mine'), tool('home', 'team'), page('a.md', 'team'), tool('tasks', 'mine'));
  history = forgetSpace(history, 'team');
  assert.deepEqual(history.entries, [tool('home', 'mine'), tool('tasks', 'mine')]);
  assert.equal(history.index, 1);
  assert.deepEqual(forgetSpace(walk(tool('home', 'team')), 'team'), EMPTY_HISTORY);
});

test('shortcuts: ⌘[ ⌘] outside text, ⌘⌥← ⌘⌥→ everywhere', () => {
  const key = (k: string, extra: Partial<{ metaKey: boolean; ctrlKey: boolean; altKey: boolean; shiftKey: boolean }> = {}) => ({
    key: k, metaKey: true, ctrlKey: false, altKey: false, shiftKey: false, ...extra,
  });
  assert.equal(historyShortcut(key('['), false), -1);
  assert.equal(historyShortcut(key(']'), false), 1);
  // In the editor ⌘[ and ⌘] outdent and indent.
  assert.equal(historyShortcut(key('['), true), null);
  assert.equal(historyShortcut(key('ArrowLeft', { altKey: true }), true), -1);
  assert.equal(historyShortcut(key('ArrowRight', { altKey: true }), false), 1);
  // ⌘← alone moves the caret to the line start.
  assert.equal(historyShortcut(key('ArrowLeft'), false), null);
  assert.equal(historyShortcut(key('[', { metaKey: false }), false), null);
  assert.equal(historyShortcut(key('[', { shiftKey: true }), false), null);
  assert.equal(historyShortcut(key('[', { metaKey: false, ctrlKey: true }), false), -1);
});
