import assert from 'node:assert/strict';
import test from 'node:test';

import {
  DEFAULT_SCHEDULE,
  adoptMovedCards,
  answerCard,
  buildQueue,
  changedSinceReview,
  collectCards,
  deckStats,
  deckTree,
  emptyReviewStore,
  isDue,
  isNew,
  masteryOf,
  parseCards,
  parseReviewStore,
  previewIntervals,
  renderCloze,
  resetCard,
  review,
  scheduleOf,
  setSuspended,
  type OfMatrix,
} from '../src/lib/flashcards.ts';

const page = [
  '---',
  'title: Rust',
  '---',
  '# Rust',
  '',
  '- What does the borrow checker enforce? #card',
  '  id:: 6540a1b2',
  '  card-last-interval:: 4',
  '  card-repeats:: 1',
  '  card-ease-factor:: 2.6',
  '  card-next-schedule:: 2026-11-05T02:00:00.000Z',
  '  card-last-reviewed:: 2026-11-01T02:00:00.000Z',
  '  card-last-score:: 5',
  '  - Ownership rules',
  '    - at compile time',
  '- Rust has {{cloze zero-cost}} abstractions [[card]]',
  '- A normal bullet',
  '\t- Nested question #Card',
  '\t\t- Nested answer',
  '',
  '```',
  '- Not a card #card',
  '```',
  'Plain text #card is not a block',
].join('\n');

test('Logseq #card blocks: question, children as answer, Logseq ids and imported schedules', () => {
  const cards = parseCards('Rust.md', page);
  assert.deepEqual(cards.map((item) => [item.line, item.front, item.back, item.cloze]), [
    [5, 'What does the borrow checker enforce?', '- Ownership rules\n  - at compile time', false],
    [15, 'Rust has {{cloze zero-cost}} abstractions', '', true],
    [17, 'Nested question', '- Nested answer', false],
  ]);
  // A Logseq block id keeps the card's identity across moves and edits.
  assert.equal(cards[0].id, 'block:6540a1b2');
  assert.deepEqual(cards[0].imported, {
    lastInterval: 4, repeats: 1, ease: 2.6,
    nextSchedule: '2026-11-05T02:00:00.000Z', lastReviewed: '2026-11-01T02:00:00.000Z', lastScore: 5,
  });
  assert.equal(cards[1].imported, null);
  assert.equal(collectCards([{ path: 'A.md', content: page }, { path: 'B.md', content: '- Q #card\n  - A\n' }]).length, 4);
});

test('cloze text is hidden on the question and highlighted on the answer', () => {
  assert.equal(renderCloze('Rust has {{cloze zero-cost}} abstractions', false), 'Rust has [...] abstractions');
  assert.equal(renderCloze('Rust has {{cloze: zero-cost}} abstractions', true), 'Rust has ==zero-cost== abstractions');
});

test('SM-5 scheduling matches Logseq', () => {
  const now = new Date('2026-11-01T10:00:00.000Z');
  let matrix: OfMatrix = {};
  const first = review(DEFAULT_SCHEDULE, 5, matrix, now);
  assert.deepEqual(first.schedule, {
    lastInterval: 4, repeats: 1, ease: 2.6,
    nextSchedule: '2026-11-05T10:00:00.000Z', lastReviewed: '2026-11-01T10:00:00.000Z', lastScore: 5,
  });
  assert.deepEqual(first.matrix, { 0: { 2.5: 4.14 } });
  matrix = first.matrix;
  const second = review(first.schedule, 5, matrix, now);
  assert.equal(second.schedule.lastInterval, 4);
  assert.equal(second.schedule.repeats, 2);
  assert.equal(second.schedule.ease, 2.7);
  const third = review(second.schedule, 5, second.matrix, now);
  assert.equal(third.schedule.lastInterval, 11.2);
  assert.equal(third.schedule.ease, 2.8);
  assert.equal(third.matrix['2']['2.7'], 2.79);

  const recalled = review(DEFAULT_SCHEDULE, 3, {}, now);
  assert.equal(recalled.schedule.lastInterval, 4);
  assert.equal(recalled.schedule.ease, 2.36);

  const forgotten = review(second.schedule, 1, {}, now);
  assert.equal(forgotten.schedule.lastInterval, -1);
  assert.equal(forgotten.schedule.repeats, 1);
  assert.equal(forgotten.schedule.ease, 2.7);
  const local = new Date(forgotten.schedule.nextSchedule!);
  assert.equal(local.getHours(), 0);
  assert.equal(local.getTime() > now.getTime(), true);
});

test('review history stays out of the note, with daily limits and suspension', () => {
  const now = new Date('2026-11-06T00:00:00.000Z');
  const today = '2026-11-06';
  const cards = parseCards('Rust.md', page);
  let store = emptyReviewStore();
  // The imported schedule is due again by now; the two others are new.
  assert.equal(isDue(scheduleOf(store, cards[0]), now), true);
  assert.equal(isNew(store, cards[0]), false);
  assert.deepEqual(deckStats(cards, store, now), { total: 3, due: 1, fresh: 2, suspended: 0, changed: 0 });
  assert.deepEqual(buildQueue(cards, store, now, today).map((item) => item.line), [5, 15, 17]);

  store = { ...store, settings: { newPerDay: 1, reviewsPerDay: 200 } };
  assert.deepEqual(buildQueue(cards, store, now, today).map((item) => item.line), [5, 15]);
  store = answerCard(store, cards[1], 5, now, today);
  assert.equal(store.cards[cards[1].id].repeats, 1);
  assert.equal(store.log.at(-1)?.wasNew, true);
  // The day's single new card is used up and the reviewed one is not due again.
  assert.deepEqual(buildQueue(cards, store, now, today).map((item) => item.line), [5]);

  store = setSuspended(store, cards[0], true);
  assert.deepEqual(buildQueue(cards, store, now, today), []);
  assert.equal(deckStats(cards, store, now).suspended, 1);
  store = setSuspended(store, cards[0], false);
  assert.deepEqual(buildQueue(cards, store, now, today).map((item) => item.line), [5]);

  store = resetCard(store, cards[1]);
  assert.equal(isNew(store, cards[1]), true);
  const { path, front, ...schedule } = scheduleOf(store, cards[1]) as typeof DEFAULT_SCHEDULE & { path: string; front: string };
  assert.deepEqual([path, front, schedule], ['Rust.md', cards[1].front, DEFAULT_SCHEDULE]);
});

test('mastery follows Anki: new, learning under three weeks, mastered after, suspended', () => {
  const now = new Date('2026-11-06T00:00:00.000Z');
  const today = '2026-11-06';
  const cards = parseCards('Rust.md', page);
  let store = emptyReviewStore();
  assert.equal(masteryOf(store, cards[1]), 'new');
  store = answerCard(store, cards[1], 5, now, today);
  assert.equal(masteryOf(store, cards[1]), 'learning');
  store = { ...store, cards: { ...store.cards, [cards[1].id]: { ...store.cards[cards[1].id], lastInterval: 21 } } };
  assert.equal(masteryOf(store, cards[1]), 'mastered');
  // Forgetting a mastered card sends it back to learning.
  store = answerCard(store, cards[1], 1, now, today);
  assert.equal(masteryOf(store, cards[1]), 'learning');
  store = setSuspended(store, cards[1], true);
  assert.equal(masteryOf(store, cards[1]), 'suspended');
});

test('editing an answer flags the card for a re-check', () => {
  const now = new Date('2026-11-01T10:00:00.000Z');
  const before = parseCards('Q.md', '- What is SM-5? #card\n  - An algorithm\n');
  const store = answerCard(emptyReviewStore(), before[0], 5, now, '2026-11-01');
  assert.equal(changedSinceReview(store, before[0]), false);
  const after = parseCards('Q.md', '- What is SM-5? #card\n  - A spaced repetition algorithm\n');
  assert.equal(after[0].id, before[0].id);
  assert.equal(changedSinceReview(store, after[0]), true);
  assert.equal(deckStats(after, store, now).changed, 1);
});

test('cards with the same question are separate cards with separate history', () => {
  const javascript = '- 主要用途是什么？ #card\n  - 浏览器交互\n';
  const sql = '- 主要用途是什么？ #card\n  - 数据查询\n';
  const cards = collectCards([{ path: 'JavaScript.md', content: javascript }, { path: 'SQL.md', content: sql }]);
  assert.deepEqual(cards.map((item) => [item.path, item.back]), [['JavaScript.md', '- 浏览器交互'], ['SQL.md', '- 数据查询']]);
  assert.notEqual(cards[0].id, cards[1].id);
  const now = new Date('2026-11-01T10:00:00.000Z');
  const store = answerCard(emptyReviewStore(), cards[0], 5, now, '2026-11-01');
  // Reviewing the JavaScript card leaves the SQL card due today.
  assert.deepEqual(buildQueue(cards, store, now, '2026-11-02').map((item) => item.path), ['SQL.md']);
  // Deleting the JavaScript page does not hand its history to the SQL card.
  assert.deepEqual(adoptMovedCards([cards[1]], store), store);
  // Within one page, repeated questions get distinct keys.
  const twice = parseCards('Same.md', `${javascript}${javascript}`);
  assert.notEqual(twice[0].id, twice[1].id);
});

test('a moved page keeps the history of its cards', () => {
  const now = new Date('2026-11-01T10:00:00.000Z');
  const [card] = parseCards('Inbox/Rust.md', '- Ownership #card\n  - One owner\n');
  const store = answerCard(emptyReviewStore(), card, 5, now, '2026-11-01');
  const [moved] = parseCards('Areas/Rust.md', '- Ownership #card\n  - One owner\n');
  const adopted = adoptMovedCards([moved], store);
  assert.equal(adopted.cards[moved.id].repeats, 1);
  assert.equal(adopted.cards[moved.id].path, 'Areas/Rust.md');
  assert.equal(adopted.cards[card.id], undefined);
  assert.equal(adopted.log[0].id, moved.id);
  assert.equal(isNew(adopted, moved), false);
});

test('stored history tolerates bad values and earlier formats', () => {
  const now = new Date('2026-11-01T10:00:00.000Z');
  const [card] = parseCards('Q.md', '- Q #card\n  - A\n');
  const store = answerCard({ ...emptyReviewStore(), settings: { newPerDay: 5, reviewsPerDay: 50 } }, card, 5, now, '2026-11-01');
  assert.deepEqual(parseReviewStore(JSON.parse(JSON.stringify(store))), store);
  const matrix: OfMatrix = { 1: { 2.5: 4.1 } };
  assert.deepEqual(parseReviewStore({ version: 2, ofMatrix: { ...matrix, 2: { 2.5: 'x' } } }).ofMatrix, matrix);
  const old = parseReviewStore({ version: 1, cards: { abc: { due: '2026-11-01', interval: 1, ease: 2.5 } }, settings: { newPerDay: 7, reviewsPerDay: 70 } });
  assert.deepEqual(old.cards, {});
  assert.deepEqual(old.settings, { newPerDay: 7, reviewsPerDay: 70 });
  assert.deepEqual(parseReviewStore(null), emptyReviewStore());
});

test('preview intervals follow the saved schedule', () => {
  assert.deepEqual(previewIntervals(DEFAULT_SCHEDULE, {}), { 1: 'Tomorrow', 3: '4d', 5: '4d' });
});

test("decks come from the card's tags, else the page's tags, else its folder", () => {
  const cards = collectCards([
    { path: 'Languages/Japanese.md', content: '- 食べる #card #日语/N2\n  - to eat\n- 飲む #card #日语 #[[Daily words]]\n  - to drink\n- 見る #card\n  - to see\n' },
    { path: 'Biology/Cells.md', content: '---\ntags: [biology, "#exam"]\n---\n- What is a ribosome? #card\n  - It makes proteins\n' },
    { path: 'Logseq page.md', content: 'tags:: history, [[Rome]]\n\n- Year Rome fell? #card\n  - 476\n' },
    { path: 'Loose.md', content: '- Use C#define? #card\n  - No\n' },
  ]);
  const byFront = Object.fromEntries(cards.map((card) => [card.front, card.decks]));
  assert.deepEqual(byFront['食べる'], ['日语/N2']);
  assert.deepEqual(byFront['飲む'], ['日语', 'Daily words']);
  assert.deepEqual(byFront['見る'], ['Languages']);
  assert.deepEqual(byFront['What is a ribosome?'], ['biology', 'exam']);
  assert.deepEqual(byFront['Year Rome fell?'], ['history', 'Rome']);
  // `#` inside a word is not a tag; a root page without tags is Unsorted.
  assert.deepEqual(byFront['Use C#define?'], ['Unsorted']);

  const tree = deckTree(cards);
  assert.deepEqual(tree.map((deck) => deck.path), ['biology', 'Daily words', 'exam', 'history', 'Languages', 'Rome', '日语', 'Unsorted']);
  const japanese = tree.find((deck) => deck.path === '日语')!;
  assert.equal(japanese.cards.length, 2);
  assert.deepEqual(japanese.children.map((deck) => [deck.path, deck.cards.length]), [['日语/N2', 1]]);
});

test('tagging a card keeps its review history', () => {
  const before = parseCards('Words.md', '- 食べる #card\n  - to eat\n');
  const after = parseCards('Words.md', '- 食べる #card #日语\n  - to eat\n');
  assert.equal(after[0].id, before[0].id);
  assert.equal(after[0].front, '食べる');
  // History saved while the tag was part of the question moves to the card.
  const store = emptyReviewStore();
  const legacyId = 'legacy-0';
  store.cards[legacyId] = { ...DEFAULT_SCHEDULE, path: 'Words.md', front: '食べる #日语', backHash: after[0].backHash, repeats: 3 };
  const adopted = adoptMovedCards(after, store);
  assert.equal(adopted.cards[after[0].id]?.repeats, 3);
});
