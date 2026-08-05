import { useCallback, useEffect, useState } from 'react';
import {
  BookmarkPlus,
  CircleAlert,
  FilePenLine,
  GitCommitHorizontal,
  History,
  LoaderCircle,
} from 'lucide-react';

import {
  createLocalCheckpoint,
  getSpaceHistory,
  type SpaceHistory,
} from '@/api';
import { Button } from '@/components/ui/button';
import { InlineFeedback } from '@/components/ui/inline-feedback';
import { C } from '@/lib/design';
import {
  canCreateCheckpoint,
  defaultCheckpointName,
  draftChangeLabel,
} from '@/lib/history';

interface HistoryViewProps {
  workspaceSlug: string;
  local: boolean;
}

export function HistoryView({ workspaceSlug, local }: HistoryViewProps) {
  const [spaceHistory, setSpaceHistory] = useState<SpaceHistory | null>(null);
  const [loading, setLoading] = useState(local);
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState('');
  const checkpointReady = canCreateCheckpoint(spaceHistory?.currentDraft.changedFiles);

  const refresh = useCallback(async () => {
    if (!local) return;
    setLoading(true);
    setError('');
    try {
      setSpaceHistory(await getSpaceHistory(workspaceSlug));
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : 'Could not load Space history');
    } finally {
      setLoading(false);
    }
  }, [local, workspaceSlug]);

  useEffect(() => {
    const task = window.setTimeout(() => { void refresh(); }, 0);
    return () => window.clearTimeout(task);
  }, [refresh]);

  const createCheckpoint = async () => {
    setCreating(true);
    setError('');
    try {
      await createLocalCheckpoint(workspaceSlug, defaultCheckpointName());
      setSpaceHistory(await getSpaceHistory(workspaceSlug));
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : 'Could not create checkpoint');
    } finally {
      setCreating(false);
    }
  };

  return (
    <section style={{ width: 'min(860px, 100%)' }}>
      <header style={{ marginBottom: 30 }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 8 }}>
          <History size={20} color={C.accent} strokeWidth={1.8} />
          <h1 className="page-title page-title--compact" style={{ margin: 0 }}>History</h1>
        </div>
        <p style={{ margin: 0, maxWidth: 590, color: C.muted, fontSize: 13.5, lineHeight: 1.65 }}>
          Your Space stays in one editable Draft. A checkpoint records a named local Git snapshot
          without taking you out of that Draft.
        </p>
      </header>

      {!local ? (
        <div style={noticeStyle}>
          <CircleAlert size={18} color={C.amber} />
          <div>
            <strong style={{ display: 'block', color: C.ink2, fontSize: 13.5, marginBottom: 3 }}>
              Local history only
            </strong>
            <span style={{ color: C.muted, fontSize: 12.5, lineHeight: 1.55 }}>
              Checkpoints are stored in the Space&apos;s local Git repository. Cloud history is not enabled.
            </span>
          </div>
        </div>
      ) : (
        <>
          <article style={draftCardStyle}>
            <div style={{ display: 'flex', gap: 14, minWidth: 0, alignItems: 'flex-start' }}>
              <div style={draftIconStyle}><FilePenLine size={19} /></div>
              <div style={{ minWidth: 0 }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}>
                  <h2 style={{ margin: 0, color: C.ink, fontFamily: 'var(--font-serif)', fontSize: 19, fontWeight: 650 }}>
                    Current Draft
                  </h2>
                  <span style={currentBadgeStyle}>Live</span>
                </div>
                <p style={{ margin: '5px 0 0', color: C.ink2, fontSize: 12.5 }}>
                  {spaceHistory
                    ? draftChangeLabel(spaceHistory.currentDraft.changedFiles, spaceHistory.checkpoints.length > 0)
                    : 'Reading saved Draft…'}
                </p>
                <p style={{ margin: '8px 0 0', color: C.muted, fontSize: 12, lineHeight: 1.55 }}>
                  Auto Save updates this Draft only. It never creates a checkpoint.
                </p>
              </div>
            </div>
            <Button
              type="button"
              onClick={() => { void createCheckpoint(); }}
              disabled={creating || loading || !checkpointReady}
              title={checkpointReady ? 'Record the current Draft as a local Git snapshot' : 'Save a change before creating another checkpoint'}
              style={{ flexShrink: 0 }}
            >
              {creating
                ? <><LoaderCircle size={14} className="animate-spin" /> Creating…</>
                : checkpointReady
                  ? <><BookmarkPlus size={14} /> Create checkpoint</>
                  : <><BookmarkPlus size={14} /> No new changes</>}
            </Button>
          </article>

          {error && (
            <InlineFeedback
              className="mt-3.5"
              title="Could not update history"
              description={error}
              action={<button type="button" onClick={() => { void refresh(); }} style={retryStyle}>Retry</button>}
            />
          )}

          <div style={{ marginTop: 30 }}>
            <div style={{ display: 'flex', alignItems: 'baseline', justifyContent: 'space-between', marginBottom: 14 }}>
              <h2 style={{ margin: 0, color: C.ink2, fontSize: 12, fontWeight: 700, letterSpacing: '0.08em', textTransform: 'uppercase' }}>
                Checkpoints
              </h2>
              {spaceHistory && (
                <span style={{ color: C.faint, fontSize: 11.5 }}>{spaceHistory.checkpoints.length} total</span>
              )}
            </div>

            {loading ? (
              <div style={emptyStyle}><LoaderCircle size={17} className="animate-spin" /> Loading history…</div>
            ) : spaceHistory?.checkpoints.length ? (
              <ol style={{ display: 'grid', gap: 10, margin: 0, padding: 0, listStyle: 'none' }}>
                {spaceHistory.checkpoints.map((checkpoint) => (
                  <li key={checkpoint.id}>
                    <div style={checkpointCardStyle}>
                      <span style={commitIconStyle}><GitCommitHorizontal size={16} /></span>
                      <div style={{ minWidth: 0 }}>
                        <strong style={{ display: 'block', color: C.ink2, fontSize: 13.5, fontWeight: 620, overflow: 'hidden', textOverflow: 'ellipsis' }}>
                          {checkpoint.name}
                        </strong>
                        <time dateTime={new Date(checkpoint.createdAt * 1000).toISOString()} style={{ display: 'block', marginTop: 4, color: C.muted, fontSize: 11.5 }}>
                          {new Date(checkpoint.createdAt * 1000).toLocaleString([], { dateStyle: 'medium', timeStyle: 'short' })}
                        </time>
                      </div>
                      <code title={checkpoint.id} style={commitCodeStyle}>{checkpoint.id.slice(0, 8)}</code>
                    </div>
                  </li>
                ))}
              </ol>
            ) : (
              <div style={emptyStyle}>
                No checkpoints yet. Your saved work is still in Current Draft.
              </div>
            )}
          </div>
        </>
      )}
    </section>
  );
}

const draftCardStyle: React.CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
  gap: 22,
  padding: '20px 20px 20px 18px',
  border: `1px solid #edcbb7`,
  borderRadius: 12,
  background: 'linear-gradient(135deg, #fffaf5 0%, #fbeadd 100%)',
};

const draftIconStyle: React.CSSProperties = {
  width: 38,
  height: 38,
  borderRadius: 11,
  display: 'grid',
  placeItems: 'center',
  flexShrink: 0,
  color: C.accent,
  background: '#fff8f2',
  border: '1px solid #f0d4c4',
};

const currentBadgeStyle: React.CSSProperties = {
  padding: '2px 7px',
  borderRadius: 999,
  background: C.accent,
  color: '#fff',
  fontSize: 9.5,
  fontWeight: 750,
  letterSpacing: '0.06em',
  textTransform: 'uppercase',
};

const noticeStyle: React.CSSProperties = {
  display: 'flex',
  alignItems: 'flex-start',
  gap: 10,
  padding: '14px 16px',
  borderRadius: 9,
  border: `1px solid ${C.line}`,
  background: C.amberSoft,
};

const checkpointCardStyle: React.CSSProperties = {
  minWidth: 0,
  minHeight: 62,
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
  gap: 16,
  padding: '12px 14px',
  border: `1px solid ${C.line}`,
  borderRadius: 9,
  background: C.panel,
};

const commitIconStyle: React.CSSProperties = {
  width: 34,
  height: 34,
  display: 'grid',
  placeItems: 'center',
  borderRadius: 999,
  color: C.green,
  background: C.greenSoft,
  border: `1px solid ${C.line}`,
  flexShrink: 0,
};

const commitCodeStyle: React.CSSProperties = {
  flexShrink: 0,
  padding: '3px 7px',
  borderRadius: 5,
  color: C.muted,
  background: C.rail,
  fontSize: 10.5,
  letterSpacing: '0.02em',
};

const emptyStyle: React.CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  gap: 8,
  minHeight: 90,
  padding: 20,
  border: `1px dashed ${C.line}`,
  borderRadius: 9,
  color: C.muted,
  fontSize: 12.5,
  background: 'rgba(253, 252, 251, 0.55)',
};

const retryStyle: React.CSSProperties = {
  border: 0,
  padding: 0,
  background: 'transparent',
  color: C.red,
  fontSize: 12,
  fontWeight: 650,
  cursor: 'pointer',
};
