import assert from 'node:assert/strict';
import test from 'node:test';

import { formatDate, isoWeek, isoWeekStart, journalDateFromPath, parseDate } from '../src/lib/journal.ts';
import { extractTasks } from '../src/lib/tasks.ts';
import {
  DEFAULT_REVIEW_CONFIG,
  newReviewBody,
  reviewPagePath,
  reviewWeekFromPath,
  weekLabel,
} from '../src/lib/weekly-review.ts';

test('ISO weeks format and parse like Obsidian Periodic Notes', () => {
  assert.deepEqual(isoWeek(new Date(2026, 9, 31)), { year: 2026, week: 44 });
  assert.deepEqual(isoWeek(new Date(2027, 0, 1)), { year: 2026, week: 53 });
  assert.equal(formatDate(new Date(2026, 9, 31), 'GGGG-[W]WW'), '2026-W44');
  assert.equal(formatDate(new Date(2026, 9, 31), 'gggg-[W]ww'), '2026-W44');
  assert.equal(formatDate(new Date(2026, 9, 31), 'YYYY-MM-DD'), '2026-10-31');
  assert.equal(parseDate('2026-W44', 'GGGG-[W]WW')?.toDateString(), new Date(2026, 9, 26).toDateString());
  assert.equal(isoWeekStart(2026, 54), null);
  assert.equal(isoWeekStart(2025, 53), null);
});

test('review pages are named by week and recognised from their path', () => {
  const path = reviewPagePath(DEFAULT_REVIEW_CONFIG, new Date(2026, 9, 31));
  assert.equal(path, 'journal/2026-W44.md');
  assert.equal(reviewWeekFromPath(DEFAULT_REVIEW_CONFIG, path)?.toDateString(), new Date(2026, 9, 26).toDateString());
  assert.equal(reviewWeekFromPath(DEFAULT_REVIEW_CONFIG, 'journal/2026-10-31.md'), null);
  assert.equal(reviewWeekFromPath(DEFAULT_REVIEW_CONFIG, 'Projects/2026-W44.md'), null);
  // A daily journal format does not mistake a week page for a day.
  assert.equal(journalDateFromPath({ folder: 'journal', format: 'YYYY-MM-DD', template: null }, path), null);
  assert.equal(weekLabel(new Date(2026, 9, 26)), '2026-W44');
});

test('a new review summarises the week without duplicating tasks', () => {
  const tasks = [
    ...extractTasks('Projects/CoWiki.md', [
      '- [x] Ship journal ✅ 2026-10-28',
      '- [x] Old work ✅ 2026-10-10',
      '- [ ] Reply to reviewers 📅 2026-10-20',
      '- [ ] Write report 📅 2026-10-30',
      '- [ ] Later 📅 2026-11-20',
      '- [ ] Someday',
    ].join('\n')),
  ];
  const body = newReviewBody({
    date: new Date(2026, 9, 31),
    tasks,
    journal: [
      { key: '2026-10-27', path: 'journal/2026-10-27.md' },
      { key: '2026-10-19', path: 'journal/2026-10-19.md' },
    ],
  });
  assert.match(body, /^---\ntype: Review\ntitle: "2026-W44"\nweek: "2026-W44"\n---\n\n# Week 44 · Oct 26 – Nov 1, 2026\n/);
  assert.match(body, /## Done this week\n\n- Ship journal · \[\[Projects\/CoWiki\|CoWiki\]\]\n\n## Still open/);
  assert.match(body, /- Reply to reviewers \(overdue since 2026-10-20\)/);
  assert.match(body, /- Write report \(due 2026-10-30\)/);
  assert.doesNotMatch(body, /Later|Someday|Old work/);
  assert.match(body, /## Journal\n\n\[\[journal\/2026-10-27\|Tue 27\]\]\n/);
  assert.doesNotMatch(body, /- \[ \]/);
  assert.equal(extractTasks('journal/2026-W44.md', body).length, 0);
});

test('a template replaces the layout but keeps the summary', () => {
  const body = newReviewBody({ date: new Date(2026, 9, 31), tasks: [], journal: [], template: '# {{title}}\n\n## Wins\n' });
  assert.match(body, /^# 2026-W44\n\n## Wins\n\n## Done this week\n\n- Nothing checked off yet\./);
});
