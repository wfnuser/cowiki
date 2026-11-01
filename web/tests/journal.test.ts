import assert from 'node:assert/strict';
import test from 'node:test';

import {
  DEFAULT_JOURNAL_CONFIG,
  addDays,
  dateKey,
  formatDate,
  journalConfigFromObsidian,
  journalDateFromPath,
  journalEntryKeys,
  journalPagePath,
  monthGrid,
  newJournalBody,
  parseDate,
  renderTemplate,
} from '../src/lib/journal.ts';
import { adjacentEntry, withTitle } from '../src/lib/journal.ts';

const halloween = new Date(2026, 9, 31);

test('formats moment-style tokens used by Obsidian daily notes', () => {
  assert.equal(formatDate(halloween, 'YYYY-MM-DD'), '2026-10-31');
  assert.equal(formatDate(halloween, 'YYYY/MM/YYYY-MM-DD'), '2026/10/2026-10-31');
  assert.equal(formatDate(halloween, 'dddd, MMMM Do, YYYY'), 'Saturday, October 31st, 2026');
  assert.equal(formatDate(halloween, 'YYYY_MM_DD'), '2026_10_31');
  assert.equal(formatDate(halloween, '[Week of] MMM D'), 'Week of Oct 31');
});

test('parses formatted dates back and rejects impossible days', () => {
  assert.equal(dateKey(parseDate('2026-10-31', 'YYYY-MM-DD')!), '2026-10-31');
  assert.equal(dateKey(parseDate('Oct 31st, 2026', 'MMM Do, YYYY')!), '2026-10-31');
  assert.equal(parseDate('2026-02-30', 'YYYY-MM-DD'), null);
  assert.equal(parseDate('notes', 'YYYY-MM-DD'), null);
});

test('journal paths round-trip through folder and nested formats', () => {
  const config = { folder: 'journal', format: 'YYYY/MM/YYYY-MM-DD', template: null };
  const path = journalPagePath(config, halloween);
  assert.equal(path, 'journal/2026/10/2026-10-31.md');
  assert.equal(dateKey(journalDateFromPath(config, path)!), '2026-10-31');
  assert.equal(journalDateFromPath(config, 'projects/2026-10-31.md'), null);
  assert.equal(journalDateFromPath({ ...config, folder: '' }, '2026/10/2026-10-31.md')?.getDate(), 31);
});

test('entry keys are unique and sorted', () => {
  const keys = journalEntryKeys(DEFAULT_JOURNAL_CONFIG, [
    'journal/2026-10-31.md',
    'journal/2026-10-01.md',
    'journal/README.md',
    'notes/2026-10-02.md',
  ]);
  assert.deepEqual(keys, ['2026-10-01', '2026-10-31']);
});

test('date arithmetic stays on local calendar days across month ends', () => {
  assert.equal(dateKey(addDays(halloween, 1)), '2026-11-01');
  assert.equal(dateKey(addDays(new Date(2026, 2, 1), -1)), '2026-02-28');
});

test('renders Obsidian core template variables and leaves others alone', () => {
  const rendered = renderTemplate(
    '# {{title}}\nCreated {{date:YYYY/MM/DD}} at {{time}}\n<% tp.date.now() %>',
    halloween,
    '2026-10-31',
    new Date(2026, 9, 31, 8, 5),
  );
  assert.equal(rendered, '# 2026-10-31\nCreated 2026/10/31 at 08:05\n<% tp.date.now() %>');
});

test('default journal body has a dated title and plan/notes/review sections', () => {
  const body = newJournalBody(DEFAULT_JOURNAL_CONFIG, halloween, null);
  assert.match(body, /^---\ntype: Journal\ntitle: "2026-10-31"\ndate: 2026-10-31\n---/);
  assert.match(body, /# Saturday, October 31, 2026/);
  assert.match(body, /## Plan[\s\S]*## Notes[\s\S]*## Review/);
  assert.equal(newJournalBody(DEFAULT_JOURNAL_CONFIG, halloween, 'Day {{date}}'), 'Day 2026-10-31');
});

test('month grid marks today and existing entries', () => {
  const grid = monthGrid(halloween, halloween, new Set(['2026-10-01']));
  assert.equal(grid.length, 6);
  const cells = grid.flat();
  assert.equal(cells.find((cell) => cell.isToday)?.key, '2026-10-31');
  assert.equal(cells.find((cell) => cell.key === '2026-10-01')?.hasEntry, true);
  assert.equal(grid[0][0].key, '2026-09-28', 'weeks start on Monday');
});

test('reads Obsidian daily-notes and periodic-notes settings', () => {
  assert.deepEqual(
    journalConfigFromObsidian({ folder: 'Daily/', format: 'YYYY-MM-DD', template: 'Templates/Daily' }),
    { folder: 'Daily', format: 'YYYY-MM-DD', template: 'Templates/Daily.md' },
  );
  assert.deepEqual(
    journalConfigFromObsidian({ daily: { folder: 'Journal', format: 'YYYY/MM/DD' } }),
    { folder: 'Journal', format: 'YYYY/MM/DD', template: null },
  );
  assert.deepEqual(journalConfigFromObsidian({}), { folder: '', format: 'YYYY-MM-DD', template: null });
  assert.equal(journalConfigFromObsidian(null), null);
});

test('a journal template never keeps its own title', () => {
  assert.equal(withTitle('---\ntype: Note\ntitle: Daily\n---\n\n# Day\n', '2026-11-01'), '---\ntype: Note\ntitle: "2026-11-01"\n---\n\n# Day\n');
  assert.equal(withTitle('# No frontmatter\n', 'x'), '# No frontmatter\n');
});

test('previous/next step through existing entries only', () => {
  const keys = ['2026-03-31', '2026-04-06', '2026-04-14', '2026-11-01'];
  assert.equal(adjacentEntry(keys, '2026-04-14', -1), '2026-04-06');
  assert.equal(adjacentEntry(keys, '2026-04-14', 1), '2026-11-01');
  assert.equal(adjacentEntry(keys, '2026-04-10', 1), '2026-04-14');
  assert.equal(adjacentEntry(keys, '2026-03-31', -1), null);
  assert.equal(adjacentEntry(keys, '2026-11-01', 1), null);
});
