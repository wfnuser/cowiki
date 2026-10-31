/**
 * Daily journal conventions. A journal entry is an ordinary wiki page whose
 * path is derived from a date, so it stays readable in any Markdown editor and
 * can link to knowledge, projects and tasks like every other page.
 *
 * The date format follows the moment.js subset that Obsidian's Daily Notes and
 * Periodic Notes plugins use, so an imported vault keeps its existing layout
 * (for example `YYYY-MM-DD` or `YYYY/MM/YYYY-MM-DD`).
 */

export interface JournalConfig {
  /** Folder that holds journal entries, relative to the Space root. */
  folder: string;
  /** moment-style date format; may contain `/` to nest entries by month. */
  format: string;
  /** Optional template page (repo path) used when creating a new entry. */
  template: string | null;
}

export const DEFAULT_JOURNAL_CONFIG: JournalConfig = {
  folder: 'journal',
  format: 'YYYY-MM-DD',
  template: null,
};

const MONTHS = [
  'January', 'February', 'March', 'April', 'May', 'June',
  'July', 'August', 'September', 'October', 'November', 'December',
];
const WEEKDAYS = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'];

// Week tokens (GGGG/gggg, WW/ww, W/w) use ISO weeks starting on Monday.
const TOKEN = /\[([^\]]*)\]|GGGG|gggg|WW|ww|W|w|YYYY|YY|MMMM|MMM|MM|M|Do|DD|D|dddd|ddd|HH|H|mm|ss|A|a/g;

function pad(value: number, width = 2): string {
  return String(value).padStart(width, '0');
}

function ordinal(day: number): string {
  const mod100 = day % 100;
  if (mod100 >= 11 && mod100 <= 13) return `${day}th`;
  switch (day % 10) {
    case 1: return `${day}st`;
    case 2: return `${day}nd`;
    case 3: return `${day}rd`;
    default: return `${day}th`;
  }
}

/** Format a local date with a moment.js-compatible subset of tokens. */
export function formatDate(date: Date, format: string): string {
  return format.replace(TOKEN, (match, literal: string | undefined) => {
    if (literal !== undefined) return literal;
    switch (match) {
      case 'GGGG': case 'gggg': return String(isoWeek(date).year);
      case 'WW': case 'ww': return pad(isoWeek(date).week);
      case 'W': case 'w': return String(isoWeek(date).week);
      case 'YYYY': return String(date.getFullYear());
      case 'YY': return pad(date.getFullYear() % 100);
      case 'MMMM': return MONTHS[date.getMonth()];
      case 'MMM': return MONTHS[date.getMonth()].slice(0, 3);
      case 'MM': return pad(date.getMonth() + 1);
      case 'M': return String(date.getMonth() + 1);
      case 'Do': return ordinal(date.getDate());
      case 'DD': return pad(date.getDate());
      case 'D': return String(date.getDate());
      case 'dddd': return WEEKDAYS[date.getDay()];
      case 'ddd': return WEEKDAYS[date.getDay()].slice(0, 3);
      case 'HH': return pad(date.getHours());
      case 'H': return String(date.getHours());
      case 'mm': return pad(date.getMinutes());
      case 'ss': return pad(date.getSeconds());
      case 'A': return date.getHours() < 12 ? 'AM' : 'PM';
      case 'a': return date.getHours() < 12 ? 'am' : 'pm';
      default: return match;
    }
  });
}

function escapeRegex(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

interface DatePattern {
  regex: RegExp;
  parts: string[];
}

function datePattern(format: string): DatePattern {
  const parts: string[] = [];
  let source = '';
  let last = 0;
  for (const match of format.matchAll(TOKEN)) {
    source += escapeRegex(format.slice(last, match.index));
    last = (match.index ?? 0) + match[0].length;
    if (match[1] !== undefined) {
      source += escapeRegex(match[1]);
      continue;
    }
    const token = match[0];
    parts.push(token);
    switch (token) {
      case 'YYYY': case 'GGGG': case 'gggg': source += '(\\d{4})'; break;
      case 'WW': case 'ww': source += '(\\d{2})'; break;
      case 'W': case 'w': source += '(\\d{1,2})'; break;
      case 'YY': source += '(\\d{2})'; break;
      case 'MMMM': source += `(${MONTHS.join('|')})`; break;
      case 'MMM': source += `(${MONTHS.map((m) => m.slice(0, 3)).join('|')})`; break;
      case 'MM': case 'DD': case 'HH': case 'mm': case 'ss': source += '(\\d{2})'; break;
      case 'M': case 'D': case 'H': source += '(\\d{1,2})'; break;
      case 'Do': source += '(\\d{1,2})(?:st|nd|rd|th)'; break;
      case 'dddd': source += `(${WEEKDAYS.join('|')})`; break;
      case 'ddd': source += `(${WEEKDAYS.map((d) => d.slice(0, 3)).join('|')})`; break;
      default: source += '(AM|PM|am|pm)'; break;
    }
  }
  source += escapeRegex(format.slice(last));
  return { regex: new RegExp(`^${source}$`, 'i'), parts };
}

/** Parse a formatted date back into a local Date, or null if it does not match. */
export function parseDate(value: string, format: string): Date | null {
  const { regex, parts } = datePattern(format);
  const match = regex.exec(value);
  if (!match) return null;
  let year: number | null = null;
  let month: number | null = null;
  let day: number | null = null;
  let weekYear: number | null = null;
  let week: number | null = null;
  parts.forEach((token, index) => {
    const raw = match[index + 1];
    switch (token) {
      case 'GGGG': case 'gggg': weekYear = Number(raw); break;
      case 'WW': case 'ww': case 'W': case 'w': week = Number(raw); break;
      case 'YYYY': year = Number(raw); break;
      case 'YY': year = 2000 + Number(raw); break;
      case 'MMMM': month = MONTHS.findIndex((m) => m.toLowerCase() === raw.toLowerCase()); break;
      case 'MMM': month = MONTHS.findIndex((m) => m.slice(0, 3).toLowerCase() === raw.toLowerCase()); break;
      case 'MM': case 'M': month = Number(raw) - 1; break;
      case 'DD': case 'D': case 'Do': day = Number(raw); break;
      default: break;
    }
  });
  if (weekYear !== null && week !== null && day === null) return isoWeekStart(weekYear, week);
  if (year === null || month === null || day === null || month < 0) return null;
  const date = new Date(year, month, day);
  if (date.getFullYear() !== year || date.getMonth() !== month || date.getDate() !== day) return null;
  return date;
}

/** Local calendar day as `YYYY-MM-DD`; never derived from UTC. */
export function dateKey(date: Date): string {
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

export function fromDateKey(key: string): Date | null {
  return parseDate(key, 'YYYY-MM-DD');
}

export function addDays(date: Date, days: number): Date {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate() + days);
}

export function startOfDay(date: Date): Date {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate());
}

/** ISO 8601 week: weeks start on Monday; week 1 contains January 4th. */
export function isoWeek(date: Date): { year: number; week: number } {
  const thursday = new Date(Date.UTC(date.getFullYear(), date.getMonth(), date.getDate()));
  thursday.setUTCDate(thursday.getUTCDate() + 4 - (thursday.getUTCDay() || 7));
  const yearStart = Date.UTC(thursday.getUTCFullYear(), 0, 1);
  return {
    year: thursday.getUTCFullYear(),
    week: Math.ceil(((thursday.getTime() - yearStart) / 86_400_000 + 1) / 7),
  };
}

export function startOfIsoWeek(date: Date): Date {
  return addDays(startOfDay(date), 1 - (date.getDay() || 7));
}

/** Monday of an ISO week, or null when the week does not exist that year. */
export function isoWeekStart(year: number, week: number): Date | null {
  if (week < 1 || week > 53) return null;
  const monday = addDays(startOfIsoWeek(new Date(year, 0, 4)), (week - 1) * 7);
  const check = isoWeek(monday);
  return check.year === year && check.week === week ? monday : null;
}

function normalizeFolder(folder: string): string {
  return folder.replaceAll('\\', '/').replace(/^\/+|\/+$/g, '').replace(/^\.\//, '');
}

/** Repo-relative Markdown path of the journal entry for a day. */
export function journalPagePath(config: JournalConfig, date: Date): string {
  const name = formatDate(date, config.format || DEFAULT_JOURNAL_CONFIG.format);
  const folder = normalizeFolder(config.folder);
  return `${folder ? `${folder}/` : ''}${name}.md`;
}

/** Date of a journal entry path, or null when the path is not a journal entry. */
export function journalDateFromPath(config: JournalConfig, path: string): Date | null {
  const normalized = path.replaceAll('\\', '/').replace(/^\/+/, '');
  if (!/\.md$/i.test(normalized)) return null;
  const folder = normalizeFolder(config.folder);
  let rest = normalized.replace(/\.md$/i, '');
  if (folder) {
    if (!rest.toLowerCase().startsWith(`${folder.toLowerCase()}/`)) return null;
    rest = rest.slice(folder.length + 1);
  }
  return parseDate(rest, config.format || DEFAULT_JOURNAL_CONFIG.format);
}

/** Sorted date keys of existing journal entries among the given page paths. */
export function journalEntryKeys(config: JournalConfig, paths: Iterable<string>): string[] {
  const keys = new Set<string>();
  for (const path of paths) {
    const date = journalDateFromPath(config, path);
    if (date) keys.add(dateKey(date));
  }
  return [...keys].sort();
}

/**
 * Render an Obsidian-compatible core template: `{{title}}`, `{{date}}`,
 * `{{time}}`, `{{date:FORMAT}}` and `{{time:FORMAT}}`. Unknown expressions
 * (for example Templater's `<% %>` blocks) are left untouched.
 */
export function renderTemplate(template: string, date: Date, title: string, now = new Date()): string {
  const moment = new Date(
    date.getFullYear(), date.getMonth(), date.getDate(),
    now.getHours(), now.getMinutes(), now.getSeconds(),
  );
  return template.replace(/\{\{\s*(title|date|time)(?::([^}]*))?\s*\}\}/g, (_match, name: string, format?: string) => {
    if (name === 'title') return title;
    if (name === 'date') return formatDate(moment, format?.trim() || 'YYYY-MM-DD');
    return formatDate(moment, format?.trim() || 'HH:mm');
  });
}

function yamlString(value: string): string {
  return `"${value.replaceAll('\\', '\\\\').replaceAll('"', '\\"')}"`;
}

/** Body of a brand-new journal entry: the configured template or a light default. */
export function newJournalBody(config: JournalConfig, date: Date, template: string | null, now = new Date()): string {
  const title = formatDate(date, (config.format || DEFAULT_JOURNAL_CONFIG.format).split('/').at(-1) || 'YYYY-MM-DD');
  if (template !== null && template.trim()) {
    return renderTemplate(template, date, title, now);
  }
  return [
    '---',
    'type: Journal',
    `title: ${yamlString(title)}`,
    `date: ${dateKey(date)}`,
    '---',
    '',
    `# ${formatDate(date, 'dddd, MMMM D, YYYY')}`,
    '',
    '## Plan',
    '',
    '## Notes',
    '',
    '## Review',
    '',
  ].join('\n');
}

export interface MonthCell {
  key: string;
  day: number;
  inMonth: boolean;
  isToday: boolean;
  hasEntry: boolean;
}

/** Six-week calendar grid (weeks start on Monday) for a month view. */
export function monthGrid(month: Date, today: Date, entries: ReadonlySet<string>): MonthCell[][] {
  const first = new Date(month.getFullYear(), month.getMonth(), 1);
  const offset = (first.getDay() + 6) % 7;
  const start = addDays(first, -offset);
  const todayKey = dateKey(today);
  const weeks: MonthCell[][] = [];
  for (let week = 0; week < 6; week += 1) {
    const row: MonthCell[] = [];
    for (let weekday = 0; weekday < 7; weekday += 1) {
      const date = addDays(start, week * 7 + weekday);
      const key = dateKey(date);
      row.push({
        key,
        day: date.getDate(),
        inMonth: date.getMonth() === month.getMonth(),
        isToday: key === todayKey,
        hasEntry: entries.has(key),
      });
    }
    weeks.push(row);
  }
  return weeks;
}

/**
 * Translate Obsidian Daily Notes / Periodic Notes settings into a journal
 * config. Missing values fall back to Obsidian's own defaults (vault root,
 * `YYYY-MM-DD`), not CoWiki's, so imported entries are found where they are.
 */
export function journalConfigFromObsidian(settings: unknown): JournalConfig | null {
  if (!settings || typeof settings !== 'object') return null;
  const record = settings as Record<string, unknown>;
  const daily = record.daily && typeof record.daily === 'object'
    ? record.daily as Record<string, unknown>
    : record;
  const text = (value: unknown) => (typeof value === 'string' ? value.trim() : '');
  const template = text(daily.template);
  return {
    folder: normalizeFolder(text(daily.folder)),
    format: text(daily.format) || 'YYYY-MM-DD',
    template: template ? `${template.replace(/\.md$/i, '')}.md` : null,
  };
}
