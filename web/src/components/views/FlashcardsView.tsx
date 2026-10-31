import { useEffect, useMemo, useState } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { ExternalLink, Layers, LoaderCircle, PauseCircle, Plus, RefreshCw } from 'lucide-react';

import { withObsidianReader } from '@/components/ObsidianReader';
import { listCardSources, readCardReviews, writeCardReviews } from '@/local-api';
import { C } from '@/lib/design';
import { dateKey } from '@/lib/journal';
import { obsidianToReaderMarkdown } from '@/lib/obsidian-markdown';
import {
  answerCard,
  buildQueue,
  changedSinceReview,
  collectCards,
  deckStats,
  emptyReviewStore,
  parseReviewStore,
  previewIntervals,
  setSuspended,
  type Flashcard,
  type Rating,
  type ReviewStore,
} from '@/lib/flashcards';

const MARKDOWN = withObsidianReader(undefined);
const RATINGS: Array<{ rating: Rating; label: string; key: string }> = [
  { rating: 'again', label: 'Again', key: '1' },
  { rating: 'hard', label: 'Hard', key: '2' },
  { rating: 'good', label: 'Good', key: '3' },
  { rating: 'easy', label: 'Easy', key: '4' },
];

interface FlashcardsViewProps {
  workspaceSlug: string;
  onOpenSource: (path: string) => void;
  /** Append a `front :: back` card to the Space's inbox deck. */
  onAddCard: (front: string, back: string) => Promise<void>;
}

function CardText({ text }: { text: string }) {
  return (
    <div className="page-prose" style={{ fontSize: 16, lineHeight: 1.6, color: C.ink, overflowWrap: 'anywhere' }}>
      <ReactMarkdown remarkPlugins={[remarkGfm]} components={MARKDOWN}>{obsidianToReaderMarkdown(text)}</ReactMarkdown>
    </div>
  );
}

export function FlashcardsView({ workspaceSlug, onOpenSource, onAddCard }: FlashcardsViewProps) {
  const [cards, setCards] = useState<Flashcard[]>([]);
  const [store, setStore] = useState<ReviewStore>(emptyReviewStore);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [reloadKey, setReloadKey] = useState(0);
  const [session, setSession] = useState<Flashcard[] | null>(null);
  const [revealed, setRevealed] = useState(false);
  const [front, setFront] = useState('');
  const [back, setBack] = useState('');
  const [adding, setAdding] = useState(false);
  const today = dateKey(new Date());

  useEffect(() => {
    let cancelled = false;
    const timer = window.setTimeout(() => {
      Promise.all([listCardSources(workspaceSlug), readCardReviews(workspaceSlug)])
        .then(([sources, saved]) => {
          if (cancelled) return;
          setCards(collectCards(sources));
          setStore(parseReviewStore(saved));
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

  const stats = useMemo(() => deckStats(cards, store, today), [cards, store, today]);
  const queue = useMemo(() => buildQueue(cards, store, today), [cards, store, today]);
  const decks = useMemo(() => {
    const byPage = new Map<string, Flashcard[]>();
    for (const item of cards) byPage.set(item.path, [...(byPage.get(item.path) ?? []), item]);
    return [...byPage.entries()].sort(([left], [right]) => left.localeCompare(right));
  }, [cards]);

  const persist = (next: ReviewStore) => {
    setStore(next);
    writeCardReviews(workspaceSlug, next).catch((caught: unknown) => {
      setError(`Could not save review history: ${caught instanceof Error ? caught.message : String(caught)}`);
    });
  };

  const current = session?.[0] ?? null;

  const rate = (rating: Rating) => {
    if (!current || !session) return;
    persist(answerCard(store, current, rating, today));
    // "Again" comes back later in this session.
    setSession(rating === 'again' ? [...session.slice(1), current] : session.slice(1));
    setRevealed(false);
  };

  const suspend = () => {
    if (!current || !session) return;
    persist(setSuspended(store, current, true, today));
    setSession(session.slice(1));
    setRevealed(false);
  };

  useEffect(() => {
    if (!current) return;
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (target && (target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName))) return;
      if (!revealed && (event.key === ' ' || event.key === 'Enter')) {
        event.preventDefault();
        setRevealed(true);
        return;
      }
      const choice = RATINGS.find((item) => item.key === event.key);
      if (revealed && choice) {
        event.preventDefault();
        rate(choice.rating);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  const add = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!front.trim() || !back.trim() || adding) return;
    setAdding(true);
    try {
      await onAddCard(front, back);
      setFront('');
      setBack('');
      setReloadKey((key) => key + 1);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setAdding(false);
    }
  };

  const updateSetting = (key: 'newPerDay' | 'reviewsPerDay', value: string) => {
    const number = Math.max(0, Math.min(9999, Math.round(Number(value) || 0)));
    persist({ ...store, settings: { ...store.settings, [key]: number } });
  };

  if (current) {
    const state = store.cards[current.id];
    const intervals = previewIntervals(state);
    return (
      <section style={{ width: 'min(720px, 100%)', margin: '0 auto' }}>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 14, fontSize: 12.5, color: C.muted }}>
          <span>{session?.length ?? 0} left · {state ? 'Review' : 'New'}</span>
          <div style={{ display: 'flex', gap: 6 }}>
            <button type="button" onClick={() => onOpenSource(current.path)} style={ghostButton} title={current.path}>
              <ExternalLink size={13} /> Open source
            </button>
            <button type="button" onClick={suspend} style={ghostButton}>
              <PauseCircle size={13} /> Suspend
            </button>
            <button type="button" onClick={() => { setSession(null); setRevealed(false); }} style={ghostButton}>End</button>
          </div>
        </div>
        <div style={cardStyle}>
          {changedSinceReview(current, state) && (
            <div style={{ marginBottom: 10, fontSize: 12, color: C.amber }}>The answer changed since you last reviewed this card.</div>
          )}
          <CardText text={current.front} />
          {revealed && (
            <>
              <hr style={{ border: 'none', borderTop: `1px solid ${C.line}`, margin: '18px 0' }} />
              <CardText text={current.back} />
            </>
          )}
        </div>
        <div style={{ display: 'flex', justifyContent: 'center', gap: 8, marginTop: 16 }}>
          {!revealed ? (
            <button type="button" onClick={() => setRevealed(true)} style={primaryButton}>Show answer <kbd style={kbd}>Space</kbd></button>
          ) : RATINGS.map((item) => (
            <button key={item.rating} type="button" onClick={() => rate(item.rating)} style={{ ...ratingButton, borderColor: item.rating === 'again' ? C.red : C.line }}>
              <span style={{ fontWeight: 650 }}>{item.label}</span>
              <span style={{ fontSize: 11, color: C.muted }}>{intervals[item.rating]} · {item.key}</span>
            </button>
          ))}
        </div>
      </section>
    );
  }

  return (
    <section style={{ width: 'min(760px, 100%)', margin: '0 auto' }}>
      <header style={{ marginBottom: 18 }}>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 16, marginBottom: 8 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
            <Layers size={20} color={C.accent} strokeWidth={1.8} />
            <h1 className="page-title page-title--compact" style={{ margin: 0 }}>Flashcards</h1>
          </div>
          <button type="button" onClick={() => setReloadKey((key) => key + 1)} style={ghostButton}>
            <RefreshCw size={13} /> Refresh
          </button>
        </div>
        <p style={{ margin: 0, color: C.muted, fontSize: 13 }}>
          Cards live in your notes; your review history stays on this device and is not shared with the Space.
        </p>
      </header>

      {error && <div role="alert" style={{ marginBottom: 12, color: C.red, fontSize: 12.5 }}>{error}</div>}

      {loading ? (
        <div style={emptyState}><LoaderCircle size={15} className="animate-spin" /> Collecting cards…</div>
      ) : (
        <>
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, minmax(0, 1fr))', gap: 10, marginBottom: 14 }}>
            {([['Due', stats.due], ['New', stats.fresh], ['Total', stats.total], ['Suspended', stats.suspended]] as Array<[string, number]>).map(([label, value]) => (
              <div key={label} style={tile}>
                <div style={{ fontSize: 20, fontWeight: 650, color: C.ink }}>{value}</div>
                <div style={{ fontSize: 11.5, color: C.muted }}>{label}</div>
              </div>
            ))}
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 12, flexWrap: 'wrap', marginBottom: 20 }}>
            <button
              type="button"
              disabled={queue.length === 0}
              onClick={() => { setSession(queue); setRevealed(false); }}
              style={{ ...primaryButton, opacity: queue.length === 0 ? 0.5 : 1 }}
            >
              {queue.length === 0 ? 'Nothing to review today' : `Study now · ${queue.length}`}
            </button>
            <label style={settingLabel}>
              New per day
              <input type="number" min={0} value={store.settings.newPerDay} onChange={(event) => updateSetting('newPerDay', event.target.value)} style={numberInput} />
            </label>
            <label style={settingLabel}>
              Reviews per day
              <input type="number" min={0} value={store.settings.reviewsPerDay} onChange={(event) => updateSetting('reviewsPerDay', event.target.value)} style={numberInput} />
            </label>
            {stats.changed > 0 && <span style={{ fontSize: 12, color: C.amber }}>{stats.changed} changed since last review</span>}
          </div>

          <form onSubmit={add} style={{ ...tile, display: 'grid', gap: 8, marginBottom: 20 }}>
            <div style={{ fontSize: 12, fontWeight: 650, color: C.muted, textTransform: 'uppercase', letterSpacing: '.04em' }}>Add a card</div>
            <input aria-label="Question" placeholder="Question" value={front} onChange={(event) => setFront(event.target.value)} style={textInput} />
            <input aria-label="Answer" placeholder="Answer" value={back} onChange={(event) => setBack(event.target.value)} style={textInput} />
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 8 }}>
              <span style={{ fontSize: 11.5, color: C.muted }}>Saved as <code>Question :: Answer</code> in the page Flashcards.</span>
              <button type="submit" disabled={!front.trim() || !back.trim() || adding} style={primaryButton}>
                <Plus size={13} /> {adding ? 'Adding…' : 'Add'}
              </button>
            </div>
          </form>

          {decks.length === 0 ? (
            <div style={emptyState}>
              No cards yet. Tag a note with #flashcards and write <code>Question :: Answer</code>, or add <code>#card</code> to a bullet whose children hold the answer.
            </div>
          ) : (
            <ul style={{ listStyle: 'none', margin: 0, padding: 0, border: `1px solid ${C.line}`, borderRadius: 10, background: C.panel }}>
              {decks.map(([path, items], index) => {
                const deck = deckStats(items, store, today);
                return (
                  <li key={path} style={{ display: 'flex', alignItems: 'center', gap: 10, padding: '9px 12px', borderTop: index === 0 ? 'none' : `1px solid ${C.lineSoft}` }}>
                    <button type="button" onClick={() => onOpenSource(path)} style={linkButton} title={path}>
                      {path.replace(/\.md$/i, '')}
                    </button>
                    <span style={{ marginLeft: 'auto', fontSize: 12, color: C.muted }}>
                      {deck.total} cards · {deck.due} due · {deck.fresh} new
                    </span>
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

const ghostButton: React.CSSProperties = {
  display: 'inline-flex', alignItems: 'center', gap: 6, padding: '6px 9px',
  border: `1px solid ${C.line}`, borderRadius: 7, background: C.panel,
  color: C.ink2, fontSize: 12, fontWeight: 620, cursor: 'pointer',
};

const primaryButton: React.CSSProperties = {
  display: 'inline-flex', alignItems: 'center', gap: 6, padding: '8px 14px', border: 'none', borderRadius: 8,
  background: C.accent, color: C.onAccent, fontSize: 13, fontWeight: 650, cursor: 'pointer',
};

const ratingButton: React.CSSProperties = {
  display: 'inline-flex', flexDirection: 'column', alignItems: 'center', gap: 2, minWidth: 92, padding: '8px 12px',
  border: `1px solid ${C.line}`, borderRadius: 9, background: C.panel, color: C.ink, fontSize: 13, cursor: 'pointer',
};

const kbd: React.CSSProperties = {
  marginLeft: 6, padding: '0 5px', borderRadius: 4, border: `1px solid color-mix(in srgb, ${C.onAccent} 45%, transparent)`, fontSize: 10.5,
};

const cardStyle: React.CSSProperties = {
  minHeight: 220, padding: '28px 30px', border: `1px solid ${C.line}`, borderRadius: 14, background: C.panel,
};

const tile: React.CSSProperties = {
  padding: '12px 14px', border: `1px solid ${C.line}`, borderRadius: 10, background: C.panel,
};

const settingLabel: React.CSSProperties = {
  display: 'inline-flex', alignItems: 'center', gap: 6, fontSize: 12, color: C.muted,
};

const numberInput: React.CSSProperties = {
  width: 64, padding: '4px 6px', border: `1px solid ${C.line}`, borderRadius: 6, background: C.bg, color: C.ink, fontSize: 12.5,
};

const textInput: React.CSSProperties = {
  width: '100%', padding: '7px 10px', border: `1px solid ${C.line}`, borderRadius: 7, background: C.bg, color: C.ink, fontSize: 13.5,
};

const linkButton: React.CSSProperties = {
  padding: 0, border: 'none', background: 'transparent', color: C.accent, fontSize: 13, cursor: 'pointer',
  overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', textAlign: 'left',
};

const emptyState: React.CSSProperties = {
  display: 'flex', alignItems: 'center', justifyContent: 'center', gap: 8, minHeight: 120,
  border: `1px dashed ${C.line}`, borderRadius: 11, color: C.muted, fontSize: 12.5, textAlign: 'center', padding: 16, lineHeight: 1.6,
};
