import { useEffect, useMemo, useRef, useState } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { ArrowUpRight, Check, ChevronLeft, ChevronRight, CirclePause, FileText, Layers, LoaderCircle, Play, RefreshCw, RotateCcw, SkipForward, SlidersHorizontal, X } from 'lucide-react';
import { Popover as PopoverPrimitive } from 'radix-ui';

import { withObsidianReader } from '@/components/ObsidianReader';
import { NumberField } from '@/components/ui/number-field';
import { ViewEmpty, ViewHeader, ViewIconButton, ViewSectionHeading } from '@/components/ui/view-header';
import { listCardSources, readCardReviews, writeCardReviews } from '@/local-api';
import { dateKey } from '@/lib/journal';
import { useI18n } from '@/lib/I18nContext';
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
  isNew,
  parseReviewStore,
  previewIntervals,
  renderCloze,
  resetCard,
  scheduleOf,
  setSuspended,
  type DeckNode,
  type Flashcard,
  type ReviewStore,
  type Score,
} from '@/lib/flashcards';

const MARKDOWN = withObsidianReader(undefined);
// Logseq's answers and shortcuts (f / t / r), with 1-3 as well.
const SCORES: Array<{ score: Score; label: string; keys: string[]; dot: string; tone: 'red' | 'amber' | 'green' }> = [
  { score: 1, label: 'Forgotten', keys: ['f', '1'], dot: 'bg-red', tone: 'red' },
  { score: 3, label: 'Took a while', keys: ['t', '2'], dot: 'bg-amber', tone: 'amber' },
  { score: 5, label: 'Remembered', keys: ['r', '3'], dot: 'bg-green', tone: 'green' },
];

interface FlashcardsViewProps {
  workspaceSlug: string;
  onOpenSource: (path: string) => void;
}

function CardText({ text, className }: { text: string; className: string }) {
  return (
    <div className={cn('prose', className)}>
      <ReactMarkdown remarkPlugins={[remarkGfm]} components={MARKDOWN}>{obsidianToReaderMarkdown(text)}</ReactMarkdown>
    </div>
  );
}

export function FlashcardsView({ workspaceSlug, onOpenSource }: FlashcardsViewProps) {
  const { t, tl } = useI18n();
  const [cards, setCards] = useState<Flashcard[]>([]);
  const [store, setStore] = useState<ReviewStore>(emptyReviewStore);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [reloadKey, setReloadKey] = useState(0);
  const [session, setSession] = useState<Flashcard[] | null>(null);
  /** The deck being studied, for the session's top line; null for all decks. */
  const [sessionDeck, setSessionDeck] = useState<string | null>(null);
  /** The deck whose overview is open (Anki's deck screen). */
  const [openDeck, setOpenDeck] = useState<string | null>(null);
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
  /** Today's queue of some cards, split like Anki's New and Due columns. */
  const queueOf = (some: Flashcard[]) => {
    const list = buildQueue(some, store, now, today);
    const fresh = list.filter((card) => isNew(store, card)).length;
    return { list, fresh, due: list.length - fresh };
  };
  const study = (list: Flashcard[], deck: string | null) => {
    setSession(list);
    setSessionSize(list.length);
    setSessionDeck(deck);
    setReviewed(0);
    setRevealed(false);
  };

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

  if (current) {
    const schedule = scheduleOf(store, current);
    const intervals = previewIntervals(schedule, store.ofMatrix);
    const fresh = isNew(store, current);
    const left = session?.length ?? 0;
    const progress = sessionSize > 0 ? (sessionSize - left) / sessionSize : 0;
    const tool = 'inline-flex size-8 items-center justify-center rounded-md text-text-tertiary transition-colors hover:bg-bg-hover hover:text-text disabled:pointer-events-none disabled:opacity-40 [&_svg]:size-4';
    const key = 'rounded border border-border px-1.5 text-caption font-normal leading-4 text-text-faint';
    return (
      <section className="mx-auto flex min-h-[calc(100vh-150px)] flex-col justify-center pb-8" style={{ width: 'min(640px, 100%)' }}>
        <div className="mb-2.5 flex h-8 items-center justify-between gap-3">
          <span className="flex min-w-0 items-center gap-2 text-small text-text-secondary">
            <span className="truncate font-medium">{sessionDeck ? sessionDeck.split('/').join(' / ') : t('All decks')}</span>
            <span className={cn('inline-flex shrink-0 items-center gap-1.5 text-meta', fresh ? 'text-blue' : 'text-green')}>
              <span aria-hidden className={cn('size-1.5 rounded-full', fresh ? 'bg-blue' : 'bg-green')} />
              {fresh ? t('New') : t('Review {count}', { count: schedule.repeats + 1 })}
            </span>
          </span>
          <div className="flex shrink-0 items-center gap-0.5">
            <span className="mr-2 text-meta tabular-nums text-text-tertiary">{t('{count} left', { count: left })}</span>
            <span aria-hidden className="mr-1.5 h-3.5 w-px bg-border-soft" />
            <button type="button" onClick={() => onOpenSource(current.path)} className={tool} aria-label={t('Open note')} title={`${t('Open note')} · ${current.path}`}><ArrowUpRight /></button>
            <button type="button" onClick={advance} className={tool} aria-label={t('Skip')} title={t('Skip (N)')}><SkipForward /></button>
            <button type="button" onClick={suspend} className={tool} aria-label={t('Suspend')} title={t('Suspend: stop showing this card until you resume it')}><CirclePause /></button>
            <button type="button" onClick={reset} disabled={fresh} className={tool} aria-label={t('Reset')} title={t('Reset: review this card again from the start')}><RotateCcw /></button>
            <button type="button" onClick={endSession} className={tool} aria-label={t('End')} title={t('End session')}><X /></button>
          </div>
        </div>
        <div className="mb-6 h-1 overflow-hidden rounded-full bg-border-soft" role="progressbar" aria-label={t('Session progress')} aria-valuemin={0} aria-valuemax={sessionSize} aria-valuenow={sessionSize - left}>
          <div className="h-full rounded-full bg-accent transition-[width] duration-300" style={{ width: `${progress * 100}%` }} />
        </div>
        {error && <div role="alert" className="mb-3 text-small text-red">{error}</div>}
        <div className={cn('study-stage', left > 1 && 'has-stack')}>
          <div className={cn('study-card', revealed && current.back && 'is-revealed')}>
            <div className="study-front">
              {changedSinceReview(store, current) && (
                <p className="mb-4 flex items-center justify-center gap-1.5 text-meta text-amber"><span className="size-1.5 rounded-full bg-current" />{t('The answer changed since your last review.')}</p>
              )}
              <CardText className="study-face" text={renderCloze(current.front, revealed)} />
            </div>
            {revealed && current.back && (
              <div className="study-answer-pane study-reveal">
                <CardText className="study-answer" text={renderCloze(current.back, true)} />
              </div>
            )}
          </div>
        </div>
        <div className="mt-5">
          {!revealed ? (
            <button type="button" onClick={() => setRevealed(true)} className="study-show-btn">
              {current.cloze && !current.back ? t('Show clozes') : t('Show answer')}
              <kbd className="study-show-kbd">Space</kbd>
            </button>
          ) : (
            <div className="study-reveal grid grid-cols-3 gap-3">
              {SCORES.map((item) => {
                const label = tl(item.label);
                return (
                  <button
                    key={item.score}
                    type="button"
                    data-tone={item.tone}
                    onClick={() => rate(item.score)}
                    title={`${label} (${item.keys[0].toUpperCase()})`}
                    className="study-rate-btn"
                  >
                    <span className="flex items-center gap-2 text-small font-medium text-text">
                      <span aria-hidden className={cn('size-2 rounded-full', item.dot)} />
                      {label}
                    </span>
                    <span className="flex items-center gap-2 text-meta tabular-nums text-text-tertiary">
                      <span>{tl(intervals[item.score])}</span>
                      <kbd className={key}>{item.keys[0].toUpperCase()}</kbd>
                    </span>
                  </button>
                );
              })}
            </div>
          )}
        </div>
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
              {t('Reviews')}
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
  const studyButton = (list: Flashcard[], deck: string | null, label: string) => (
    <button
      type="button"
      onClick={() => study(list, deck)}
      className="inline-flex h-9 items-center gap-2 rounded-lg bg-accent px-4 text-small font-medium text-on-accent transition-colors hover:bg-accent-hover"
    >
      <Play className="size-3.5 fill-current" aria-hidden />
      {label}
    </button>
  );
  const done = <p className="m-0 flex items-center gap-1.5 text-small text-text-tertiary"><Check className="size-4 text-green" aria-hidden /> {t('All done for today.')}</p>;

  // Anki's deck screen: what is due in one deck, and where its cards live.
  const deck = openDeck ? findDeck(tree, openDeck) : null;
  if (deck) {
    const queue = queueOf(deck.cards);
    const stats = deckStats(deck.cards, store, now);
    const byPage = new Map<string, number>();
    for (const item of deck.cards) byPage.set(item.path, (byPage.get(item.path) ?? 0) + 1);
    return (
      <section style={{ width: 'min(860px, 100%)' }}>
        <ViewHeader
          leading={<ViewIconButton label={t('All decks')} onClick={() => setOpenDeck(null)}><ChevronLeft className="size-4" /></ViewIconButton>}
          title={deck.path.split('/').join(' / ')}
          actions={<>{limits}{refreshButton}</>}
        />
        {error && <div role="alert" className="mb-3 text-small text-red">{error}</div>}
        <div className="deck-summary">
          <dl className="deck-metrics">
            <div className="deck-metric"><dt>{t('New')}</dt><dd className={queue.fresh ? 'text-blue' : 'text-text-faint'}>{queue.fresh}</dd></div>
            <div className="deck-metric"><dt>{t('Due')}</dt><dd className={queue.due ? 'text-green' : 'text-text-faint'}>{queue.due}</dd></div>
            <div className="deck-metric"><dt>{t('Cards')}</dt><dd className="text-text-secondary">{stats.total}</dd></div>
          </dl>
          <div>{queue.list.length ? studyButton(queue.list, deck.path, t('Study now')) : done}</div>
        </div>
        <ViewSectionHeading meta={byPage.size}>{t('Notes')}</ViewSectionHeading>
        <ul className="view-list">
          {[...byPage.entries()].sort(([left], [right]) => left.localeCompare(right)).map(([path, count]) => {
            const clean = path.replace(/\.md$/i, '');
            const slash = clean.lastIndexOf('/');
            const folder = slash >= 0 ? clean.slice(0, slash + 1) : '';
            const name = slash >= 0 ? clean.slice(slash + 1) : clean;
            return (
              <li key={path}>
                <button type="button" onClick={() => onOpenSource(path)} className="view-row" title={path}>
                  <span className="view-row-icon"><FileText size={16} /></span>
                  <span className="view-row-title">
                    {folder && <span className="text-text-tertiary">{folder}</span>}
                    <span>{name}</span>
                  </span>
                  <span className="view-row-meta">{t(count === 1 ? '{count} card' : '{count} cards', { count })}</span>
                </button>
              </li>
            );
          })}
        </ul>
      </section>
    );
  }

  const hasSubdecks = tree.some((node) => node.children.length > 0);
  const allFresh = queue.filter((card) => isNew(store, card)).length;
  const allDue = queue.length - allFresh;
  const rows = (nodes: DeckNode[], depth: number): React.ReactNode => nodes.map((node) => {
    const queue = queueOf(node.cards);
    const open = !collapsed.has(node.path);
    return (
      <li key={node.path}>
        <div className="deck-row" style={{ paddingLeft: (hasSubdecks ? 4 : 4) + depth * 18 }}>
          {hasSubdecks && (node.children.length > 0 ? (
            <button
              type="button"
              aria-label={open ? `Collapse ${node.name}` : `Expand ${node.name}`}
              onClick={() => setCollapsed((current) => {
                const next = new Set(current);
                if (open) next.add(node.path); else next.delete(node.path);
                return next;
              })}
              className="deck-row-toggle"
            >
              <ChevronRight className={cn('size-3.5 transition-transform', open && 'rotate-90')} aria-hidden />
            </button>
          ) : <span className="deck-row-toggle" aria-hidden />)}
          <button type="button" className="deck-row-main" onClick={() => setOpenDeck(node.path)}>
            <Layers className="mr-2.5 size-3.5 shrink-0 text-text-faint" aria-hidden />
            <span className="min-w-0 flex-1 truncate text-text">{node.name}</span>
            <span className={cn('deck-count', queue.fresh ? 'text-blue' : 'text-text-faint')}>{queue.fresh}</span>
            <span className={cn('deck-count', queue.due ? 'text-green' : 'text-text-faint')}>{queue.due}</span>
          </button>
          {queue.list.length > 0 ? (
            <button
              type="button"
              className="deck-row-study"
              aria-label={`Study ${node.name}`}
              title={`Study ${node.name}`}
              onClick={() => study(queue.list, node.path)}
            >
              <Play className="size-3 fill-current" aria-hidden />
            </button>
          ) : (
            <span className="inline-block size-7 shrink-0" aria-hidden />
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
          <div className="deck-summary">
            <dl className="deck-metrics">
              <div className="deck-metric"><dt>{t('New')}</dt><dd className={allFresh ? 'text-blue' : 'text-text-faint'}>{allFresh}</dd></div>
              <div className="deck-metric"><dt>{t('Due')}</dt><dd className={allDue ? 'text-green' : 'text-text-faint'}>{allDue}</dd></div>
              <div className="deck-metric"><dt>{t('Total')}</dt><dd className="text-text-secondary">{stats.total}</dd></div>
            </dl>
            <div>
              {queue.length > 0 ? studyButton(queue, null, t('Study all · {count}', { count: queue.length })) : done}
            </div>
          </div>
          <div className={cn('deck-head', !hasSubdecks && 'is-flat')} aria-hidden>
            <span className="flex-1">{t('Deck')}</span>
            <span className="deck-count">{t('New')}</span>
            <span className="deck-count">{t('Due')}</span>
          </div>
          <ul className="m-0 list-none border-y border-border-soft p-0" aria-label={t('Decks')}>{rows(tree, 0)}</ul>
          {(stats.suspended > 0 || reviewed > 0 || stats.changed > 0) && (
            <div className="mt-3 flex flex-wrap items-center justify-end gap-2 px-1 text-small text-text-tertiary">
              <span>
                {stats.suspended > 0 && <><span className="tabular-nums">{stats.suspended}</span> {t('suspended')}</>}
                {reviewed > 0 && <>{stats.suspended > 0 && ' · '}<span className="tabular-nums">{reviewed}</span> {t('reviewed now')}</>}
                {stats.changed > 0 && <span className="text-amber">{(stats.suspended > 0 || reviewed > 0) && ' · '}<span className="tabular-nums">{stats.changed}</span> {t('changed since review')}</span>}
              </span>
            </div>
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
