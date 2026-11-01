/**
 * Flashcards written inside ordinary notes, the way Logseq (file graphs) does
 * it:
 *
 * - A card is a bullet tagged `#card` (or `[[card]]`). The bullet is the
 *   question; its indented children are the answer.
 * - `{{cloze text}}` hides `text` until the answer is shown.
 * - Review state is written back into the note as block properties right
 *   under the question, so the schedule travels with the note:
 *
 *       - What does SM-5 adapt? #card
 *         card-last-interval:: 4
 *         card-repeats:: 1
 *         card-ease-factor:: 2.6
 *         card-next-schedule:: 2026-11-05T02:00:00.000Z
 *         card-last-reviewed:: 2026-11-01T02:00:00.000Z
 *         card-last-score:: 5
 *         - The optimal factor matrix
 *
 * Scheduling follows Logseq's SM-5 (frontend/extensions/srs.cljs in Logseq
 * 0.10.9). The optimal-factor matrix it learns over time is the only state kept
 * outside the notes; losing it just resets intervals to the defaults.
 */

export const CARD_PROPERTIES = [
  'card-last-interval',
  'card-repeats',
  'card-ease-factor',
  'card-next-schedule',
  'card-last-reviewed',
  'card-last-score',
] as const;

export interface CardSchedule {
  /** Days; -1 before the first successful answer. */
  lastInterval: number;
  repeats: number;
  ease: number;
  /** ISO timestamp, or null when the card was never scheduled. */
  nextSchedule: string | null;
  lastReviewed: string | null;
  lastScore: number | null;
}

export interface Flashcard {
  /**
   * Path, question and how many cards above it share that question. Only a
   * display key: each card's review state lives in its own block, so cards
   * with the same question in other pages never share history.
   */
  id: string;
  path: string;
  /** 0-based line of the question bullet in the source page. */
  line: number;
  front: string;
  back: string;
  cloze: boolean;
  schedule: CardSchedule;
}

/** Logseq's three answers: Forgotten, Took a while to recall, Remembered. */
export type Score = 1 | 3 | 5;

/** Optimal factors by repetition number, then ease (as strings, like JSON keys). */
export type OfMatrix = Record<string, Record<string, number>>;

export const DEFAULT_SCHEDULE: CardSchedule = {
  lastInterval: -1,
  repeats: 0,
  ease: 2.5,
  nextSchedule: null,
  lastReviewed: null,
  lastScore: null,
};

const INITIAL_INTERVAL = 4;
const LEARNING_FRACTION = 0.5;
const CARD_TAG = /(^|\s)#card(?=$|[\s.,;:!?)])|#?\[\[card\]\]/i;
const BULLET = /^(\s*)[-*+]\s+(.*)$/;
const PROPERTY = /^\s*([A-Za-z][\w-]*)::\s?(.*)$/;
const CLOZE = /\{\{cloze:?\s+([\s\S]*?)\}\}/g;

function hash(text: string): string {
  let value = 0x811c9dc5;
  for (let index = 0; index < text.length; index += 1) {
    value ^= text.charCodeAt(index);
    value = Math.imul(value, 0x01000193);
  }
  return (value >>> 0).toString(36);
}

function indentWidth(line: string): number {
  return /^\s*/.exec(line)![0].replace(/\t/g, '    ').length;
}

function frontmatterEnd(lines: string[]): number {
  if (lines[0]?.trim() !== '---') return 0;
  const end = lines.findIndex((line, index) => index > 0 && line.trim() === '---');
  return end < 0 ? 0 : end + 1;
}

function codeLines(lines: string[]): boolean[] {
  let fence: string | null = null;
  return lines.map((line) => {
    const marker = /^\s*(`{3,}|~{3,})/.exec(line)?.[1];
    if (fence) {
      if (marker && marker[0] === fence[0] && marker.length >= fence.length) fence = null;
      return true;
    }
    if (marker) {
      fence = marker;
      return true;
    }
    return false;
  });
}

function number(value: string | undefined, fallback: number): number {
  const parsed = Number.parseFloat(value ?? '');
  return Number.isFinite(parsed) ? parsed : fallback;
}

function text(value: string | undefined): string | null {
  const trimmed = value?.trim();
  return trimmed && trimmed !== 'nil' ? trimmed : null;
}

function scheduleFrom(properties: Map<string, string>): CardSchedule {
  const score = text(properties.get('card-last-score'));
  return {
    lastInterval: number(properties.get('card-last-interval'), DEFAULT_SCHEDULE.lastInterval),
    repeats: Math.round(number(properties.get('card-repeats'), DEFAULT_SCHEDULE.repeats)),
    ease: number(properties.get('card-ease-factor'), DEFAULT_SCHEDULE.ease),
    nextSchedule: text(properties.get('card-next-schedule')),
    lastReviewed: text(properties.get('card-last-reviewed')),
    lastScore: score === null ? null : number(score, 0),
  };
}

function stripCardTag(value: string): string {
  return value.replace(/#?\[\[card\]\]/gi, '').replace(/(^|\s)#card(?=$|[\s.,;:!?)])/gi, '$1').replace(/\s+$/, '').trim();
}

interface Block {
  /** Index of the bullet line. */
  line: number;
  /** Last line of the question's own text and properties (the bullet line when none). */
  ownEnd: number;
  indent: string;
  properties: Map<string, string>;
  front: string;
  back: string;
}

/** Card blocks in a page, with the line ranges needed to edit them. */
function cardBlocks(content: string): Block[] {
  const lines = content.split(/\r?\n/);
  const start = frontmatterEnd(lines);
  const inCode = codeLines(lines);
  const blocks: Block[] = [];
  for (let index = start; index < lines.length; index += 1) {
    if (inCode[index]) continue;
    const bullet = BULLET.exec(lines[index]);
    if (!bullet || !CARD_TAG.test(bullet[2])) continue;
    const width = indentWidth(lines[index]);
    const properties = new Map<string, string>();
    const front = [bullet[2]];
    let next = index + 1;
    // The question's own lines: properties and continuation text, up to the first child bullet.
    while (next < lines.length && lines[next].trim() !== '' && indentWidth(lines[next]) > width && !BULLET.test(lines[next])) {
      const property = PROPERTY.exec(lines[next]);
      if (property) {
        properties.set(property[1].toLowerCase(), property[2]);
      } else {
        front.push(lines[next].trim());
      }
      next += 1;
    }
    const ownEnd = next - 1;
    const children: string[] = [];
    while (next < lines.length && (lines[next].trim() === '' || indentWidth(lines[next]) > width)) {
      if (!PROPERTY.test(lines[next]) || inCode[next]) children.push(lines[next]);
      next += 1;
    }
    while (children.length > 0 && children.at(-1)!.trim() === '') children.pop();
    const base = Math.min(...children.filter((line) => line.trim()).map((line) => /^\s*/.exec(line)![0].length));
    blocks.push({
      line: index,
      ownEnd,
      indent: `${/^\s*/.exec(lines[index])![0]}  `,
      properties,
      front: stripCardTag(front.join('\n')),
      back: children.map((line) => line.slice(Number.isFinite(base) ? base : 0)).join('\n').trim(),
    });
    index = next - 1;
  }
  return blocks;
}

/** Cards defined in one page. */
export function parseCards(path: string, content: string): Flashcard[] {
  const seen = new Map<string, number>();
  return cardBlocks(content)
    .filter((block) => block.front || block.back)
    .map((block) => {
      const occurrence = seen.get(block.front) ?? 0;
      seen.set(block.front, occurrence + 1);
      return { block, occurrence };
    })
    .map(({ block, occurrence }) => ({
      id: `${hash(`${path}\u0000${block.front}`)}-${occurrence}`,
      path,
      line: block.line,
      front: block.front,
      back: block.back,
      cloze: /\{\{cloze:?\s/.test(`${block.front}\n${block.back}`),
      schedule: scheduleFrom(block.properties),
    }));
}

export function collectCards(sources: Array<{ path: string; content: string }>): Flashcard[] {
  return sources.flatMap((source) => parseCards(source.path, source.content));
}

/** Hide `{{cloze}}` text as `[...]`, or reveal it highlighted. */
export function renderCloze(value: string, reveal: boolean): string {
  return value.replace(CLOZE, (_match, inner: string) => (reveal ? `==${inner.trim()}==` : '[...]'));
}

// ---------------------------------------------------------------------------
// SM-5, as in Logseq 0.10.9 (https://www.supermemo.com/en/archives1990-2015/english/ol/sm5)

const fix2 = (value: number) => Math.round(value * 100) / 100;

function getOf(matrix: OfMatrix, repetition: number, ease: number): number {
  return matrix[String(repetition)]?.[String(ease)] ?? (repetition <= 1 ? INITIAL_INTERVAL : ease);
}

function intervalFor(repetition: number, ease: number, matrix: OfMatrix): number {
  if (repetition <= 1) return getOf(matrix, 1, ease);
  return getOf(matrix, repetition, ease) * intervalFor(repetition - 1, ease, matrix);
}

function nextEase(ease: number, score: Score): number {
  const next = ease + (0.1 - (5 - score) * (0.08 + 0.02 * (5 - score)));
  return next < 1.3 ? 1.3 : next;
}

function nextMatrix(matrix: OfMatrix, repetition: number, score: Score, ease: number): OfMatrix {
  const of = getOf(matrix, repetition, ease);
  const adjusted = of * (0.72 + score * 0.07);
  const blended = (1 - LEARNING_FRACTION) * of + adjusted * LEARNING_FRACTION;
  const key = String(repetition);
  return { ...matrix, [key]: { ...matrix[key], [String(ease)]: fix2(blended) } };
}

/** Days until the next review and the updated factors, like Logseq's `next-interval`. */
function nextInterval(schedule: CardSchedule, score: Score, matrix: OfMatrix) {
  const ease = schedule.ease || 2.5;
  const repeats = schedule.repeats;
  const updated = nextMatrix(matrix, repeats, score, ease);
  if (score < 3) return { interval: -1, repeats: 1, ease, matrix: updated };
  const easeAfter = nextEase(ease, score);
  return { interval: fix2(intervalFor(repeats, easeAfter, updated)), repeats: repeats + 1, ease: fix2(easeAfter), matrix: updated };
}

function tomorrow(now: Date): Date {
  return new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1);
}

/** Answer a card: its new schedule and the updated factor matrix. */
export function review(schedule: CardSchedule, score: Score, matrix: OfMatrix, now: Date): { schedule: CardSchedule; matrix: OfMatrix } {
  const next = nextInterval(schedule, score, matrix);
  // Logseq shows a forgotten card again tomorrow.
  const due = score === 1 ? tomorrow(now) : new Date(now.getTime() + Math.max(0, next.interval) * 86_400_000);
  return {
    matrix: next.matrix,
    schedule: {
      lastInterval: next.interval,
      repeats: next.repeats,
      ease: next.ease,
      nextSchedule: due.toISOString(),
      lastReviewed: now.toISOString(),
      lastScore: score,
    },
  };
}

export function intervalLabel(days: number): string {
  if (days < 1) return 'Tomorrow';
  if (days < 30) return `${Math.round(days)}d`;
  if (days < 365) return `${Math.round(days / 30)}mo`;
  return `${Math.round((days / 365) * 10) / 10}y`;
}

export function previewIntervals(schedule: CardSchedule, matrix: OfMatrix): Record<Score, string> {
  return {
    1: 'Tomorrow',
    3: intervalLabel(nextInterval(schedule, 3, matrix).interval),
    5: intervalLabel(nextInterval(schedule, 5, matrix).interval),
  };
}

/** Logseq's rule: never answered correctly, unscheduled, or scheduled before now. */
export function isDue(card: Flashcard, now: Date): boolean {
  const { repeats, nextSchedule } = card.schedule;
  if (repeats < 1 || !nextSchedule) return true;
  const time = Date.parse(nextSchedule);
  return !Number.isFinite(time) || time < now.getTime();
}

export function isNew(card: Flashcard): boolean {
  return card.schedule.repeats < 1 && !card.schedule.lastReviewed;
}

export interface DeckStats {
  total: number;
  due: number;
  fresh: number;
}

export function deckStats(cards: Flashcard[], now: Date): DeckStats {
  return {
    total: cards.length,
    due: cards.filter((item) => isDue(item, now)).length,
    fresh: cards.filter(isNew).length,
  };
}

/** Cards due now, unscheduled ones first, then by next review time (as Logseq sorts them). */
export function buildQueue(cards: Flashcard[], now: Date): Flashcard[] {
  return cards
    .filter((item) => isDue(item, now))
    .sort((left, right) => (left.schedule.nextSchedule ?? '').localeCompare(right.schedule.nextSchedule ?? ''));
}

function propertyValues(schedule: CardSchedule | null): Record<(typeof CARD_PROPERTIES)[number], string | null> {
  const value = schedule ?? { ...DEFAULT_SCHEDULE };
  return {
    'card-last-interval': String(value.lastInterval),
    'card-repeats': String(value.repeats),
    'card-ease-factor': String(value.ease),
    'card-next-schedule': value.nextSchedule,
    'card-last-reviewed': value.lastReviewed,
    'card-last-score': value.lastScore === null ? null : String(value.lastScore),
  };
}

/**
 * Write a card's schedule into its page, under the question bullet. The card
 * is found again in the current text, so edits since it was loaded are kept.
 * A null schedule resets the card (Logseq's "Reset").
 */
export function writeSchedule(content: string, card: Pick<Flashcard, 'front' | 'line'>, schedule: CardSchedule | null): string {
  const eol = content.includes('\r\n') ? '\r\n' : '\n';
  const lines = content.split(/\r?\n/);
  const candidates = cardBlocks(content).filter((block) => block.front === card.front);
  if (candidates.length === 0) throw new Error('This card was changed or removed. Refresh and try again.');
  const block = candidates.reduce((best, item) => (Math.abs(item.line - card.line) < Math.abs(best.line - card.line) ? item : best));
  const values = propertyValues(schedule);
  const own = lines.slice(block.line + 1, block.ownEnd + 1);
  const indent = own.find((line) => PROPERTY.test(line))?.match(/^\s*/)?.[0] ?? block.indent;
  const isCardProperty = (line: string) => (CARD_PROPERTIES as readonly string[]).includes(PROPERTY.exec(line)?.[1].toLowerCase() ?? '');
  const kept = own.filter((line) => !isCardProperty(line));
  // Properties sit right after the question's first line, after any others it already has (id::, ...).
  let at = 0;
  while (at < kept.length && PROPERTY.test(kept[at])) at += 1;
  const written = CARD_PROPERTIES.filter((key) => values[key] !== null).map((key) => `${indent}${key}:: ${values[key]}`);
  lines.splice(block.line + 1, own.length, ...kept.slice(0, at), ...written, ...kept.slice(at));
  return lines.join(eol);
}

/** Append a card as a Logseq block at the end of a page. */
export function appendCardBlock(content: string, front: string, back: string): string {
  const question = front.replace(/\s+/g, ' ').trim();
  const answer = back.trim().split(/\r?\n/).filter((line) => line.trim()).map((line) => `  - ${line.trim()}`);
  const base = content.replace(/\s+$/, '');
  return `${base}${base ? '\n\n' : ''}- ${question} #card\n${answer.join('\n')}${answer.length ? '\n' : ''}`;
}

export function parseOfMatrix(value: unknown): OfMatrix {
  const raw = value && typeof value === 'object' ? (value as { ofMatrix?: unknown }).ofMatrix : null;
  if (!raw || typeof raw !== 'object') return {};
  const matrix: OfMatrix = {};
  for (const [repetition, row] of Object.entries(raw as Record<string, unknown>)) {
    if (!row || typeof row !== 'object') continue;
    for (const [ease, factor] of Object.entries(row as Record<string, unknown>)) {
      if (typeof factor === 'number' && Number.isFinite(factor)) {
        matrix[repetition] = { ...matrix[repetition], [ease]: factor };
      }
    }
  }
  return matrix;
}
