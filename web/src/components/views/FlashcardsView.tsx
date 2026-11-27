import { useEffect, useMemo, useRef, useState } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { ArrowUpRight, ChevronRight, CirclePause, LoaderCircle, RefreshCw, RotateCcw, SkipForward, SlidersHorizontal, X } from 'lucide-react';
import { Popover as PopoverPrimitive } from 'radix-ui';

import { withObsidianReader } from '@/components/ObsidianReader';
import { NumberField } from '@/components/ui/number-field';
import { ViewEmpty, ViewHeader, ViewIconButton } from '@/components/ui/view-header';
import { listCardSources, readCardReviews, writeCardReviews } from '@/local-api';
import { dateKey } from '@/lib/journal';
import { useI18n } from '@/lib/I18nContext';
import type { TranslationKey } from '@/lib/i18n';
import { obsidianToReaderMarkdown } from '@/lib/obsidian-markdown';
import { cn } from '@/lib/utils';
import {
  adoptMovedCards,
  answerCard,
  buildQueue,
  changedSinceReview,
  collectCards,
  deckStats,
  deckTree,
  emptyReviewStore,
  isDue,
  isNew,
  masteryOf,
  parseReviewStore,
  previewIntervals,
  renderCloze,
  resetCard,
  scheduleOf,
  setSuspended,
  type DeckNode,
  type Flashcard,
  type Mastery,
  type ReviewStore,
  type Score,
} from '@/lib/flashcards';

const MARKDOWN = withObsidianReader(undefined);
// Logseq's answers and shortcuts (f / t / r), with 1-3 as well.
const SCORES: Array<{ score: Score; label: string; keys: string[] }> = [
  { score: 1, label: 'Forgotten', keys: ['f', '1'] },
  { score: 3, label: 'Took a while', keys: ['t', '2'] },
  { score: 5, label: 'Remembered', keys: ['r', '3'] },
];

interface FlashcardsViewProps {
  workspaceSlug: string;
  /** The open deck (`Japanese/N2`), or null for every deck. */
  deck: string | null;
  /** A study session is on screen; it is its own step in Back and Forward. */
  studying: boolean;
  /** Open a deck or start studying: one step in Back and Forward. */
  onNavigate: (next: { deck: string | null; studying: boolean }) => void;
  /** The session ended: step back to where it started. */
  onLeaveStudy: () => void;
  onOpenSource: (path: string) => void;
}

/** A card's text on one line: clozes shown, Markdown marks dropped. */
function plainText(value: string): string {
  return renderCloze(value, true)
    .replace(/!\[\[[^\]]*\]\]/g, '')
    .replace(/\[\[([^\]|]*)(?:\|([^\]]*))?\]\]/g, (_match, target: string, alias?: string) => alias || target)
    .replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/(\*\*|__|==|~~|`)/g, '')
    .replace(/^\s*(?:[-*+]|\d+\.)\s+/gm, '')
    .replace(/\s+/g, ' ')
    .trim();
}

const MASTERY_LABEL: Record<Mastery, TranslationKey> = {
  new: 'New',
  learning: 'Learning',
  mastered: 'Mastered',
  suspended: 'Suspended',
};

function CardText({ text, className }: { text: string; className: string }) {
  return (
    <div className={cn('prose', className)}>
      <ReactMarkdown remarkPlugins={[remarkGfm]} components={MARKDOWN}>{obsidianToReaderMarkdown(text)}</ReactMarkdown>
    </div>
  );
}

export function FlashcardsView({ workspaceSlug, deck: deckPath, studying: studyStep, onNavigate, onLeaveStudy, onOpenSource }: FlashcardsViewProps) {
  const { t, tl, locale, intl } = useI18n();
  /** The next interval as a person says it: "Tomorrow", "4d" → "4 天". */
  const intervalText = (label: string) => {
    const match = /^(\d+(?:\.\d+)?)(d|mo|y)$/.exec(label);
    if (!match) return tl(label);
    const key = match[2] === 'd' ? '{count}d' : match[2] === 'mo' ? '{count}mo' : '{count}y';
    return t(key, { count: match[1] });
  };
  const [cards, setCards] = useState<Flashcard[]>([]);
  const [store, setStore] = useState<ReviewStore>(emptyReviewStore);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [reloadKey, setReloadKey] = useState(0);
  const [session, setSession] = useState<Flashcard[] | null>(null);
  /** The deck being studied, for the session's top line; null for all decks. */
  const [sessionDeck, setSessionDeck] = useState<string | null>(null);
  const [collapsed, setCollapsed] = useState<Set<string>>(() => new Set());
  const [reviewed, setReviewed] = useState(0);
  /** How many cards the session started with, for its progress line. */
  const [sessionSize, setSessionSize] = useState(0);
  const [revealed, setRevealed] = useState(false);
  const [now, setNow] = useState(() => new Date());
  const today = dateKey(now);

  const persist = (next: ReviewStore) => {
    setStore(next);
    writeCardReviews(workspaceSlug, next).catch((caught: unknown) => {
      setError(`Could not save review history: ${caught instanceof Error ? caught.message : String(caught)}`);
    });
  };

  useEffect(() => {
    let cancelled = false;
    const timer = window.setTimeout(() => {
      Promise.all([listCardSources(workspaceSlug), readCardReviews(workspaceSlug)])
        .then(([sources, saved]) => {
          if (cancelled) return;
          const found = collectCards(sources);
          const loaded = parseReviewStore(saved);
          // Cards whose page moved keep their history.
          const adopted = adoptMovedCards(found, loaded);
          setCards(found);
          setStore(adopted);
          const wanted = studyRef.current;
          if (wanted.studying && !sessionRef.current) {
            const deck = wanted.deck ? findDeck(deckTree(found), wanted.deck) : null;
            begin(buildQueue(deck ? deck.cards : found, adopted, new Date(), dateKey(new Date())), wanted.deck);
          }
          if (adopted !== loaded) writeCardReviews(workspaceSlug, adopted).catch(() => {});
          setNow(new Date());
          setError('');
        })
        .catch((caught: unknown) => {
          if (!cancelled) setError(caught instanceof Error ? caught.message : String(caught));
        })
        .finally(() => {
          if (!cancelled) setLoading(false);
        });
    }, 0);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [workspaceSlug, reloadKey]);

  const stats = useMemo(() => deckStats(cards, store, now), [cards, store, now]);
  const queue = useMemo(() => buildQueue(cards, store, now, today), [cards, store, now, today]);
  const tree = useMemo(() => deckTree(cards), [cards]);
  /** Start showing a session's cards (local state only). */
  function begin(list: Flashcard[], deck: string | null) {
    setSession(list);
    setSessionSize(list.length);
    setSessionDeck(deck);
    setReviewed(0);
    setRevealed(false);
  }
  /** Start studying: the session becomes a step in Back and Forward. */
  const study = (list: Flashcard[], deck: string | null) => {
    begin(list, deck);
    onNavigate({ deck, studying: true });
  };
  const queueOf = (deck: string | null) => {
    const node = deck ? findDeck(tree, deck) : null;
    return node ? buildQueue(node.cards, store, now, today) : queue;
  };

  // Back or Forward moved in or out of a study step.
  const [shownStep, setShownStep] = useState({ deck: deckPath, studying: studyStep });
  if (shownStep.deck !== deckPath || shownStep.studying !== studyStep) {
    setShownStep({ deck: deckPath, studying: studyStep });
    if (!studyStep && session) {
      setSession(null);
      setRevealed(false);
      setNow(new Date());
    }
    if (studyStep && !session && !loading) begin(queueOf(deckPath), deckPath);
  }
  const studyRef = useRef({ deck: deckPath, studying: studyStep });
  const sessionRef = useRef(session);
  useEffect(() => {
    studyRef.current = { deck: deckPath, studying: studyStep };
    sessionRef.current = session;
  });

  const current = session?.[0] ?? null;

  /** Move to the next card; refresh the counts when the session is over. */
  const advance = () => {
    const remaining = session?.slice(1) ?? [];
    setRevealed(false);
    if (remaining.length > 0) {
      setSession(remaining);
      return;
    }
    setSession(null);
    setNow(new Date());
    onLeaveStudy();
  };

  const rate = (score: Score) => {
    if (!current) return;
    persist(answerCard(store, current, score, new Date(), dateKey(new Date())));
    setReviewed((count) => count + 1);
    advance();
  };

  const suspend = () => {
    if (!current) return;
    persist(setSuspended(store, current, true));
    advance();
  };

  const reset = () => {
    if (!current) return;
    persist(resetCard(store, current));
    advance();
  };

  const endSession = () => {
    setSession(null);
    setRevealed(false);
    setReloadKey((key) => key + 1);
    onLeaveStudy();
  };

  const updateSetting = (key: 'newPerDay' | 'reviewsPerDay', value: string) => {
    const amount = Math.max(0, Math.min(9999, Math.round(Number(value) || 0)));
    persist({ ...store, settings: { ...store.settings, [key]: amount } });
  };

  const onStudyKey = useRef<(event: KeyboardEvent) => void>(() => {});
  useEffect(() => {
    onStudyKey.current = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (target && (target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName))) return;
      if (event.metaKey || event.ctrlKey || event.altKey) return;
      if (!revealed && (event.key === ' ' || event.key === 'Enter' || event.key === 's')) {
        event.preventDefault();
        setRevealed(true);
        return;
      }
      if (event.key === 'n') {
        event.preventDefault();
        advance();
        return;
      }
      const choice = SCORES.find((item) => item.keys.includes(event.key));
      if (revealed && choice) {
        event.preventDefault();
        rate(choice.score);
      }
    };
  });

  const studying = current !== null;
  useEffect(() => {
    if (!studying) return;
    const onKey = (event: KeyboardEvent) => onStudyKey.current(event);
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [studying]);

  // A study session's pieces; the layout below decides where each one sits.
  let parts: { deck: string; left: string; done: number; size: number; tools: React.ReactNode; card: React.ReactNode; actions: React.ReactNode } | null = null;
  if (current) {
    const schedule = scheduleOf(store, current);
    const intervals = previewIntervals(schedule, store.ofMatrix);
    const fresh = isNew(store, current);
    const left = session?.length ?? 0;
    const tool = 'inline-flex size-7 items-center justify-center rounded-md text-text-tertiary transition-colors hover:bg-bg-hover hover:text-text disabled:pointer-events-none disabled:opacity-40 [&_svg]:size-4';
    parts = {
      deck: sessionDeck ? sessionDeck.split('/').join(' / ') : t('All decks'),
      left: t('{count} left', { count: left }),
      done: sessionSize - left,
      size: sessionSize,
      tools: (
        <>
          <button type="button" onClick={() => onOpenSource(current.path)} className={tool} aria-label={t('Open note')} title={`${t('Open note')} · ${current.path}`}><ArrowUpRight /></button>
          <button type="button" onClick={advance} className={tool} aria-label={t('Skip')} title={t('Skip (N)')}><SkipForward /></button>
          <button type="button" onClick={suspend} className={tool} aria-label={t('Suspend')} title={t('Suspend: stop showing this card until you resume it')}><CirclePause /></button>
          <button type="button" onClick={reset} disabled={fresh} className={tool} aria-label={t('Reset')} title={t('Reset: review this card again from the start')}><RotateCcw /></button>
          <button type="button" onClick={endSession} className={tool} aria-label={t('End')} title={t('End session')}><X /></button>
        </>
      ),
      card: (
        <div className={cn('study-card', revealed && current.back && 'is-revealed')}>
          <div className="study-front">
            {changedSinceReview(store, current) && (
              <p className="mb-4 text-meta text-text-tertiary">{t('The answer changed since your last review.')}</p>
            )}
            <CardText className="study-face" text={renderCloze(current.front, revealed)} />
          </div>
          {revealed && current.back && (
            <div className="study-answer-pane study-reveal">
              <CardText className="study-answer" text={renderCloze(current.back, true)} />
            </div>
          )}
        </div>
      ),
      actions: (
        <div className="study-actions">
          {!revealed ? (
            <button type="button" onClick={() => setRevealed(true)} className="study-action">
              <span className="study-action-label">{current.cloze && !current.back ? t('Show clozes') : t('Show answer')}</span>
              <kbd className="study-key">{locale === 'en' ? 'Space' : t('Space bar')}</kbd>
            </button>
          ) : (
            <div className="study-reveal study-ratings">
              {SCORES.map((item) => {
                const label = tl(item.label);
                return (
                  <button
                    key={item.score}
                    type="button"
                    onClick={() => rate(item.score)}
                    title={`${label} (${item.keys[0].toUpperCase()})`}
                    className="study-action is-rating"
                  >
                    <span className="study-action-label">{label}</span>
                    <span className="flex items-center gap-1.5">
                      <span className="study-interval">{intervalText(intervals[item.score])}</span>
                      <kbd className="study-key">{item.keys[0].toUpperCase()}</kbd>
                    </span>
                  </button>
                );
              })}
            </div>
          )}
        </div>
      ),
    };
  }

  if (parts) {
    return (
      <section className="mx-auto flex min-h-[calc(100vh-150px)] flex-col justify-center pb-8" style={{ width: 'min(640px, 100%)' }}>
        <div className="mb-2.5 flex h-8 items-center justify-between gap-3">
          <span className="truncate text-small font-medium text-text-secondary">{parts.deck}</span>
          <div className="flex shrink-0 items-center gap-0.5">
            <span className="mr-2 text-meta tabular-nums text-text-tertiary">{parts.left}</span>
            {parts.tools}
          </div>
        </div>
        <div className="study-progress" role="progressbar" aria-label={t('Session progress')} aria-valuemin={0} aria-valuemax={parts.size} aria-valuenow={parts.done}>
          <div style={{ width: `${parts.size > 0 ? (parts.done / parts.size) * 100 : 0}%` }} />
        </div>
        {error && <div role="alert" className="mb-3 text-small text-red">{error}</div>}
        {parts.card}
        {parts.actions}
      </section>
    );
  }

  const limits = (
    <PopoverPrimitive.Root>
      <PopoverPrimitive.Trigger asChild>
        <button type="button" aria-label={t('Daily limits')} title={t('Daily limits')} className="flex size-8 items-center justify-center rounded-md text-text-tertiary transition-colors hover:bg-bg-hover hover:text-text">
          <SlidersHorizontal className="size-3.5" />
        </button>
      </PopoverPrimitive.Trigger>
      <PopoverPrimitive.Portal>
        <PopoverPrimitive.Content align="end" sideOffset={6} className="z-[200] w-60 rounded-lg border border-border bg-panel p-3 shadow-[var(--shadow-popover)]">
          <p className="mb-2 text-small font-medium text-text">{t('Daily limits')}</p>
          <div className="flex flex-col gap-2 text-small text-text-secondary">
            <span className="flex items-center justify-between gap-3">
              {t('New cards')}
              <NumberField label={t('New cards per day')} value={store.settings.newPerDay} onChange={(value) => updateSetting('newPerDay', String(value))} />
            </span>
            <span className="flex items-center justify-between gap-3">
              {t('Card reviews')}
              <NumberField label={t('Reviews per day')} value={store.settings.reviewsPerDay} step={10} onChange={(value) => updateSetting('reviewsPerDay', String(value))} />
            </span>
          </div>
        </PopoverPrimitive.Content>
      </PopoverPrimitive.Portal>
    </PopoverPrimitive.Root>
  );
  const refreshButton = (
    <ViewIconButton label={t('Refresh')} onClick={() => setReloadKey((key) => key + 1)}>
      <RefreshCw className="size-3.5" />
    </ViewIconButton>
  );
  /** Readwise's one line: how many cards today, and one word to start. */
  const todayLine = (list: Flashcard[], deck: string | null, detail?: string) => (
    <div className="flashcards-today">
      <p>
        {list.length > 0 ? (
          <>
            <span>{t(list.length === 1 ? '{count} card to review today.' : '{count} cards to review today.', { count: list.length })}</span>
            <button type="button" className="flashcards-start" onClick={() => study(list, deck)}>{t('Start')}</button>
          </>
        ) : (
          <span className="text-text-tertiary">{t('All done for today.')}</span>
        )}
      </p>
      {detail && <p className="flashcards-today-detail">{detail}</p>}
    </div>
  );

  /** When a card comes up next, the way a person says it. */
  const nextReview = (card: Flashcard, mastery: Mastery) => {
    if (mastery === 'new' || mastery === 'suspended') return '';
    const schedule = scheduleOf(store, card);
    if (isDue(schedule, now) || !schedule.nextSchedule) return t('Today');
    const next = new Date(schedule.nextSchedule);
    const day = (date: Date) => new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
    const days = Math.round((day(next) - day(now)) / 86_400_000);
    if (days <= 0) return t('Today');
    if (days === 1) return t('Tomorrow');
    if (days < 30) return t('in {count} days', { count: days });
    return next.toLocaleDateString(intl, next.getFullYear() === now.getFullYear()
      ? { month: 'short', day: 'numeric' }
      : { year: 'numeric', month: 'short', day: 'numeric' });
  };

  // One deck: its line for today, how well its cards are known, then the cards.
  const deck = deckPath ? findDeck(tree, deckPath) : null;
  if (deck) {
    const list = buildQueue(deck.cards, store, now, today);
    const rows = [...deck.cards]
      .sort((left, right) => left.path.localeCompare(right.path) || left.line - right.line)
      .map((card) => ({ card, mastery: masteryOf(store, card) }));
    const count = (mastery: Mastery) => rows.filter((row) => row.mastery === mastery).length;
    const suspended = count('suspended');
    const detail = t(rows.length === 1 ? '{total} card: {new} new, {learning} learning, {mastered} mastered.' : '{total} cards: {new} new, {learning} learning, {mastered} mastered.', {
      total: rows.length,
      new: count('new'),
      learning: count('learning'),
      mastered: count('mastered'),
    }) + (suspended > 0 ? `${locale === 'en' ? ' ' : ''}${t('{count} suspended.', { count: suspended })}` : '');
    return (
      <section style={{ width: 'min(860px, 100%)' }}>
        <ViewHeader title={deck.path.split('/').join(' / ')} actions={<>{limits}{refreshButton}</>} />
        {error && <div role="alert" className="mb-3 text-small text-red">{error}</div>}
        {todayLine(list, deck.path, detail)}
        <ul className="view-list" aria-label={t('Cards')}>
          {rows.map(({ card, mastery }) => {
            const back = plainText(card.back);
            const next = nextReview(card, mastery);
            return (
              <li key={card.id}>
                <div className={cn('deck-line', mastery === 'suspended' && 'is-suspended has-action')}>
                  <button type="button" className="deck-open" onClick={() => onOpenSource(card.path)} title={`${t('Open note')} · ${card.path.replace(/\.md$/i, '')}`}>
                    <span className="deck-name">
                      {plainText(card.front)}
                      {back && <span className="view-row-detail">{back}</span>}
                    </span>
                    <span className="card-mastery" data-mastery={mastery}>{t(MASTERY_LABEL[mastery])}</span>
                    <span className="card-next line-swap">{next}</span>
                  </button>
                  {mastery === 'suspended' && (
                    <button type="button" className="line-action" onClick={() => persist(setSuspended(store, card, false))}>
                      {t('Resume')}
                    </button>
                  )}
                </div>
              </li>
            );
          })}
        </ul>
      </section>
    );
  }

  const hasSubdecks = tree.some((node) => node.children.length > 0);
  const rows = (nodes: DeckNode[], depth: number): React.ReactNode => nodes.map((node) => {
    const list = buildQueue(node.cards, store, now, today);
    const open = !collapsed.has(node.path);
    return (
      <li key={node.path}>
        <div className={cn('deck-line', list.length > 0 && 'has-due has-action')}>
          {hasSubdecks && (
            <span className="deck-toggle-slot" style={{ marginLeft: depth * 18 }}>
              {node.children.length > 0 && (
                <button
                  type="button"
                  aria-label={t(open ? 'Collapse {name}' : 'Expand {name}', { name: node.name })}
                  onClick={() => setCollapsed((current) => {
                    const next = new Set(current);
                    if (open) next.add(node.path); else next.delete(node.path);
                    return next;
                  })}
                  className="deck-toggle"
                >
                  <ChevronRight className={cn('size-3.5 transition-transform', open && 'rotate-90')} aria-hidden />
                </button>
              )}
            </span>
          )}
          <button type="button" className="deck-open" onClick={() => onNavigate({ deck: node.path, studying: false })}>
            <span className="deck-name">{node.name}</span>
            <span className="deck-due line-swap">{list.length > 0 ? t('{count} to review', { count: list.length }) : t('Done')}</span>
          </button>
          {list.length > 0 && (
            <button type="button" className="line-action" aria-label={t('Study {name}', { name: node.name })} onClick={() => study(list, node.path)}>
              {t('Start')}
            </button>
          )}
        </div>
        {open && node.children.length > 0 && <ul className="m-0 list-none p-0">{rows(node.children, depth + 1)}</ul>}
      </li>
    );
  });

  return (
    <section style={{ width: 'min(860px, 100%)' }}>
      <ViewHeader
        title={t('Flashcards')}
        actions={(
          <>
            {limits}
            {refreshButton}
          </>
        )}
      />

      {error && <div role="alert" className="mb-3 text-small text-red">{error}</div>}

      {loading ? (
        <p className="flex items-center gap-2 py-10 text-small text-text-tertiary"><LoaderCircle className="size-3.5 animate-spin" /> {t('Collecting cards…')}</p>
      ) : cards.length === 0 ? (
        <ViewEmpty>
          <span>{t('No cards yet. Tag a list item with #card to make one.')}</span>
        </ViewEmpty>
      ) : (
        <>
          {todayLine(queue, null)}
          <ul className="view-list" aria-label={t('Decks')}>{rows(tree, 0)}</ul>
          {(stats.suspended > 0 || reviewed > 0 || stats.changed > 0) && (
            <p className="mt-4 text-meta text-text-tertiary">
              {[
                stats.suspended > 0 && `${stats.suspended} ${t('suspended')}`,
                reviewed > 0 && `${reviewed} ${t('reviewed now')}`,
                stats.changed > 0 && `${stats.changed} ${t('changed since review')}`,
              ].filter(Boolean).join(' · ')}
            </p>
          )}
        </>
      )}
    </section>
  );
}

function findDeck(nodes: DeckNode[], path: string): DeckNode | null {
  for (const node of nodes) {
    if (node.path === path) return node;
    const found = findDeck(node.children, path);
    if (found) return found;
  }
  return null;
}
