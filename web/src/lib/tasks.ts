/**
 * Wiki-native tasks. A task is a Markdown list item that already lives in a
 * page (journal, project page, meeting note). Views only read and rewrite that
 * one line, so every view shows the same record and there is no second copy.
 *
 * Supported syntaxes:
 * - Markdown/Obsidian checkboxes: `- [ ]`, `- [x]`, `- [/]` (doing), `- [-]` (cancelled)
 * - Obsidian Tasks metadata: 📅 due, ⏳ scheduled, 🛫 start, ✅ done, ❌ cancelled,
 *   priorities 🔺 ⏫ 🔼 🔽 ⏬
 * - Dataview inline fields: `[due:: 2026-11-01]`, `(scheduled:: …)`
 * - Logseq markers: `- TODO`, `- DOING`, `- NOW`, `- LATER`, `- DONE`, `- CANCELED`
 */

export type TaskStatus = 'todo' | 'doing' | 'done' | 'cancelled';
export type TaskPriority = 'highest' | 'high' | 'medium' | 'low' | 'lowest';
export type TaskSyntax = 'checkbox' | 'logseq';

export interface TaskItem {
  /** Repo-relative path of the page that owns the task. */
  path: string;
  /** Zero-based line number in the file at the time it was read. */
  line: number;
  /** Exact original line, used to detect concurrent edits before rewriting. */
  raw: string;
  /** Display text without list markers and metadata. */
  text: string;
  status: TaskStatus;
  syntax: TaskSyntax;
  due: string | null;
  scheduled: string | null;
  start: string | null;
  doneDate: string | null;
  priority: TaskPriority | null;
  tags: string[];
  /** Day of the journal entry that contains the task, if any. */
  journalDate: string | null;
}

const CHECKBOX = /^(\s*(?:[-*+]|\d+[.)])\s+)\[([ xX/\-])\]\s?(.*)$/;
const LOGSEQ = /^(\s*[-*+]\s+)(TODO|DOING|NOW|LATER|DONE|CANCELED|CANCELLED)\s+(.*)$/;
const DATE = '(\\d{4}-\\d{2}-\\d{2})';
const EMOJI_FIELDS: Array<[keyof Pick<TaskItem, 'due' | 'scheduled' | 'start' | 'doneDate'>, string]> = [
  ['due', '📅'],
  ['scheduled', '⏳'],
  ['start', '🛫'],
  ['doneDate', '✅'],
];
const PRIORITIES: Array<[TaskPriority, string]> = [
  ['highest', '🔺'],
  ['high', '⏫'],
  ['medium', '🔼'],
  ['low', '🔽'],
  ['lowest', '⏬'],
];
const INLINE_FIELD = /[[(]\s*(due|scheduled|start|completion|done)\s*::\s*(\d{4}-\d{2}-\d{2})\s*[\])]/gi;
const TAG = /(^|\s)#([\p{L}\p{N}_/-]+)/gu;

function emojiDate(text: string, emoji: string): string | null {
  const match = new RegExp(`${emoji}\\uFE0F?\\s*${DATE}`, 'u').exec(text);
  return match ? match[1] : null;
}

function stripMetadata(text: string): string {
  let result = text;
  for (const [, emoji] of EMOJI_FIELDS) {
    result = result.replace(new RegExp(`${emoji}\\uFE0F?\\s*${DATE}`, 'gu'), '');
  }
  result = result.replace(new RegExp(`(?:➕|❌)\\uFE0F?\\s*${DATE}`, 'gu'), '');
  for (const [, emoji] of PRIORITIES) {
    result = result.replaceAll(emoji, '');
  }
  result = result.replace(/🔁️?\s*[^📅⏳🛫✅➕❌]*/gu, '');
  result = result.replace(INLINE_FIELD, '');
  return result.replace(/\s{2,}/g, ' ').trim();
}

/** Parse one line; returns null when it is not a task. */
export function parseTaskLine(line: string): Omit<TaskItem, 'path' | 'line' | 'journalDate'> | null {
  let status: TaskStatus;
  let syntax: TaskSyntax;
  let rest: string;
  const checkbox = CHECKBOX.exec(line);
  if (checkbox) {
    syntax = 'checkbox';
    rest = checkbox[3];
    switch (checkbox[2]) {
      case 'x': case 'X': status = 'done'; break;
      case '/': status = 'doing'; break;
      case '-': status = 'cancelled'; break;
      default: status = 'todo';
    }
  } else {
    const logseq = LOGSEQ.exec(line);
    if (!logseq) return null;
    syntax = 'logseq';
    rest = logseq[3];
    switch (logseq[2]) {
      case 'DONE': status = 'done'; break;
      case 'DOING': case 'NOW': status = 'doing'; break;
      case 'CANCELED': case 'CANCELLED': status = 'cancelled'; break;
      default: status = 'todo';
    }
  }
  const fields: Record<'due' | 'scheduled' | 'start' | 'doneDate', string | null> = {
    due: null, scheduled: null, start: null, doneDate: null,
  };
  for (const [field, emoji] of EMOJI_FIELDS) {
    fields[field] = emojiDate(rest, emoji);
  }
  for (const match of rest.matchAll(INLINE_FIELD)) {
    const key = match[1].toLowerCase();
    const value = match[2];
    if (key === 'due' && !fields.due) fields.due = value;
    if (key === 'scheduled' && !fields.scheduled) fields.scheduled = value;
    if (key === 'start' && !fields.start) fields.start = value;
    if ((key === 'completion' || key === 'done') && !fields.doneDate) fields.doneDate = value;
  }
  const priority = PRIORITIES.find(([, emoji]) => rest.includes(emoji))?.[0] ?? null;
  const tags = [...rest.matchAll(TAG)].map((match) => match[2]);
  const text = stripMetadata(rest);
  if (!text && syntax === 'checkbox' && status === 'todo') return null;
  return {
    raw: line,
    text,
    status,
    syntax,
    due: fields.due,
    scheduled: fields.scheduled,
    start: fields.start,
    doneDate: fields.doneDate,
    priority,
    tags,
  };
}

/** Line ranges that are fenced code or the leading frontmatter block. */
function ignoredLines(lines: string[]): Set<number> {
  const ignored = new Set<number>();
  let index = 0;
  if (lines[0]?.trim() === '---') {
    ignored.add(0);
    for (index = 1; index < lines.length; index += 1) {
      ignored.add(index);
      if (lines[index].trim() === '---' || lines[index].trim() === '...') {
        index += 1;
        break;
      }
    }
  }
  let fence: string | null = null;
  for (; index < lines.length; index += 1) {
    const trimmed = lines[index].trimStart();
    const marker = /^(`{3,}|~{3,})/.exec(trimmed)?.[1];
    if (fence) {
      ignored.add(index);
      if (marker && marker[0] === fence[0] && marker.length >= fence.length) fence = null;
    } else if (marker) {
      fence = marker;
      ignored.add(index);
    }
  }
  return ignored;
}

/** Extract every task from a page's full file content. */
export function extractTasks(path: string, content: string, journalDate: string | null = null): TaskItem[] {
  const lines = content.split(/\r?\n/);
  const ignored = ignoredLines(lines);
  const tasks: TaskItem[] = [];
  lines.forEach((line, index) => {
    if (ignored.has(index)) return;
    const parsed = parseTaskLine(line);
    if (parsed) tasks.push({ ...parsed, path, line: index, journalDate });
  });
  return tasks;
}

/** Return the line with its completion state flipped. */
export function toggledTaskLine(line: string, today: string): string {
  const checkbox = CHECKBOX.exec(line);
  if (checkbox) {
    const done = checkbox[2] === 'x' || checkbox[2] === 'X';
    if (done) {
      const reopened = checkbox[3].replace(new RegExp(`\\s*✅\\uFE0F?\\s*${DATE}`, 'u'), '');
      return `${checkbox[1]}[ ] ${reopened}`.replace(/\s+$/, '');
    }
    const body = checkbox[3].replace(/\s+$/, '');
    const stamp = new RegExp(`✅\\uFE0F?\\s*${DATE}`, 'u').test(body) ? '' : ` ✅ ${today}`;
    return `${checkbox[1]}[x] ${body}${stamp}`;
  }
  const logseq = LOGSEQ.exec(line);
  if (logseq) {
    const done = logseq[2] === 'DONE';
    return `${logseq[1]}${done ? 'TODO' : 'DONE'} ${logseq[3]}`;
  }
  throw new Error('This line is no longer a task.');
}

/**
 * Apply a toggle to the page content. The task must still be at its recorded
 * line; if the page changed, the unique identical line elsewhere is used.
 * Anything else is reported instead of guessing.
 */
export function applyTaskToggle(content: string, task: Pick<TaskItem, 'line' | 'raw'>, today: string): string {
  const newline = content.includes('\r\n') ? '\r\n' : '\n';
  const lines = content.split(/\r?\n/);
  let index = lines[task.line] === task.raw ? task.line : -1;
  if (index < 0) {
    const matches = lines.flatMap((line, position) => (line === task.raw ? [position] : []));
    if (matches.length !== 1) {
      throw new Error('This task changed in its page. Reload the task list and try again.');
    }
    index = matches[0];
  }
  lines[index] = toggledTaskLine(lines[index], today);
  return lines.join(newline);
}

/** Markdown for a new task, using Obsidian Tasks' due-date marker. */
export function newTaskLine(text: string, due: string | null = null): string {
  const clean = text.replace(/\s+/g, ' ').trim();
  if (!clean) throw new Error('Task text is empty.');
  return `- [ ] ${clean}${due ? ` 📅 ${due}` : ''}`;
}

/**
 * Insert a task line into a page: under the first `## Plan` / `## Tasks` /
 * `## Todo` heading when present (after its last list item), otherwise at the
 * end of the page.
 */
export function insertTaskLine(content: string, taskLine: string): string {
  const newline = content.includes('\r\n') ? '\r\n' : '\n';
  const lines = content.split(/\r?\n/);
  const headingIndex = lines.findIndex((line) => /^#{2,3}\s+(plan|tasks?|todo|to-do|计划|任务|待办)\s*$/i.test(line.trim()));
  if (headingIndex >= 0) {
    let insertAt = headingIndex + 1;
    let index = headingIndex + 1;
    while (index < lines.length && !/^#{1,6}\s/.test(lines[index])) {
      if (lines[index].trim()) insertAt = index + 1;
      index += 1;
    }
    if (insertAt === headingIndex + 1) {
      lines.splice(insertAt, 0, '', taskLine);
    } else {
      lines.splice(insertAt, 0, taskLine);
    }
    return lines.join(newline);
  }
  const trimmed = content.replace(/\s+$/, '');
  return `${trimmed}${trimmed ? `${newline}${newline}` : ''}${taskLine}${newline}`;
}

/** The day a task is "for": its due, scheduled or start date, else its journal day. */
export function taskDay(task: TaskItem): string | null {
  return task.scheduled ?? task.due ?? task.start ?? task.journalDate;
}

export interface TaskGroups {
  overdue: TaskItem[];
  today: TaskItem[];
  upcoming: TaskItem[];
  someday: TaskItem[];
  done: TaskItem[];
}

const PRIORITY_RANK: Record<TaskPriority, number> = { highest: 0, high: 1, medium: 2, low: 3, lowest: 4 };

function compareTasks(left: TaskItem, right: TaskItem): number {
  const leftDay = taskDay(left) ?? '9999-99-99';
  const rightDay = taskDay(right) ?? '9999-99-99';
  if (leftDay !== rightDay) return leftDay < rightDay ? -1 : 1;
  const leftPriority = left.priority ? PRIORITY_RANK[left.priority] : 2;
  const rightPriority = right.priority ? PRIORITY_RANK[right.priority] : 2;
  if (leftPriority !== rightPriority) return leftPriority - rightPriority;
  if (left.path !== right.path) return left.path.localeCompare(right.path);
  return left.line - right.line;
}

/**
 * Group open tasks by day relative to `today`. A task whose due date passed is
 * overdue; a journal task without dates belongs to its journal day, so an
 * unfinished item from yesterday's plan shows up as overdue today.
 */
export function groupTasks(tasks: TaskItem[], today: string, doneLimit = 50): TaskGroups {
  const groups: TaskGroups = { overdue: [], today: [], upcoming: [], someday: [], done: [] };
  for (const task of tasks) {
    if (task.status === 'done' || task.status === 'cancelled') {
      groups.done.push(task);
      continue;
    }
    if (task.due && task.due < today) {
      groups.overdue.push(task);
      continue;
    }
    const day = taskDay(task);
    if (!day) groups.someday.push(task);
    else if (day < today) groups.overdue.push(task);
    else if (day === today) groups.today.push(task);
    else groups.upcoming.push(task);
  }
  groups.overdue.sort(compareTasks);
  groups.today.sort(compareTasks);
  groups.upcoming.sort(compareTasks);
  groups.someday.sort(compareTasks);
  groups.done.sort((left, right) => {
    const leftDay = left.doneDate ?? taskDay(left) ?? '';
    const rightDay = right.doneDate ?? taskDay(right) ?? '';
    return leftDay === rightDay ? compareTasks(left, right) : (leftDay < rightDay ? 1 : -1);
  });
  groups.done = groups.done.slice(0, doneLimit);
  return groups;
}

/** Stable identity of a task across reloads (path + original line text). */
export function taskKey(task: Pick<TaskItem, 'path' | 'line' | 'raw'>): string {
  return `${task.path}\u0000${task.line}\u0000${task.raw}`;
}
