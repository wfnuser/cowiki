import { useCallback, useEffect, useState } from 'react';
import { BookmarkPlus, LoaderCircle } from 'lucide-react';

import {
  createLocalCheckpoint,
  getSpaceHistory,
  type SpaceHistory,
} from '@/api';
import { Button } from '@/components/ui/button';
import { InlineFeedback } from '@/components/ui/inline-feedback';
import { ViewEmpty, ViewHeader } from '@/components/ui/view-header';
import {
  AUTO_CHECKPOINT_LABEL,
  canCreateCheckpoint,
  checkpointTime,
  defaultCheckpointName,
  draftChangeLabel,
} from '@/lib/history';
import { cn } from '@/lib/utils';

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

  // The view's one action, in the title row, and only when there is something to record.
  const checkpointButton = local && (creating || checkpointReady) && (
    <Button
      type="button"
      size="sm"
      variant="ghost"
      onClick={() => { void createCheckpoint(); }}
      disabled={creating || loading}
      title="Record the current Draft as a named snapshot"
      className="text-text-secondary hover:text-text"
    >
      {creating ? <LoaderCircle size={14} className="animate-spin" /> : <BookmarkPlus size={14} />}
      {creating ? 'Creating…' : 'Create checkpoint'}
    </Button>
  );
  const now = new Date();

  return (
    <section style={{ width: 'min(860px, 100%)' }}>
      <ViewHeader title="History" actions={checkpointButton || undefined} />

      {!local ? (
        <InlineFeedback tone="neutral" compact title="History is kept for local Spaces only." />
      ) : (
        <>
          {error && (
            <InlineFeedback
              className="mb-4"
              title="Could not update history"
              description={error}
              action={<button type="button" onClick={() => { void refresh(); }} className="text-meta font-semibold text-red">Retry</button>}
            />
          )}

          {loading && !spaceHistory ? (
            <ViewEmpty><LoaderCircle size={15} className="animate-spin" /> Loading…</ViewEmpty>
          ) : spaceHistory && (
            <>
              <ol aria-label="History" className="history-timeline">
                <li>
                  <span aria-hidden className="history-dot is-draft" />
                  <span className="view-row-title">
                    Current Draft
                    <span className="view-row-detail">{draftChangeLabel(spaceHistory.currentDraft.changedFiles)}</span>
                  </span>
                </li>
                {spaceHistory.checkpoints.map((checkpoint) => {
                  const created = new Date(checkpoint.createdAt * 1000);
                  const named = checkpoint.name !== AUTO_CHECKPOINT_LABEL;
                  return (
                    <li key={checkpoint.id}>
                      <span aria-hidden className={cn('history-dot', named && 'is-named')} />
                      <span className={cn('view-row-title', named ? 'text-text' : 'text-text-secondary')}>{checkpoint.name}</span>
                      <time
                        dateTime={created.toISOString()}
                        title={created.toLocaleString([], { dateStyle: 'full', timeStyle: 'short' })}
                        className="view-row-meta"
                      >
                        {checkpointTime(created, now)}
                      </time>
                    </li>
                  );
                })}
              </ol>
              {spaceHistory.checkpoints.length === 0 && (
                <p className="m-0 pl-[21px] text-small text-text-tertiary">No checkpoints yet.</p>
              )}
            </>
          )}
        </>
      )}
    </section>
  );
}
