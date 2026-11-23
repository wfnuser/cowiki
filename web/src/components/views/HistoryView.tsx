import { useCallback, useEffect, useState } from 'react';
import {
  BookmarkPlus,
  FilePenLine,
  GitCommitHorizontal,
  LoaderCircle,
} from 'lucide-react';

import {
  createLocalCheckpoint,
  getSpaceHistory,
  type SpaceHistory,
} from '@/api';
import { Button } from '@/components/ui/button';
import { InlineFeedback } from '@/components/ui/inline-feedback';
import { ViewEmpty, ViewHeader, ViewSectionHeading } from '@/components/ui/view-header';
import { fonts } from '@/lib/design';
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

  const checkpointButton = local && (
    <Button
      type="button"
      size="sm"
      variant="outline"
      onClick={() => { void createCheckpoint(); }}
      disabled={creating || loading || !checkpointReady}
      title={checkpointReady ? 'Record the current Draft as a named snapshot' : 'Save a change first'}
    >
      {creating ? <LoaderCircle size={14} className="animate-spin" /> : <BookmarkPlus size={14} />}
      {creating ? 'Creating…' : 'Create checkpoint'}
    </Button>
  );

  return (
    <section style={{ width: 'min(860px, 100%)' }}>
      <ViewHeader title="History" />

      {!local ? (
        <InlineFeedback tone="neutral" compact title="History is kept for local Spaces only." />
      ) : (
        <>
          <div className="flex items-center gap-3 rounded-lg px-1 py-1">
            <FilePenLine size={16} className="shrink-0 text-accent" aria-hidden />
            <div className="min-w-0 flex-1">
              <p className="text-[14px] font-medium leading-5 text-text">Current Draft</p>
              <p className="text-[12.5px] leading-5 text-text-tertiary">
                {spaceHistory
                  ? draftChangeLabel(spaceHistory.currentDraft.changedFiles, spaceHistory.checkpoints.length > 0)
                  : 'Reading the Draft…'}
              </p>
            </div>
            {checkpointButton}
          </div>

          {error && (
            <InlineFeedback
              className="mt-3.5"
              title="Could not update history"
              description={error}
              action={<button type="button" onClick={() => { void refresh(); }} className="text-[12px] font-semibold text-red">Retry</button>}
            />
          )}

          <div className="mt-8">
            <ViewSectionHeading meta={spaceHistory?.checkpoints.length || undefined}>Checkpoints</ViewSectionHeading>
            {loading ? (
              <ViewEmpty><LoaderCircle size={15} className="animate-spin" /> Loading…</ViewEmpty>
            ) : spaceHistory?.checkpoints.length ? (
              <ol className="m-0 list-none divide-y divide-border-soft border-y border-border-soft p-0">
                {spaceHistory.checkpoints.map((checkpoint) => {
                  const created = new Date(checkpoint.createdAt * 1000);
                  return (
                    <li key={checkpoint.id} className="flex min-w-0 items-center gap-3 px-1 py-2.5">
                      <GitCommitHorizontal size={15} className="shrink-0 text-text-faint" aria-hidden />
                      <span className="min-w-0 flex-1 truncate text-[13.5px] text-text">{checkpoint.name}</span>
                      <time dateTime={created.toISOString()} className="shrink-0 text-[12px] text-text-tertiary">
                        {created.toLocaleString([], { dateStyle: 'medium', timeStyle: 'short' })}
                      </time>
                      <code title={checkpoint.id} className="shrink-0 text-[11px] text-text-faint" style={{ fontFamily: fonts.mono }}>{checkpoint.id.slice(0, 7)}</code>
                    </li>
                  );
                })}
              </ol>
            ) : (
              <ViewEmpty>No checkpoints yet.</ViewEmpty>
            )}
          </div>
        </>
      )}
    </section>
  );
}
