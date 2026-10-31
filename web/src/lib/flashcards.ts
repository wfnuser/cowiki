/**
 * Flashcards written in ordinary notes, compatible with Obsidian's Spaced
 * Repetition plugin and Logseq:
 *
 * - In a deck note (tagged `#flashcards`, in the text or frontmatter tags):
 *   `Question :: Answer`, `Question ::: Answer` (both directions), and
 *   multi-line cards with the question, a `?` line (`??` for both
 *   directions), then the answer up to a blank line.
 * - Anywhere: a Logseq block `- Question #card` whose indented children are
 *   the answer.
 *
 * Restricting `::` cards to deck notes keeps Dataview fields (`key:: value`)
 * from turning into cards. Review history is personal and stored outside the
 * Space (see ReviewStore), so card text and scheduling never mix.
 */

export interface Flashcard {
  /** Stable across moves: derived from the question and direction. */
  id: string;
  path: string;
  /** 0-based line of the question in the source page. */
  line: number;
  front: string;
  back: string;
  /** Changes when the answer is edited, to flag cards to re-check. */
  backHash: string;
  source: 'deck' | 'logseq';
}

export type Rating = 'again' | 'hard' | 'good' | 'easy';

export interface CardState {
  /** Local day (`YYYY-MM-DD`) the card is next due. */
  due: string;
  /** Days until the next review after the last answer; 0 while relearning. */
  interval: number;
  ease: number;
  reps: number;
  lapses: number;
  suspended?: boolean;
  last?: string;
  backHash?: string;
}

export interface ReviewSettings {
  newPerDay: number;
  reviewsPerDay: number;
}

export interface ReviewLogEntry {
  id: string;
  day: string;
  rating: Rating;
  wasNew: boolean;
}

export interface ReviewStore {
  version: 1;
  cards: Record<string, CardState>;
  log: ReviewLogEntry[];
  settings: ReviewSettings;
}

export const DEFAULT_REVIEW_SETTINGS: ReviewSettings = { newPerDay: 20, reviewsPerDay: 200 };
const LOG_LIMIT = 5000;
const START_EASE = 2.5;
const MIN_EASE = 1.3;

export function emptyReviewStore(): ReviewStore {
  return { version: 1, cards: {}, log: [], settings: { ...DEFAULT_REVIEW_SETTINGS } };
}

function hash(text: string): string {
  let value = 0x811c9dc5;
  for (let index = 0; index < text.length; index += 1) {
    value ^= text.charCodeAt(index);
    value = Math.imul(value, 0x01000193);
  }
  return (value >>> 0).toString(36);
}

function normalize(text: string): string {
  return text.normalize('NFC').replace(/\s+/g, ' ').trim();
}

function card(path: string, line: number, front: string, back: string, source: Flashcard['source'], reversed = false): Flashcard | null {
  const question = normalize(front);
  const answer = back.trim();
  if (!question || !answer) return null;
  return {
    id: hash(`${reversed ? 'rev' : 'fwd'}\u0000${question}`),
    path,
    line,
    front: front.trim(),
    back: answer,
    backHash: hash(normalize(answer)),
    source,
  };
}

const LIST_MARKER = /^\s*(?:[-*+]|\d+[.)])\s+/;
const DECK_TAG = /(^|\s)#flashcards(\/[^\s#]*)?(?=\s|$)/i;

function frontmatterEnd(lines: string[]): number {
  if (lines[0]?.trim() !== '---') return 0;
  const end = lines.findIndex((line, index) => index > 0 && line.trim() === '---');
  return end < 0 ? 0 : end + 1;
}

/** Whether a page is a deck for `::` and `?` cards. */
export function isDeck(content: string): boolean {
  const lines = content.split(/\r?\n/);
  const end = frontmatterEnd(lines);
  if (end > 0 && /(^|[\s,[\-'"])flashcards([\s,\]'"/]|$)/m.test(lines.slice(1, end - 1).filter((line) => !/^\s*title:/.test(line)).join('\n'))) {
    return true;
  }
  return lines.slice(end).some((line) => DECK_TAG.test(line));
}

function stripTags(text: string): string {
  return text.replace(/(^|\s)#flashcards(\/[^\s#]*)?(?=\s|$)/gi, '$1').replace(/(^|\s)#card(?=\s|$)/g, '$1').trim();
}

/** Cards defined in one page. */
export function parseCards(path: string, content: string): Flashcard[] {
  const lines = content.split(/\r?\n/);
  const deck = isDeck(content);
  const cards: Flashcard[] = [];
  const start = frontmatterEnd(lines);
  let fence: string | null = null;
  const inCode: boolean[] = lines.map((line, index) => {
    if (index < start) return true;
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

  for (let index = start; index < lines.length; index += 1) {
    if (inCode[index]) continue;
    const line = lines[index];

    const logseq = /^(\s*)[-*+]\s+(.*\s)?#card(\s.*)?$/.exec(line);
    if (logseq) {
      const indent = logseq[1].length;
      const answer: string[] = [];
      let next = index + 1;
      while (next < lines.length && (lines[next].trim() === '' || /^\s*/.exec(lines[next])![0].length > indent)) {
        if (lines[next].trim() && !/^\s*(?:[-*+]\s+)?(card-[a-z-]+|id)::/.test(lines[next])) {
          answer.push(lines[next].replace(LIST_MARKER, '').trim());
        }
        next += 1;
      }
      const front = stripTags(`${logseq[2] ?? ''}${logseq[3] ?? ''}`);
      const made = card(path, index, front, answer.join('\n'), 'logseq');
      if (made) cards.push(made);
      continue;
    }

    if (!deck) continue;
    const both = /^(.*?)\s*:::\s*(.+)$/.exec(line);
    if (both) {
      const front = stripTags(both[1].replace(LIST_MARKER, ''));
      const back = stripTags(both[2]);
      for (const made of [card(path, index, front, back, 'deck'), card(path, index, back, front, 'deck', true)]) {
        if (made) cards.push(made);
      }
      continue;
    }
    const single = /^(.*?)\s*::\s*(.+)$/.exec(line);
    // Logseq keeps its own block properties (card-*::, id::) as `key:: value`.
    if (single && !/^\s*(card-[a-z-]+|id)::/.test(line)) {
      const made = card(path, index, stripTags(single[1].replace(LIST_MARKER, '')), stripTags(single[2]), 'deck');
      if (made) cards.push(made);
      continue;
    }
    const separator = line.trim();
    if ((separator === '?' || separator === '??') && index > start) {
      let first = index;
      while (first - 1 >= start && lines[first - 1].trim() !== '' && !inCode[first - 1]) first -= 1;
      let last = index + 1;
      while (last < lines.length && lines[last].trim() !== '' && !inCode[last]) last += 1;
      const front = stripTags(lines.slice(first, index).join('\n'));
      const back = lines.slice(index + 1, last).join('\n');
      const made = card(path, first, front, back, 'deck');
      if (made) cards.push(made);
      if (separator === '??') {
        const reverse = card(path, first, back, front, 'deck', true);
        if (reverse) cards.push(reverse);
      }
      index = last - 1;
    }
  }
  return cards;
}

/** All cards in the given pages; the first copy of a duplicate question wins. */
export function collectCards(sources: Array<{ path: string; content: string }>): Flashcard[] {
  const seen = new Set<string>();
  const cards: Flashcard[] = [];
  for (const source of sources) {
    for (const item of parseCards(source.path, source.content)) {
      if (seen.has(item.id)) continue;
      seen.add(item.id);
      cards.push(item);
    }
  }
  return cards;
}

function addDaysToKey(day: string, days: number): string {
  const [year, month, date] = day.split('-').map(Number);
  const next = new Date(year, month - 1, date + days);
  return `${next.getFullYear()}-${String(next.getMonth() + 1).padStart(2, '0')}-${String(next.getDate()).padStart(2, '0')}`;
}

function nextInterval(state: CardState | undefined, rating: Rating): { interval: number; ease: number } {
  const ease = state?.ease ?? START_EASE;
  const interval = state?.interval ?? 0;
  if (!state || interval === 0) {
    switch (rating) {
      case 'again': return { interval: 0, ease: Math.max(MIN_EASE, ease - (state ? 0 : 0.2)) };
      case 'hard': return { interval: 1, ease: Math.max(MIN_EASE, ease - 0.15) };
      case 'good': return { interval: 1, ease };
      case 'easy': return { interval: 4, ease: ease + 0.15 };
    }
  }
  switch (rating) {
    case 'again': return { interval: 0, ease: Math.max(MIN_EASE, ease - 0.2) };
    case 'hard': return { interval: Math.max(interval + 1, Math.round(interval * 1.2)), ease: Math.max(MIN_EASE, ease - 0.15) };
    case 'good': return { interval: Math.max(interval + 1, Math.round(interval * ease)), ease };
    case 'easy': return { interval: Math.max(interval + 2, Math.round(interval * ease * 1.3)), ease: ease + 0.15 };
  }
}

/** SM-2 style scheduling in whole days (as in Anki): "again" keeps the card in today's session. */
export function schedule(state: CardState | undefined, rating: Rating, today: string, backHash?: string): CardState {
  const { interval, ease } = nextInterval(state, rating);
  return {
    due: addDaysToKey(today, interval),
    interval,
    ease: Math.round(ease * 100) / 100,
    reps: (state?.reps ?? 0) + 1,
    lapses: (state?.lapses ?? 0) + (rating === 'again' && state && state.interval > 0 ? 1 : 0),
    suspended: state?.suspended,
    last: today,
    backHash: backHash ?? state?.backHash,
  };
}

export function intervalLabel(days: number): string {
  if (days <= 0) return 'Today';
  if (days < 30) return `${days}d`;
  if (days < 365) return `${Math.round(days / 30)}mo`;
  return `${Math.round((days / 365) * 10) / 10}y`;
}

export function previewIntervals(state: CardState | undefined): Record<Rating, string> {
  return {
    again: intervalLabel(nextInterval(state, 'again').interval),
    hard: intervalLabel(nextInterval(state, 'hard').interval),
    good: intervalLabel(nextInterval(state, 'good').interval),
    easy: intervalLabel(nextInterval(state, 'easy').interval),
  };
}

export interface DeckStats {
  total: number;
  due: number;
  fresh: number;
  suspended: number;
  changed: number;
}

export function changedSinceReview(item: Flashcard, state: CardState | undefined): boolean {
  return !!state?.backHash && state.backHash !== item.backHash;
}

export function deckStats(cards: Flashcard[], store: ReviewStore, today: string): DeckStats {
  const stats: DeckStats = { total: cards.length, due: 0, fresh: 0, suspended: 0, changed: 0 };
  for (const item of cards) {
    const state = store.cards[item.id];
    if (state?.suspended) stats.suspended += 1;
    else if (!state) stats.fresh += 1;
    else if (state.due <= today) stats.due += 1;
    if (changedSinceReview(item, state)) stats.changed += 1;
  }
  return stats;
}

/** Today's queue: due reviews first (oldest first), then new cards, within the daily limits. */
export function buildQueue(cards: Flashcard[], store: ReviewStore, today: string): Flashcard[] {
  const todayLog = store.log.filter((entry) => entry.day === today);
  const newDone = new Set(todayLog.filter((entry) => entry.wasNew).map((entry) => entry.id)).size;
  const reviewsDone = new Set(todayLog.filter((entry) => !entry.wasNew).map((entry) => entry.id)).size;
  const due = cards
    .filter((item) => {
      const state = store.cards[item.id];
      return state && !state.suspended && state.due <= today;
    })
    .sort((left, right) => store.cards[left.id].due.localeCompare(store.cards[right.id].due));
  const fresh = cards.filter((item) => !store.cards[item.id]);
  return [
    ...due.slice(0, Math.max(0, store.settings.reviewsPerDay - reviewsDone)),
    ...fresh.slice(0, Math.max(0, store.settings.newPerDay - newDone)),
  ];
}

/** Record an answer and return the updated store. */
export function answerCard(store: ReviewStore, item: Flashcard, rating: Rating, today: string): ReviewStore {
  const previous = store.cards[item.id];
  const next = schedule(previous, rating, today, item.backHash);
  const log = [...store.log, { id: item.id, day: today, rating, wasNew: !previous }].slice(-LOG_LIMIT);
  return { ...store, cards: { ...store.cards, [item.id]: next }, log };
}

export function setSuspended(store: ReviewStore, item: Flashcard, suspended: boolean, today: string): ReviewStore {
  const previous = store.cards[item.id] ?? { due: today, interval: 0, ease: START_EASE, reps: 0, lapses: 0 };
  return { ...store, cards: { ...store.cards, [item.id]: { ...previous, suspended } } };
}

/** Accept a stored value, tolerating older or partial files. */
export function parseReviewStore(value: unknown): ReviewStore {
  const store = emptyReviewStore();
  if (!value || typeof value !== 'object') return store;
  const raw = value as Partial<ReviewStore>;
  if (raw.cards && typeof raw.cards === 'object') {
    for (const [id, state] of Object.entries(raw.cards)) {
      if (state && typeof state.due === 'string' && Number.isFinite(state.interval) && Number.isFinite(state.ease)) {
        store.cards[id] = state;
      }
    }
  }
  if (Array.isArray(raw.log)) store.log = raw.log.filter((entry) => entry && typeof entry.id === 'string').slice(-LOG_LIMIT);
  const settings = raw.settings;
  if (settings && Number.isFinite(settings.newPerDay) && Number.isFinite(settings.reviewsPerDay)) {
    store.settings = {
      newPerDay: Math.max(0, Math.min(9999, Math.round(settings.newPerDay))),
      reviewsPerDay: Math.max(0, Math.min(99999, Math.round(settings.reviewsPerDay))),
    };
  }
  return store;
}

/** Append a `Question :: Answer` card to a deck page, tagging it as a deck if needed. */
export function appendCard(content: string, front: string, back: string): string {
  const line = `${front.replace(/\s+/g, ' ').trim()} :: ${back.replace(/\s+/g, ' ').trim()}`;
  const base = content.replace(/\s+$/, '');
  const tag = isDeck(content) ? '' : '\n\n#flashcards';
  return `${base}${tag}\n\n${line}\n`;
}
