/**
 * Weekly review pages. A review is an ordinary wiki page named after its ISO
 * week (Obsidian Periodic Notes uses the same convention), created once with a
 * snapshot of the week: tasks finished, tasks still open, and the journal
 * entries written. The reflection sections are left for the person to write.
 */

import {
  addDays,
  dateKey,
  formatDate,
  isoWeek,
  journalDateFromPath,
  renderTemplate,
  startOfIsoWeek,
} from './journal.ts';
import { taskDay, type TaskItem } from './tasks.ts';

export interface ReviewConfig {
  /** Folder that holds review pages, relative to the Space root. */
  folder: string;
  /** moment-style format; week tokens GGGG/WW follow ISO weeks. */
  format: string;
  /** Optional template page used for new reviews. */
  template: string | null;
}

export const DEFAULT_REVIEW_CONFIG: ReviewConfig = {
  folder: 'journal',
  format: 'GGGG-[W]WW',
  template: null,
};

function normalizeFolder(folder: string): string {
  return folder.replaceAll('\\', '/').replace(/^\/+|\/+$/g, '');
}

export function reviewPagePath(config: ReviewConfig, date: Date): string {
  const name = formatDate(startOfIsoWeek(date), config.format || DEFAULT_REVIEW_CONFIG.format);
  const folder = normalizeFolder(config.folder);
  return `${folder ? `${folder}/` : ''}${name}.md`;
}

/** Monday of the week a review page covers, or null for other pages. */
export function reviewWeekFromPath(config: ReviewConfig, path: string): Date | null {
  const monday = journalDateFromPath(
    { folder: config.folder, format: config.format || DEFAULT_REVIEW_CONFIG.format, template: null },
    path,
  );
  if (!monday) return null;
  // Only week-based formats name reviews; a daily format would match days.
  return reviewPagePath(config, monday).toLowerCase() === path.replaceAll('\\', '/').toLowerCase()
    && dateKey(startOfIsoWeek(monday)) === dateKey(monday)
    ? monday
    : null;
}

export function weekLabel(date: Date): string {
  const { year, week } = isoWeek(date);
  return `${year}-W${String(week).padStart(2, '0')}`;
}

function pageLink(path: string): string {
  const target = path.replace(/\.md$/i, '');
  const name = target.split('/').at(-1) ?? target;
  return target === name ? `[[${name}]]` : `[[${target}|${name}]]`;
}

function yamlString(value: string): string {
  return `"${value.replaceAll('\\', '\\\\').replaceAll('"', '\\"')}"`;
}

export interface ReviewInput {
  /** Any day of the week to review. */
  date: Date;
  tasks: TaskItem[];
  /** Paths of journal entries with their day (`YYYY-MM-DD`). */
  journal: Array<{ key: string; path: string }>;
  /** Rendered with {{title}}, {{date}} and {{time}} when present. */
  template?: string | null;
  now?: Date;
}

/** The generated "this week" sections: plain bullets, so tasks are not duplicated. */
export function weekSummary({ date, tasks, journal }: ReviewInput): string[] {
  const monday = startOfIsoWeek(date);
  const first = dateKey(monday);
  const last = dateKey(addDays(monday, 6));
  const inWeek = (day: string | null) => day !== null && day >= first && day <= last;
  const done = tasks.filter((task) => task.status === 'done' && inWeek(task.doneDate));
  const open = tasks.filter((task) => {
    if (task.status === 'done' || task.status === 'cancelled') return false;
    const day = taskDay(task);
    return day !== null && day <= last;
  });
  const lines = ['## Done this week', ''];
  if (done.length === 0) lines.push('- Nothing checked off yet.');
  for (const task of done) lines.push(`- ${task.text} · ${pageLink(task.path)}`);
  lines.push('', '## Still open', '');
  if (open.length === 0) lines.push('- Nothing overdue or due this week.');
  for (const task of open) {
    const day = taskDay(task);
    const when = day && day < first ? `overdue since ${day}` : `due ${day}`;
    lines.push(`- ${task.text} (${when}) · ${pageLink(task.path)}`);
  }
  const days = journal.filter((entry) => inWeek(entry.key)).sort((left, right) => left.key.localeCompare(right.key));
  lines.push('', '## Journal', '');
  lines.push(days.length === 0
    ? '- No journal entries this week.'
    : days.map((entry) => {
      const day = new Date(Number(entry.key.slice(0, 4)), Number(entry.key.slice(5, 7)) - 1, Number(entry.key.slice(8, 10)));
      return `[[${entry.path.replace(/\.md$/i, '')}|${formatDate(day, 'ddd D')}]]`;
    }).join(' · '));
  return lines;
}

/** Body of a new weekly review page. */
export function newReviewBody(input: ReviewInput): string {
  const monday = startOfIsoWeek(input.date);
  const sunday = addDays(monday, 6);
  const label = weekLabel(monday);
  const summary = weekSummary(input);
  if (input.template && input.template.trim()) {
    return `${renderTemplate(input.template, monday, label, input.now).replace(/\s+$/, '')}\n\n${summary.join('\n')}\n`;
  }
  const range = monday.getMonth() === sunday.getMonth()
    ? `${formatDate(monday, 'MMM D')} – ${formatDate(sunday, 'D, YYYY')}`
    : `${formatDate(monday, 'MMM D')} – ${formatDate(sunday, 'MMM D, YYYY')}`;
  return [
    '---',
    'type: Review',
    `title: ${yamlString(label)}`,
    `week: ${yamlString(label)}`,
    '---',
    '',
    `# Week ${isoWeek(monday).week} · ${range}`,
    '',
    ...summary,
    '',
    '## What I learned',
    '',
    '## What got in the way',
    '',
    '## Next week',
    '',
  ].join('\n');
}
