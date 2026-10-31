import assert from 'node:assert/strict';
import test from 'node:test';

import {
  answerCard,
  appendCard,
  buildQueue,
  changedSinceReview,
  collectCards,
  deckStats,
  emptyReviewStore,
  isDeck,
  parseCards,
  parseReviewStore,
  previewIntervals,
  schedule,
  setSuspended,
} from '../src/lib/flashcards.ts';

const deck = [
  '---',
  'tags: [flashcards, rust]',
  '---',
  '# Rust',
  '',
  'Capital of France::Paris',
  '- Borrow checker :: Enforces ownership rules at compile time',
  'ownership ::: each value has one owner',
  'card-last-score:: 5',
  '',
  'What does `?` do',
  'in Rust?',
  '?',
  'Propagates the error',
  'to the caller.',
  '',
  '```',
  'Not :: a card',
  '```',
].join('\n');

test('deck notes are recognised by tag in text or frontmatter', () => {
  assert.equal(isDeck(deck), true);
  assert.equal(isDeck('# Notes\n\nSome text #flashcards/rust here\n'), true);
  assert.equal(isDeck('---\ntitle: flashcards are great\n---\nkey:: value\n'), false);
  assert.equal(isDeck('field:: value\n'), false);
});

test('Obsidian Spaced Repetition card formats', () => {
  const cards = parseCards('Rust.md', deck);
  assert.deepEqual(cards.map((item) => [item.line, item.front, item.back]), [
    [5, 'Capital of France', 'Paris'],
    [6, 'Borrow checker', 'Enforces ownership rules at compile time'],
    [7, 'ownership', 'each value has one owner'],
    [7, 'each value has one owner', 'ownership'],
    [10, 'What does `?` do\nin Rust?', 'Propagates the error\nto the caller.'],
  ]);
  assert.equal(parseCards('Notes.md', 'Capital:: Paris\n').length, 0);
});

test('Logseq #card blocks use their children as the answer', () => {
  const cards = parseCards('Logseq.md', '- What is SM-2? #card\n  - A spaced repetition algorithm\n  - card-next-schedule:: 2026-11-01\n- Next block\n');
  assert.deepEqual(cards.map((item) => [item.front, item.back]), [['What is SM-2?', 'A spaced repetition algorithm']]);
});

test('card ids survive moves and edits flag the card', () => {
  const [first] = parseCards('A.md', '#flashcards\nQ :: A\n');
  const [moved] = parseCards('Archive/A.md', '#flashcards\n\nQ  ::  A\n');
  assert.equal(first.id, moved.id);
  const [edited] = parseCards('A.md', '#flashcards\nQ :: B\n');
  assert.equal(edited.id, first.id);
  const store = answerCard(emptyReviewStore(), first, 'good', '2026-10-31');
  assert.equal(changedSinceReview(edited, store.cards[first.id]), true);
  assert.equal(changedSinceReview(first, store.cards[first.id]), false);
  assert.equal(collectCards([{ path: 'A.md', content: '#flashcards\nQ :: A\n' }, { path: 'B.md', content: '#flashcards\nQ :: A\n' }]).length, 1);
});

test('scheduling grows intervals and relearns lapses', () => {
  let state = schedule(undefined, 'good', '2026-10-31');
  assert.deepEqual([state.due, state.interval, state.ease], ['2026-11-01', 1, 2.5]);
  state = schedule(state, 'good', '2026-11-01');
  assert.equal(state.interval, 3);
  state = schedule(state, 'good', '2026-11-04');
  assert.equal(state.interval, 8);
  assert.equal(state.due, '2026-11-12');
  const lapsed = schedule(state, 'again', '2026-11-12');
  assert.deepEqual([lapsed.interval, lapsed.due, lapsed.lapses], [0, '2026-11-12', 1]);
  assert.equal(schedule(lapsed, 'good', '2026-11-12').interval, 1);
  assert.deepEqual(previewIntervals(undefined), { again: 'Today', hard: '1d', good: '1d', easy: '4d' });
  assert.equal(schedule(undefined, 'easy', '2026-10-31').due, '2026-11-04');
});

test('daily queue respects limits, due order and suspension', () => {
  const cards = parseCards('Deck.md', ['#flashcards', 'a :: 1', 'b :: 2', 'c :: 3', 'd :: 4'].join('\n'));
  let store = emptyReviewStore();
  store.settings = { newPerDay: 2, reviewsPerDay: 1 };
  store = answerCard(store, cards[0], 'good', '2026-10-30');
  store = answerCard(store, cards[1], 'good', '2026-10-29');
  store = setSuspended(store, cards[3], true, '2026-10-31');
  const queue = buildQueue(cards, store, '2026-10-31');
  assert.deepEqual(queue.map((item) => item.front), ['b', 'c']);
  assert.deepEqual(deckStats(cards, store, '2026-10-31'), { total: 4, due: 2, fresh: 1, suspended: 1, changed: 0 });
  const after = answerCard(store, cards[2], 'good', '2026-10-31');
  assert.deepEqual(buildQueue(cards, after, '2026-10-31').map((item) => item.front), ['b']);
});

test('stored review state is validated', () => {
  assert.deepEqual(parseReviewStore(null), emptyReviewStore());
  const parsed = parseReviewStore({ cards: { x: { due: '2026-11-01', interval: 1, ease: 2.5, reps: 1, lapses: 0 }, bad: { due: 3 } }, settings: { newPerDay: -5, reviewsPerDay: 50.4 } });
  assert.deepEqual(Object.keys(parsed.cards), ['x']);
  assert.deepEqual(parsed.settings, { newPerDay: 0, reviewsPerDay: 50 });
});

test('adding a card tags the page as a deck once', () => {
  const once = appendCard('# Inbox\n', 'Q1', 'A1');
  assert.equal(once, '# Inbox\n\n#flashcards\n\nQ1 :: A1\n');
  assert.equal(appendCard(once, 'Q2', 'A 2'), '# Inbox\n\n#flashcards\n\nQ1 :: A1\n\nQ2 :: A 2\n');
  assert.equal(parseCards('Inbox.md', appendCard(once, 'Q2', 'A2')).length, 2);
});
