import assert from 'node:assert/strict';
import test from 'node:test';

import {
  applyTaskStatus,
  applyTaskToggle,
  extractTasks,
  groupTasks,
  insertTaskLine,
  newTaskLine,
  parseTaskLine,
  taskLineWithStatus,
  toggledTaskLine,
  weekBoard,
} from '../src/lib/tasks.ts';

test('parses checkbox tasks with Obsidian Tasks metadata', () => {
  const task = parseTaskLine('- [ ] Write proposal ⏫ 📅 2026-10-30 ⏳ 2026-10-29 #course #dsaa');
  assert.ok(task);
  assert.equal(task.status, 'todo');
  assert.equal(task.text, 'Write proposal #course #dsaa');
  assert.equal(task.due, '2026-10-30');
  assert.equal(task.scheduled, '2026-10-29');
  assert.equal(task.priority, 'high');
  assert.deepEqual(task.tags, ['course', 'dsaa']);
});

test('parses done, doing, cancelled and Dataview fields', () => {
  assert.equal(parseTaskLine('* [x] Ship it ✅ 2026-10-31')?.doneDate, '2026-10-31');
  assert.equal(parseTaskLine('  - [/] In flight')?.status, 'doing');
  assert.equal(parseTaskLine('1. [-] Dropped')?.status, 'cancelled');
  const dataview = parseTaskLine('- [ ] Review notes [due:: 2026-11-02]');
  assert.equal(dataview?.due, '2026-11-02');
  assert.equal(dataview?.text, 'Review notes');
});

test('parses Logseq markers', () => {
  assert.equal(parseTaskLine('- TODO Read chapter 3')?.status, 'todo');
  assert.equal(parseTaskLine('- DONE Read chapter 2')?.status, 'done');
  assert.equal(parseTaskLine('- NOW Writing')?.status, 'doing');
  assert.equal(parseTaskLine('- TODO')?.syntax, undefined);
});

test('reads Logseq priorities and SCHEDULED / DEADLINE lines', () => {
  const [task, other] = extractTasks('Plan.md', [
    '- TODO [#A] Write report #work',
    '  SCHEDULED: <2026-11-03 Tue>',
    '  DEADLINE: <2026-11-05 Thu 18:00>',
    '- LATER Read paper',
    '  - DEADLINE: <2026-12-01 Tue> is just text in a child',
  ].join('\n'));
  assert.equal(task.text, 'Write report #work');
  assert.equal(task.priority, 'high');
  assert.deepEqual(task.tags, ['work']);
  assert.equal(task.scheduled, '2026-11-03');
  assert.equal(task.due, '2026-11-05');
  assert.equal(other.due, null);
  // Obsidian Tasks lines keep using their own markers.
  assert.equal(parseTaskLine('- [ ] Plain [#A] text')?.priority, null);
});

test('ignores non-tasks, empty checkboxes, frontmatter and code fences', () => {
  assert.equal(parseTaskLine('- plain item'), null);
  assert.equal(parseTaskLine('- [ ] '), null);
  const content = [
    '---',
    'title: "- [ ] not a task"',
    '---',
    '- [ ] real task',
    '```md',
    '- [ ] example in code',
    '```',
    '~~~',
    '- [x] also code',
    '~~~',
    '- [x] finished',
  ].join('\n');
  const tasks = extractTasks('page.md', content, '2026-10-31');
  assert.deepEqual(tasks.map((task) => [task.line, task.text, task.journalDate]), [
    [3, 'real task', '2026-10-31'],
    [10, 'finished', '2026-10-31'],
  ]);
});

test('toggling stamps and removes the completion date', () => {
  const done = toggledTaskLine('- [ ] Pay rent 📅 2026-11-01', '2026-10-31');
  assert.equal(done, '- [x] Pay rent 📅 2026-11-01 ✅ 2026-10-31');
  assert.equal(toggledTaskLine(done, '2026-11-02'), '- [ ] Pay rent 📅 2026-11-01');
  assert.equal(toggledTaskLine('- TODO Call', '2026-10-31'), '- DONE Call');
  assert.equal(toggledTaskLine('- DONE Call', '2026-10-31'), '- TODO Call');
  assert.throws(() => toggledTaskLine('just text', '2026-10-31'));
});

test('applying a toggle rewrites only the task line and detects edits', () => {
  const content = '# Day\r\n\r\n- [ ] one\r\n- [ ] two\r\n';
  const [, two] = extractTasks('day.md', content);
  const updated = applyTaskToggle(content, two, '2026-10-31');
  assert.equal(updated, '# Day\r\n\r\n- [ ] one\r\n- [x] two ✅ 2026-10-31\r\n');

  const shifted = `intro\n${content.replaceAll('\r\n', '\n')}`;
  assert.match(applyTaskToggle(shifted, two, '2026-10-31'), /- \[x\] two/);

  const duplicated = '- [ ] same\n- [ ] same\n';
  assert.throws(() => applyTaskToggle(duplicated, { line: 5, raw: '- [ ] same' }, '2026-10-31'));
  assert.throws(() => applyTaskToggle('- [ ] other', two, '2026-10-31'));
});

test('new tasks use the Tasks due marker and land in the Plan section', () => {
  assert.equal(newTaskLine('  Buy   milk ', '2026-11-01'), '- [ ] Buy milk 📅 2026-11-01');
  assert.throws(() => newTaskLine('   '));
  const page = '# Day\n\n## Plan\n\n- [ ] first\n\n## Notes\n\nhello\n';
  assert.equal(
    insertTaskLine(page, '- [ ] second'),
    '# Day\n\n## Plan\n\n- [ ] first\n- [ ] second\n\n## Notes\n\nhello\n',
  );
  assert.equal(insertTaskLine('# Day\n\n## Plan\n\n## Notes\n', '- [ ] a'), '# Day\n\n## Plan\n\n- [ ] a\n\n## Notes\n');
  assert.equal(insertTaskLine('# Notes', '- [ ] a'), '# Notes\n\n- [ ] a\n');
});

test('groups open tasks by day and keeps journal tasks on their day', () => {
  const tasks = [
    ...extractTasks('journal/2026-10-30.md', '- [ ] leftover from yesterday', '2026-10-30'),
    ...extractTasks('journal/2026-10-31.md', '- [ ] today plan\n- [x] done today ✅ 2026-10-31', '2026-10-31'),
    ...extractTasks('projects/a.md', '- [ ] due tomorrow 📅 2026-11-01\n- [ ] someday idea\n- [ ] late 📅 2026-10-01'),
  ];
  const groups = groupTasks(tasks, '2026-10-31');
  assert.deepEqual(groups.overdue.map((task) => task.text), ['late', 'leftover from yesterday']);
  assert.deepEqual(groups.today.map((task) => task.text), ['today plan']);
  assert.deepEqual(groups.upcoming.map((task) => task.text), ['due tomorrow']);
  assert.deepEqual(groups.someday.map((task) => task.text), ['someday idea']);
  assert.deepEqual(groups.done.map((task) => task.text), ['done today']);
});

test('cancelled tasks reopen instead of being marked done', () => {
  assert.equal(toggledTaskLine('- [-] Dropped idea', '2026-10-31'), '- [ ] Dropped idea');
  assert.equal(toggledTaskLine('- CANCELLED Old plan', '2026-10-31'), '- TODO Old plan');
  assert.equal(toggledTaskLine('- [/] Drafting', '2026-10-31'), '- [x] Drafting ✅ 2026-10-31');
});

test('board moves set the status on the task line', () => {
  assert.equal(taskLineWithStatus('- [ ] Draft 📅 2026-11-02', 'doing', '2026-11-01'), '- [/] Draft 📅 2026-11-02');
  assert.equal(taskLineWithStatus('- [/] Draft', 'done', '2026-11-01'), '- [x] Draft ✅ 2026-11-01');
  assert.equal(taskLineWithStatus('- [x] Draft ✅ 2026-10-30', 'done', '2026-11-01'), '- [x] Draft ✅ 2026-10-30');
  assert.equal(taskLineWithStatus('- [x] Draft ✅ 2026-10-30', 'todo', '2026-11-01'), '- [ ] Draft');
  assert.equal(taskLineWithStatus('- [-] ~~Dropped~~', 'doing', '2026-11-01'), '- [/] Dropped');
  assert.equal(taskLineWithStatus('  - TODO Call Ana', 'doing', '2026-11-01'), '  - DOING Call Ana');
  assert.equal(taskLineWithStatus('- DOING Call Ana', 'done', '2026-11-01'), '- DONE Call Ana');
  assert.throws(() => taskLineWithStatus('plain text', 'done', '2026-11-01'), /no longer a task/);
  const page = '# Plan\n\n- [ ] One\n- [ ] Two\n';
  const [, two] = extractTasks('Plan.md', page);
  assert.equal(applyTaskStatus(page, two, 'doing', '2026-11-01'), '# Plan\n\n- [ ] One\n- [/] Two\n');
});

test('the week board collects open, in-progress and finished tasks', () => {
  const tasks = extractTasks('Week.md', [
    '- [ ] Overdue 📅 2026-10-20',
    '- [ ] This week 📅 2026-10-28',
    '- [ ] Next week 📅 2026-11-03',
    '- [ ] No date',
    '- [/] Doing no date',
    '- [/] Doing later 📅 2026-11-20',
    '- [x] Done this week ✅ 2026-10-27',
    '- [x] Done before ✅ 2026-10-19',
    '- [-] Cancelled 📅 2026-10-28',
  ].join('\n'));
  const board = weekBoard(tasks, '2026-10-26', '2026-11-01');
  assert.deepEqual(board.todo.map((task) => task.text), ['Overdue', 'This week']);
  assert.deepEqual(board.doing.map((task) => task.text), ['Doing no date']);
  assert.deepEqual(board.done.map((task) => task.text), ['Done this week']);
});
