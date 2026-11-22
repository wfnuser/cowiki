import { useEffect, useMemo, useState } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { ArrowUpRight, FileText, LoaderCircle, PauseCircle, RefreshCw, RotateCcw, SlidersHorizontal } from 'lucide-react';
import { Popover as PopoverPrimitive } from 'radix-ui';

import { withObsidianReader } from '@/components/ObsidianReader';
import { NumberField } from '@/components/ui/number-field';
import { listCardSources, readCardReviews, writeCardReviews } from '@/local-api';
import { C } from '@/lib/design';
import { dateKey } from '@/lib/journal';
import { obsidianToReaderMarkdown } from '@/lib/obsidian-markdown';
import { cn } from '@/lib/utils';
import {
  adoptMovedCards,
  answerCard,
  buildQueue,
  changedSinceReview,
  collectCards,
  deckStats,
  emptyReviewStore,
  isNew,
  parseReviewStore,
  previewIntervals,
  renderCloze,
  resetCard,
  scheduleOf,
  setSuspended,
  type Flashcard,
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
  onOpenSource: (path: string) => void;
}

function CardText({ text }: { text: string }) {
  return (
    <div className="page-prose" style={{ fontSize: 16, lineHeight: 1.6, color: C.ink, overflowWrap: 'anywhere' }}>
      <ReactMarkdown remarkPlugins={[remarkGfm]} components={MARKDOWN}>{obsidianToReaderMarkdown(text)}</ReactMarkdown>
    </div>
  );
}

export function FlashcardsView({ workspaceSlug, onOpenSource }: FlashcardsViewProps) {
  const [cards, setCards] = useState<Flashcard[]>([]);
  const [store, setStore] = useState<ReviewStore>(emptyReviewStore);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [reloadKey, setReloadKey] = useState(0);
  const [session, setSession] = useState<Flashcard[] | null>(null);
  const [reviewed, setReviewed] = useState(0);
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
  const decks = useMemo(() => {
    const byPage = new Map<string, Flashcard[]>();
    for (const item of cards) byPage.set(item.path, [...(byPage.get(item.path) ?? []), item]);
    return [...byPage.entries()].sort(([left], [right]) => left.localeCompare(right));
  }, [cards]);

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

  useEffect(() => {
    if (!current) return;
    const onKey = (event: KeyboardEvent) => {
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
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  if (current) {
    const schedule = scheduleOf(store, current);
    const intervals = previewIntervals(schedule, store.ofMatrix);
    const fresh = isNew(store, current);
    const quiet = 'inline-flex h-7 items-center gap-1.5 rounded-md px-2 text-[12px] text-text-tertiary transition-colors hover:bg-bg-hover hover:text-text disabled:pointer-events-none disabled:opacity-40 [&_svg]:size-3.5';
    return (
      <section className="mx-auto" style={{ width: 'min(680px, 100%)' }}>
        <div className="mb-3 flex items-center justify-between gap-3">
          <span className="text-[12.5px] text-text-tertiary">
            <span className="tabular-nums">{session?.length ?? 0}</span> left · {fresh ? 'New' : `Review ${schedule.repeats + 1}`}
          </span>
          <div className="flex items-center">
            <button type="button" onClick={() => onOpenSource(current.path)} className={quiet} title={current.path}>
              <ArrowUpRight /> Open note
            </button>
            <button type="button" onClick={suspend} className={quiet} title="Stop showing this card until you resume it">
              <PauseCircle /> Suspend
            </button>
            <button type="button" onClick={reset} disabled={fresh} className={quiet} title="Review this card again from the start">
              <RotateCcw /> Reset
            </button>
            <button type="button" onClick={endSession} className={cn(quiet, 'text-text-secondary')}>End</button>
          </div>
        </div>
        {error && <div role="alert" className="mb-3 text-[12.5px] text-red">{error}</div>}
        <div className="min-h-56 rounded-xl border border-border bg-panel px-8 py-7 shadow-[var(--shadow-rest)]">
          {changedSinceReview(store, current) && (
            <p className="mb-3 flex items-center gap-1.5 text-[12px] text-amber"><span className="size-1.5 rounded-full bg-current" />The answer changed since your last review.</p>
          )}
          <CardText text={renderCloze(current.front, revealed)} />
          {revealed && current.back && (
            <>
              <div className="my-5 h-px bg-border-soft" />
              <CardText text={renderCloze(current.back, true)} />
            </>
          )}
        </div>
        <div className="mt-4 flex justify-center gap-2">
          {!revealed ? (
            <>
              <button type="button" onClick={() => setRevealed(true)} className="inline-flex h-9 items-center gap-2 rounded-lg bg-accent px-4 text-[13px] font-medium text-on-accent transition-colors hover:bg-accent-hover">
                {current.cloze && !current.back ? 'Show clozes' : 'Show answer'}
                <kbd className="rounded border border-on-accent/40 px-1 text-[10.5px] font-normal">Space</kbd>
              </button>
              <button type="button" onClick={advance} className="inline-flex h-9 items-center gap-2 rounded-lg px-3 text-[13px] text-text-secondary transition-colors hover:bg-bg-hover">
                Skip <kbd className="rounded border border-border px-1 text-[10.5px] text-text-tertiary">n</kbd>
              </button>
            </>
          ) : SCORES.map((item) => (
            <button
              key={item.score}
              type="button"
              onClick={() => rate(item.score)}
              className="flex min-w-28 flex-col items-center gap-0.5 rounded-lg border border-border bg-panel px-3 py-2 text-[13px] font-medium text-text transition-colors hover:border-border-hover hover:bg-bg-hover"
            >
              {item.label}
              <span className="text-[11px] font-normal text-text-tertiary">{intervals[item.score]} · {item.keys[0]}</span>
            </button>
          ))}
        </div>
      </section>
    );
  }

  // Only counts worth reading: a zero says nothing.
  const summary = ([
    [stats.due, 'due'],
    [stats.fresh, 'new'],
    [stats.total, stats.total === 1 ? 'card' : 'cards'],
    [stats.suspended, 'suspended'],
  ] as Array<[number, string]>).filter(([value, label]) => value > 0 || label.startsWith('card'));

  return (
    <section className="mx-auto" style={{ width: 'min(720px, 100%)' }}>
      <header className="mb-5 flex items-center justify-between gap-4">
        <h1 className="page-title page-title--compact" style={{ margin: 0 }}>Flashcards</h1>
        <div className="flex items-center">
          <PopoverPrimitive.Root>
            <PopoverPrimitive.Trigger asChild>
              <button type="button" aria-label="Daily limits" title="Daily limits" className="flex size-8 items-center justify-center rounded-md text-text-tertiary transition-colors hover:bg-bg-hover hover:text-text">
                <SlidersHorizontal className="size-3.5" />
              </button>
            </PopoverPrimitive.Trigger>
            <PopoverPrimitive.Portal>
              <PopoverPrimitive.Content align="end" sideOffset={6} className="z-[200] w-60 rounded-lg border border-border bg-panel p-3 shadow-[var(--shadow-popover)]">
                <p className="mb-2 text-[12.5px] font-medium text-text">Daily limits</p>
                <div className="flex flex-col gap-2 text-[12.5px] text-text-secondary">
                  <span className="flex items-center justify-between gap-3">
                    New cards
                    <NumberField label="New cards per day" value={store.settings.newPerDay} onChange={(value) => updateSetting('newPerDay', String(value))} />
                  </span>
                  <span className="flex items-center justify-between gap-3">
                    Reviews
                    <NumberField label="Reviews per day" value={store.settings.reviewsPerDay} step={10} onChange={(value) => updateSetting('reviewsPerDay', String(value))} />
                  </span>
                </div>
              </PopoverPrimitive.Content>
            </PopoverPrimitive.Portal>
          </PopoverPrimitive.Root>
          <button type="button" onClick={() => setReloadKey((key) => key + 1)} aria-label="Refresh" title="Refresh" className="flex size-8 items-center justify-center rounded-md text-text-tertiary transition-colors hover:bg-bg-hover hover:text-text">
            <RefreshCw className="size-3.5" />
          </button>
        </div>
      </header>

      {error && <div role="alert" className="mb-3 text-[12.5px] text-red">{error}</div>}

      {loading ? (
        <p className="flex items-center gap-2 py-10 text-[12.5px] text-text-tertiary"><LoaderCircle className="size-3.5 animate-spin" /> Collecting cards…</p>
      ) : (
        <>
          <div className="mb-6 flex flex-wrap items-center gap-x-4 gap-y-2">
            <button
              type="button"
              disabled={queue.length === 0}
              onClick={() => { setSession(queue); setReviewed(0); setRevealed(false); }}
              className="inline-flex h-9 items-center rounded-lg bg-accent px-4 text-[13px] font-medium text-on-accent transition-colors hover:bg-accent-hover disabled:bg-bg-active disabled:text-text-tertiary"
            >
              {queue.length === 0 ? 'Nothing to review' : `Review ${queue.length}`}
            </button>
            <span className="text-[12.5px] text-text-tertiary">
              {summary.map(([value, label], index) => (
                <span key={label}>{index > 0 && ' · '}<span className="tabular-nums text-text-secondary">{value}</span> {label}</span>
              ))}
              {reviewed > 0 && <> · {reviewed} reviewed now</>}
              {stats.changed > 0 && <span className="text-amber"> · {stats.changed} changed</span>}
            </span>
          </div>

          {decks.length === 0 ? (
            <p className="py-10 text-center text-[12.5px] text-text-tertiary">No cards yet. Tag a bullet with <code className="font-mono">#card</code>; its child bullets are the answer.</p>
          ) : (
            <ul className="m-0 list-none border-t border-border-soft p-0">
              {decks.map(([path, items]) => {
                const deck = deckStats(items, store, now);
                return (
                  <li key={path} className="border-b border-border-soft">
                    <button
                      type="button"
                      onClick={() => onOpenSource(path)}
                      className="group flex w-full items-center gap-2.5 px-2 py-2.5 text-left transition-colors hover:bg-bg-hover/60"
                      title={path}
                    >
                      <FileText className="size-3.5 shrink-0 text-text-faint" aria-hidden />
                      <span className="min-w-0 flex-1 truncate text-[13.5px] text-text">{path.replace(/\.md$/i, '')}</span>
                      <span className="shrink-0 text-[12px] tabular-nums text-text-tertiary">
                        {deck.due > 0 && <span className="text-text-secondary">{deck.due} due · </span>}
                        {deck.total} {deck.total === 1 ? 'card' : 'cards'}
                        {deck.suspended > 0 ? ` · ${deck.suspended} suspended` : ''}
                      </span>
                      <ArrowUpRight className="size-3 shrink-0 text-text-faint opacity-0 transition-opacity group-hover:opacity-100" aria-hidden />
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </>
      )}
    </section>
  );
}
