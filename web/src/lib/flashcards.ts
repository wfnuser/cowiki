/**
 * Flashcards written in ordinary notes with Logseq's syntax:
 *
 * - A card is a bullet tagged `#card` (or `[[card]]`). The bullet is the
 *   question; its indented children are the answer.
 * - `{{cloze text}}` hides `text` until the answer is shown.
 *
 * Cards are only read from notes. Review history is personal and kept per
 * device outside the Space (ReviewStore), so card content and scheduling never
 * mix and reviewing never changes a note. Scheduling follows Logseq's SM-5
 * (frontend/extensions/srs.cljs in Logseq 0.10.9) with its three answers.
 * A note imported from Logseq may carry `card-*::` properties; they seed a
 * card's schedule until it is first reviewed here.
 */

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
   * Key of the card's review history: the block's Logseq `id::` when it has
   * one, otherwise page, question and how many cards above share that
   * question. Cards with the same question in different pages are separate.
   */
  id: string;
  path: string;
  /** 0-based line of the question bullet in the source page. */
  line: number;
  front: string;
  back: string;
  cloze: boolean;
  /** Changes when the answer is edited, to suggest re-checking the card. */
  backHash: string;
  /** Schedule from Logseq `card-*::` properties in the note, if any (read only). */
  imported: CardSchedule | null;
}

/** Logseq's three answers: Forgotten, Took a while to recall, Remembered. */
export type Score = 1 | 3 | 5;

/** Optimal factors by repetition number, then ease (as strings, like JSON keys). */
export type OfMatrix = Record<string, Record<string, number>>;

export interface CardState extends CardSchedule {
  suspended?: boolean;
  /** Answer hash when last reviewed. */
  backHash?: string;
  /** Where the card was last seen, to keep its history when its page moves. */
  path: string;
  front: string;
}

export interface ReviewSettings {
  newPerDay: number;
  reviewsPerDay: number;
}

export interface ReviewLogEntry {
  id: string;
  /** Local day, `YYYY-MM-DD`. */
  day: string;
  score: Score;
  wasNew: boolean;
}

export interface ReviewStore {
  version: 3;
  cards: Record<string, CardState>;
  log: ReviewLogEntry[];
  settings: ReviewSettings;
  ofMatrix: OfMatrix;
}

export const DEFAULT_SCHEDULE: CardSchedule = {
  lastInterval: -1,
  repeats: 0,
  ease: 2.5,
  nextSchedule: null,
  lastReviewed: null,
  lastScore: null,
};

export const DEFAULT_REVIEW_SETTINGS: ReviewSettings = { newPerDay: 20, reviewsPerDay: 200 };
const LOG_LIMIT = 5000;

export function emptyReviewStore(): ReviewStore {
  return { version: 3, cards: {}, log: [], settings: { ...DEFAULT_REVIEW_SETTINGS }, ofMatrix: {} };
}

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
  properties: Map<string, string>;
  front: string;
  back: string;
}

/** Card blocks in a page. */
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
    const children: string[] = [];
    while (next < lines.length && (lines[next].trim() === '' || indentWidth(lines[next]) > width)) {
      if (!PROPERTY.test(lines[next]) || inCode[next]) children.push(lines[next]);
      next += 1;
    }
    while (children.length > 0 && children.at(-1)!.trim() === '') children.pop();
    const base = Math.min(...children.filter((line) => line.trim()).map((line) => /^\s*/.exec(line)![0].length));
    blocks.push({
      line: index,
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
    .map(({ block, occurrence }) => {
      const blockId = text(block.properties.get('id'));
      const imported = [...block.properties.keys()].some((key) => key.startsWith('card-'))
        ? scheduleFrom(block.properties)
        : null;
      return {
        id: blockId ? `block:${blockId}` : `${hash(`${path}\u0000${block.front}`)}-${occurrence}`,
        path,
        line: block.line,
        front: block.front,
        back: block.back,
        cloze: /\{\{cloze:?\s/.test(`${block.front}\n${block.back}`),
        backHash: hash(block.back.replace(/\s+/g, ' ').trim()),
        imported,
      };
    });
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

/** A card's saved review state, or what its note imported from Logseq. */
export function stateOf(store: ReviewStore, card: Flashcard): CardState | undefined {
  const saved = store.cards[card.id];
  if (saved) return saved;
  return card.imported ? { ...card.imported, path: card.path, front: card.front } : undefined;
}

export function scheduleOf(store: ReviewStore, card: Flashcard): CardSchedule {
  return stateOf(store, card) ?? DEFAULT_SCHEDULE;
}

/** Logseq's rule: never answered correctly, unscheduled, or scheduled before now. */
export function isDue(schedule: CardSchedule, now: Date): boolean {
  if (schedule.repeats < 1 || !schedule.nextSchedule) return true;
  const time = Date.parse(schedule.nextSchedule);
  return !Number.isFinite(time) || time < now.getTime();
}

export function isNew(store: ReviewStore, card: Flashcard): boolean {
  const state = stateOf(store, card);
  return !state || (state.repeats < 1 && !state.lastReviewed);
}

export function changedSinceReview(store: ReviewStore, card: Flashcard): boolean {
  const backHash = store.cards[card.id]?.backHash;
  return !!backHash && backHash !== card.backHash;
}

/**
 * Keep history for cards whose page was moved or renamed: a saved state whose
 * card is gone moves to the one new card with the same question and answer.
 * A card that only shares the question (a different topic) never inherits it.
 */
export function adoptMovedCards(cards: Flashcard[], store: ReviewStore): ReviewStore {
  const current = new Set(cards.map((card) => card.id));
  const orphans = Object.entries(store.cards).filter(([id]) => !current.has(id));
  if (orphans.length === 0) return store;
  const unsaved = cards.filter((card) => !store.cards[card.id]);
  let next = store;
  for (const [id, state] of orphans) {
    const same = (card: Flashcard) => card.front === state.front && card.backHash === state.backHash;
    const matches = unsaved.filter((card) => same(card) && card.path !== state.path);
    const others = orphans.filter(([otherId, other]) => otherId !== id && other.front === state.front && other.backHash === state.backHash);
    if (matches.length !== 1 || others.length > 0) continue;
    const [card] = matches;
    const cardsById = { ...next.cards };
    delete cardsById[id];
    cardsById[card.id] = { ...state, path: card.path };
    next = {
      ...next,
      cards: cardsById,
      log: next.log.map((entry) => (entry.id === id ? { ...entry, id: card.id } : entry)),
    };
  }
  return next;
}

export interface DeckStats {
  total: number;
  due: number;
  fresh: number;
  suspended: number;
  changed: number;
}

export function deckStats(cards: Flashcard[], store: ReviewStore, now: Date): DeckStats {
  const stats: DeckStats = { total: cards.length, due: 0, fresh: 0, suspended: 0, changed: 0 };
  for (const card of cards) {
    if (stateOf(store, card)?.suspended) stats.suspended += 1;
    else if (isNew(store, card)) stats.fresh += 1;
    else if (isDue(scheduleOf(store, card), now)) stats.due += 1;
    if (changedSinceReview(store, card)) stats.changed += 1;
  }
  return stats;
}

/** Today's queue: due reviews (earliest first), then new cards, within the daily limits. */
export function buildQueue(cards: Flashcard[], store: ReviewStore, now: Date, today: string): Flashcard[] {
  const todayLog = store.log.filter((entry) => entry.day === today);
  const newDone = new Set(todayLog.filter((entry) => entry.wasNew).map((entry) => entry.id)).size;
  const reviewsDone = new Set(todayLog.filter((entry) => !entry.wasNew).map((entry) => entry.id)).size;
  const active = cards.filter((card) => !stateOf(store, card)?.suspended);
  const due = active
    .filter((card) => !isNew(store, card) && isDue(scheduleOf(store, card), now))
    .sort((left, right) => (scheduleOf(store, left).nextSchedule ?? '').localeCompare(scheduleOf(store, right).nextSchedule ?? ''));
  const fresh = active.filter((card) => isNew(store, card));
  return [
    ...due.slice(0, Math.max(0, store.settings.reviewsPerDay - reviewsDone)),
    ...fresh.slice(0, Math.max(0, store.settings.newPerDay - newDone)),
  ];
}

/** Record an answer and return the updated store. */
export function answerCard(store: ReviewStore, card: Flashcard, score: Score, now: Date, today: string): ReviewStore {
  const previous = stateOf(store, card);
  const next = review(previous ?? DEFAULT_SCHEDULE, score, store.ofMatrix, now);
  const state: CardState = {
    ...next.schedule,
    ...(previous?.suspended ? { suspended: true } : {}),
    backHash: card.backHash,
    path: card.path,
    front: card.front,
  };
  return {
    ...store,
    ofMatrix: next.matrix,
    cards: { ...store.cards, [card.id]: state },
    log: [...store.log, { id: card.id, day: today, score, wasNew: isNew(store, card) }].slice(-LOG_LIMIT),
  };
}

export function setSuspended(store: ReviewStore, card: Flashcard, suspended: boolean): ReviewStore {
  const previous = stateOf(store, card) ?? { ...DEFAULT_SCHEDULE, path: card.path, front: card.front };
  return { ...store, cards: { ...store.cards, [card.id]: { ...previous, suspended } } };
}

/** Logseq's "Reset": review the card again from the start. */
export function resetCard(store: ReviewStore, card: Flashcard): ReviewStore {
  const suspended = store.cards[card.id]?.suspended ? { suspended: true } : {};
  return {
    ...store,
    cards: { ...store.cards, [card.id]: { ...DEFAULT_SCHEDULE, ...suspended, path: card.path, front: card.front } },
  };
}

function finite(value: unknown): value is number {
  return typeof value === 'number' && Number.isFinite(value);
}

function parseMatrix(raw: unknown): OfMatrix {
  if (!raw || typeof raw !== 'object') return {};
  const matrix: OfMatrix = {};
  for (const [repetition, row] of Object.entries(raw as Record<string, unknown>)) {
    if (!row || typeof row !== 'object') continue;
    for (const [ease, factor] of Object.entries(row as Record<string, unknown>)) {
      if (finite(factor)) matrix[repetition] = { ...matrix[repetition], [ease]: factor };
    }
  }
  return matrix;
}

/**
 * Accept a stored value, tolerating partial files and earlier formats: the
 * first format (SM-2, keyed by question) keeps only its daily limits, and the
 * second (factor matrix only) keeps the matrix.
 */
export function parseReviewStore(value: unknown): ReviewStore {
  const store = emptyReviewStore();
  if (!value || typeof value !== 'object') return store;
  const raw = value as Partial<Omit<ReviewStore, 'version'>> & { version?: number };
  store.ofMatrix = parseMatrix(raw.ofMatrix);
  const settings = raw.settings;
  if (settings && finite(settings.newPerDay) && finite(settings.reviewsPerDay)) {
    store.settings = {
      newPerDay: Math.max(0, Math.min(9999, Math.round(settings.newPerDay))),
      reviewsPerDay: Math.max(0, Math.min(99999, Math.round(settings.reviewsPerDay))),
    };
  }
  if (raw.version !== 3) return store;
  for (const [id, state] of Object.entries(raw.cards ?? {})) {
    if (state && finite(state.repeats) && finite(state.ease) && finite(state.lastInterval) && typeof state.front === 'string' && typeof state.path === 'string') {
      store.cards[id] = state;
    }
  }
  if (Array.isArray(raw.log)) {
    store.log = raw.log.filter((entry) => entry && typeof entry.id === 'string' && typeof entry.day === 'string').slice(-LOG_LIMIT);
  }
  return store;
}
