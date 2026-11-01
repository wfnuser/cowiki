import assert from 'node:assert/strict';
import test from 'node:test';

import {
  DEFAULT_SCHEDULE,
  buildQueue,
  collectCards,
  deckStats,
  isDue,
  parseCards,
  parseOfMatrix,
  previewIntervals,
  renderCloze,
  review,
  writeSchedule,
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

test('Logseq #card blocks: question, children as answer, properties as schedule', () => {
  const cards = parseCards('Rust.md', page);
  assert.deepEqual(cards.map((item) => [item.line, item.front, item.back, item.cloze]), [
    [5, 'What does the borrow checker enforce?', '- Ownership rules\n  - at compile time', false],
    [15, 'Rust has {{cloze zero-cost}} abstractions', '', true],
    [17, 'Nested question', '- Nested answer', false],
  ]);
  assert.deepEqual(cards[0].schedule, {
    lastInterval: 4, repeats: 1, ease: 2.6,
    nextSchedule: '2026-11-05T02:00:00.000Z', lastReviewed: '2026-11-01T02:00:00.000Z', lastScore: 5,
  });
  assert.deepEqual(cards[1].schedule, DEFAULT_SCHEDULE);
  assert.equal(collectCards([{ path: 'A.md', content: page }, { path: 'B.md', content: '- Q #card\n  - A\n' }]).length, 4);
});

test('cloze text is hidden on the question and highlighted on the answer', () => {
  assert.equal(renderCloze('Rust has {{cloze zero-cost}} abstractions', false), 'Rust has [...] abstractions');
  assert.equal(renderCloze('Rust has {{cloze: zero-cost}} abstractions', true), 'Rust has ==zero-cost== abstractions');
});

test('SM-5 scheduling matches Logseq', () => {
  const now = new Date('2026-11-01T10:00:00.000Z');
  let matrix: OfMatrix = {};
  let first = review(DEFAULT_SCHEDULE, 5, matrix, now);
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
  first = { schedule: DEFAULT_SCHEDULE, matrix: {} };
  assert.deepEqual(previewIntervals(first.schedule, first.matrix), { 1: 'Tomorrow', 3: '4d', 5: '4d' });
});

test('due cards follow Logseq: unanswered, unscheduled or past their time', () => {
  const now = new Date('2026-11-06T00:00:00.000Z');
  const cards = parseCards('Rust.md', page);
  assert.equal(isDue(cards[0], now), true);
  assert.equal(isDue(cards[0], new Date('2026-11-04T00:00:00.000Z')), false);
  assert.equal(isDue(cards[1], now), true);
  assert.deepEqual(deckStats(cards, new Date('2026-11-04T00:00:00.000Z')), { total: 3, due: 2, fresh: 2 });
  assert.deepEqual(buildQueue(cards, now).map((item) => item.line), [15, 17, 5]);
});

test('a review is written back as properties under the question', () => {
  const [card] = parseCards('Rust.md', page);
  const now = new Date('2026-11-05T03:00:00.000Z');
  const next = review(card.schedule, 5, {}, now).schedule;
  // A line added above the card since it was loaded must not break the write.
  const edited = page.replace('# Rust\n', '# Rust\n\nIntro line\n');
  const written = writeSchedule(edited, card, next);
  const lines = written.split('\n');
  const at = lines.indexOf('- What does the borrow checker enforce? #card');
  assert.deepEqual(lines.slice(at, at + 10), [
    '- What does the borrow checker enforce? #card',
    '  id:: 6540a1b2',
    '  card-last-interval:: 4',
    '  card-repeats:: 2',
    '  card-ease-factor:: 2.7',
    '  card-next-schedule:: 2026-11-09T03:00:00.000Z',
    '  card-last-reviewed:: 2026-11-05T03:00:00.000Z',
    '  card-last-score:: 5',
    '  - Ownership rules',
    '    - at compile time',
  ]);
  assert.equal(parseCards('Rust.md', written)[0].schedule.repeats, 2);

  const nested = parseCards('Rust.md', page)[2];
  const nestedWritten = writeSchedule(page, nested, review(nested.schedule, 3, {}, now).schedule).split('\n');
  assert.equal(nestedWritten[18], '\t  card-last-interval:: 4');
  assert.equal(nestedWritten[24], '\t\t- Nested answer');

  const reset = writeSchedule(written, card, null).split('\n');
  assert.deepEqual(reset.slice(at + 1, at + 6), [
    '  id:: 6540a1b2',
    '  card-last-interval:: -1',
    '  card-repeats:: 0',
    '  card-ease-factor:: 2.5',
    '  - Ownership rules',
  ]);
  assert.throws(() => writeSchedule('# Empty\n', card, next), /changed or removed/);
});

test('the learned factor matrix tolerates bad stored values', () => {
  assert.deepEqual(parseOfMatrix({ ofMatrix: { 1: { 2.5: 4.1, 2.6: 'x' } } }), { 1: { 2.5: 4.1 } });
  assert.deepEqual(parseOfMatrix({ version: 1, cards: {} }), {});
  assert.deepEqual(parseOfMatrix(null), {});
});

test('cards with the same question are separate cards with separate history', () => {
  const javascript = '- 主要用途是什么？ #card\n  - 浏览器交互\n';
  const sql = '- 主要用途是什么？ #card\n  - 数据查询\n';
  const cards = collectCards([{ path: 'JavaScript.md', content: javascript }, { path: 'SQL.md', content: sql }]);
  assert.deepEqual(cards.map((item) => [item.path, item.back]), [['JavaScript.md', '- 浏览器交互'], ['SQL.md', '- 数据查询']]);
  assert.notEqual(cards[0].id, cards[1].id);
  const now = new Date('2026-11-01T10:00:00.000Z');
  const reviewed = writeSchedule(javascript, cards[0], review(cards[0].schedule, 5, {}, now).schedule);
  // Reviewing the JavaScript card leaves the SQL card due today.
  const after = collectCards([{ path: 'JavaScript.md', content: reviewed }, { path: 'SQL.md', content: sql }]);
  assert.deepEqual(buildQueue(after, now).map((item) => item.path), ['SQL.md']);
  // Within one page, repeated questions get distinct keys and are written separately.
  const twice = parseCards('Same.md', `${javascript}${javascript}`);
  assert.notEqual(twice[0].id, twice[1].id);
  const second = writeSchedule(`${javascript}${javascript}`, twice[1], review(twice[1].schedule, 5, {}, now).schedule);
  assert.deepEqual(parseCards('Same.md', second).map((item) => item.schedule.repeats), [0, 1]);
});
