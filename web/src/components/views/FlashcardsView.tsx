import { useEffect, useMemo, useState } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { ExternalLink, Layers, LoaderCircle, RefreshCw, RotateCcw } from 'lucide-react';

import { withObsidianReader } from '@/components/ObsidianReader';
import { listCardSources, readCardReviews, writeCardReviews } from '@/local-api';
import { C } from '@/lib/design';
import { obsidianToReaderMarkdown } from '@/lib/obsidian-markdown';
import {
  buildQueue,
  collectCards,
  deckStats,
  isNew,
  parseOfMatrix,
  previewIntervals,
  renderCloze,
  review,
  writeSchedule,
  type CardSchedule,
  type Flashcard,
  type OfMatrix,
  type Score,
} from '@/lib/flashcards';

const MARKDOWN = withObsidianReader(undefined);
// Logseq's answers and shortcuts (f / t / r), with 1-3 as well.
const SCORES: Array<{ score: Score; label: string; keys: string[]; tone: string }> = [
  { score: 1, label: 'Forgotten', keys: ['f', '1'], tone: C.red },
  { score: 3, label: 'Took a while', keys: ['t', '2'], tone: C.line },
  { score: 5, label: 'Remembered', keys: ['r', '3'], tone: C.green },
];

interface FlashcardsViewProps {
  workspaceSlug: string;
  onOpenSource: (path: string) => void;
  /** Re-read a page, transform it and save it. */
  onEditPage: (path: string, transform: (content: string) => string) => Promise<void>;
}

function CardText({ text }: { text: string }) {
  return (
    <div className="page-prose" style={{ fontSize: 16, lineHeight: 1.6, color: C.ink, overflowWrap: 'anywhere' }}>
      <ReactMarkdown remarkPlugins={[remarkGfm]} components={MARKDOWN}>{obsidianToReaderMarkdown(text)}</ReactMarkdown>
    </div>
  );
}

export function FlashcardsView({ workspaceSlug, onOpenSource, onEditPage }: FlashcardsViewProps) {
  const [cards, setCards] = useState<Flashcard[]>([]);
  const [matrix, setMatrix] = useState<OfMatrix>({});
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [reloadKey, setReloadKey] = useState(0);
  const [session, setSession] = useState<Flashcard[] | null>(null);
  const [reviewed, setReviewed] = useState(0);
  const [revealed, setRevealed] = useState(false);
  const [saving, setSaving] = useState(false);
  const [now, setNow] = useState(() => new Date());

  useEffect(() => {
    let cancelled = false;
    const timer = window.setTimeout(() => {
      Promise.all([listCardSources(workspaceSlug), readCardReviews(workspaceSlug)])
        .then(([sources, saved]) => {
          if (cancelled) return;
          setCards(collectCards(sources));
          setMatrix(parseOfMatrix(saved));
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

  const stats = useMemo(() => deckStats(cards, now), [cards, now]);
  const queue = useMemo(() => buildQueue(cards, now), [cards, now]);
  const decks = useMemo(() => {
    const byPage = new Map<string, Flashcard[]>();
    for (const item of cards) byPage.set(item.path, [...(byPage.get(item.path) ?? []), item]);
    return [...byPage.entries()].sort(([left], [right]) => left.localeCompare(right));
  }, [cards]);

  const current = session?.[0] ?? null;

  /** Move to the next card; reload the counts when the session is over. */
  const advance = () => {
    const remaining = session?.slice(1) ?? [];
    setRevealed(false);
    if (remaining.length > 0) {
      setSession(remaining);
      return;
    }
    setSession(null);
    setReloadKey((key) => key + 1);
  };

  /** Save a card's schedule into its note and move to the next card. */
  const save = async (item: Flashcard, schedule: CardSchedule | null, nextMatrix?: OfMatrix) => {
    setSaving(true);
    try {
      await onEditPage(item.path, (content) => writeSchedule(content, item, schedule));
      if (nextMatrix) {
        setMatrix(nextMatrix);
        writeCardReviews(workspaceSlug, { version: 2, ofMatrix: nextMatrix }).catch(() => {
          // The matrix only tunes intervals; the schedule itself is already in the note.
        });
      }
      setReviewed((count) => count + 1);
      setError('');
      advance();
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setSaving(false);
    }
  };

  const rate = (score: Score) => {
    if (!current || saving) return;
    const next = review(current.schedule, score, matrix, new Date());
    void save(current, next.schedule, next.matrix);
  };

  const endSession = () => {
    setSession(null);
    setRevealed(false);
    setReloadKey((key) => key + 1);
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
    const intervals = previewIntervals(current.schedule, matrix);
    const fresh = isNew(current);
    return (
      <section style={{ width: 'min(720px, 100%)', margin: '0 auto' }}>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 14, fontSize: 12.5, color: C.muted }}>
          <span>{session?.length ?? 0} left · {fresh ? 'New' : `Review #${current.schedule.repeats + 1}`}</span>
          <div style={{ display: 'flex', gap: 6 }}>
            <button type="button" onClick={() => onOpenSource(current.path)} style={ghostButton} title={current.path}>
              <ExternalLink size={13} /> Open source
            </button>
            <button
              type="button"
              onClick={() => { void save(current, null); }}
              disabled={saving || fresh}
              style={{ ...ghostButton, opacity: fresh ? 0.5 : 1 }}
              title="Reset this card so that you can review it again from the start."
            >
              <RotateCcw size={13} /> Reset
            </button>
            <button type="button" onClick={endSession} style={ghostButton}>End</button>
          </div>
        </div>
        {error && <div role="alert" style={{ marginBottom: 12, color: C.red, fontSize: 12.5 }}>{error}</div>}
        <div style={cardStyle}>
          <CardText text={renderCloze(current.front, revealed)} />
          {revealed && current.back && (
            <>
              <hr style={{ border: 'none', borderTop: `1px solid ${C.line}`, margin: '18px 0' }} />
              <CardText text={renderCloze(current.back, true)} />
            </>
          )}
        </div>
        <div style={{ display: 'flex', justifyContent: 'center', gap: 8, marginTop: 16 }}>
          {!revealed ? (
            <>
              <button type="button" onClick={() => setRevealed(true)} style={primaryButton}>
                {current.cloze && !current.back ? 'Show clozes' : 'Show answer'} <kbd style={kbd}>Space</kbd>
              </button>
              <button type="button" onClick={advance} style={ghostButton}>
                Next <kbd style={{ ...kbd, borderColor: C.line }}>n</kbd>
              </button>
            </>
          ) : SCORES.map((item) => (
            <button key={item.score} type="button" disabled={saving} onClick={() => rate(item.score)} style={{ ...ratingButton, borderColor: item.tone }}>
              <span style={{ fontWeight: 650 }}>{item.label}</span>
              <span style={{ fontSize: 11, color: C.muted }}>{intervals[item.score]} · {item.keys[0]}</span>
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
          Any bullet tagged <code>#card</code> is a card and its child bullets are the answer, as in Logseq. Each review is saved in the note itself as <code>card-*::</code> properties.
        </p>
      </header>

      {error && <div role="alert" style={{ marginBottom: 12, color: C.red, fontSize: 12.5 }}>{error}</div>}

      {loading ? (
        <div style={emptyState}><LoaderCircle size={15} className="animate-spin" /> Collecting cards…</div>
      ) : (
        <>
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, minmax(0, 1fr))', gap: 10, marginBottom: 14 }}>
            {([['Due now', stats.due], ['Never reviewed', stats.fresh], ['Total', stats.total]] as Array<[string, number]>).map(([label, value]) => (
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
              onClick={() => { setSession(queue); setReviewed(0); setRevealed(false); }}
              style={{ ...primaryButton, opacity: queue.length === 0 ? 0.5 : 1 }}
            >
              {queue.length === 0 ? 'Nothing due' : `Review now · ${queue.length}`}
            </button>
            {reviewed > 0 && <span style={{ fontSize: 12, color: C.muted }}>{reviewed} reviewed this session</span>}
          </div>

          {decks.length === 0 ? (
            <div style={emptyState}>
              No cards yet. Add <code>#card</code> to any bullet in your notes; its indented child bullets are the answer.
            </div>
          ) : (
            <ul style={{ listStyle: 'none', margin: 0, padding: 0, border: `1px solid ${C.line}`, borderRadius: 10, background: C.panel }}>
              {decks.map(([path, items], index) => {
                const deck = deckStats(items, now);
                return (
                  <li key={path} style={{ display: 'flex', alignItems: 'center', gap: 10, padding: '9px 12px', borderTop: index === 0 ? 'none' : `1px solid ${C.lineSoft}` }}>
                    <button type="button" onClick={() => onOpenSource(path)} style={linkButton} title={path}>
                      {path.replace(/\.md$/i, '')}
                    </button>
                    <span style={{ marginLeft: 'auto', fontSize: 12, color: C.muted, whiteSpace: 'nowrap' }}>
                      {deck.total} {deck.total === 1 ? 'card' : 'cards'} · {deck.due} due
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

const linkButton: React.CSSProperties = {
  padding: 0, border: 'none', background: 'transparent', color: C.accent, fontSize: 13, cursor: 'pointer',
  overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', textAlign: 'left',
};

const emptyState: React.CSSProperties = {
  display: 'flex', alignItems: 'center', justifyContent: 'center', gap: 8, minHeight: 120,
  border: `1px dashed ${C.line}`, borderRadius: 11, color: C.muted, fontSize: 12.5, textAlign: 'center', padding: 16, lineHeight: 1.6,
};
