import { useCallback, useEffect, useState } from 'react';

import {
  discardLocalAgentChange,
  getLocalWorkingDiff,
  keepLocalWorkingDiff,
  listLocalAgentChanges,
  mergeLocalAgentChange,
  type AgentChange,
  type FileDiff,
} from '@/api';
import { Button } from '@/components/ui/button';
import { InlineFeedback } from '@/components/ui/inline-feedback';
import { ViewEmpty, ViewHeader } from '@/components/ui/view-header';
import { DiffView } from './DiffView';
import { ReviewBackButton } from './ReviewBackButton';
import { agentMergeResult, type LocalReviewSelection } from './local-review-model';

export function LocalReviewDetail({
  workspaceSlug,
  target,
  onBack,
  onDraftChanged,
  onReviewsChanged,
  onContinueAgent,
}: {
  workspaceSlug: string;
  target: LocalReviewSelection;
  onBack: () => void;
  onDraftChanged?: () => void;
  onReviewsChanged?: () => void;
  onContinueAgent?: (change: AgentChange) => void;
}) {
  const [draftDiffs, setDraftDiffs] = useState<FileDiff[] | null>(null);
  const [change, setChange] = useState<AgentChange | null>(null);
  const [loading, setLoading] = useState(true);
  const [pending, setPending] = useState<'checkpoint' | 'merge' | 'discard' | null>(null);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    setError(null);
    try {
      if (target.kind === 'local-draft') {
        setDraftDiffs(await getLocalWorkingDiff(workspaceSlug));
        setChange(null);
      } else {
        const changes = await listLocalAgentChanges(workspaceSlug);
        const next = changes.find((candidate) => candidate.id === target.changeId);
        if (!next) throw new Error('Agent Change no longer exists');
        setChange(next);
        setDraftDiffs(null);
      }
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setLoading(false);
    }
  }, [target, workspaceSlug]);

  useEffect(() => {
    const task = window.setTimeout(() => { void reload(); }, 0);
    return () => window.clearTimeout(task);
  }, [reload]);

  const run = async (
    action: NonNullable<typeof pending>,
    operation: () => Promise<unknown>,
    draftChanged = false,
  ) => {
    if (pending) return;
    setPending(action);
    setError(null);
    try {
      const result = await operation();
      await reload();
      onReviewsChanged?.();
      if (action === 'merge') {
        const merge = agentMergeResult(
          (result as AgentChange).status === 'needsResolution' ? 'needsResolution' : 'merged',
        );
        setError(merge.message);
        if (merge.draftChanged) onDraftChanged?.();
      } else if (draftChanged) {
        onDraftChanged?.();
      }
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setPending(null);
    }
  };

  const diffs = target.kind === 'local-draft' ? draftDiffs : change?.diffs;
  const title = target.kind === 'local-draft' ? 'Current Draft' : (change?.title || 'Agent Change');
  const activeChange = change?.status === 'open' || change?.status === 'needsResolution';

  const actions = !loading && !error && target.kind === 'local-draft' && draftDiffs ? (
    <Button
      size="sm"
      disabled={!draftDiffs.length || pending != null}
      onClick={() => run('checkpoint', () => keepLocalWorkingDiff(workspaceSlug, draftDiffs))}
    >
      {pending === 'checkpoint' ? 'Creating…' : 'Create checkpoint'}
    </Button>
  ) : !loading && target.kind === 'local-agent' && change && activeChange ? (
    <>
      <Button
        size="sm"
        variant="ghost"
        disabled={pending != null}
        onClick={() => run('discard', () => discardLocalAgentChange(workspaceSlug, change.id))}
      >
        {pending === 'discard' ? 'Discarding…' : 'Discard'}
      </Button>
      {onContinueAgent && (
        <Button size="sm" variant="outline" disabled={pending != null} onClick={() => onContinueAgent(change)}>
          Continue with Agent
        </Button>
      )}
      <Button
        size="sm"
        disabled={pending != null}
        onClick={() => run('merge', () => mergeLocalAgentChange(workspaceSlug, change.id), true)}
      >
        {pending === 'merge' ? 'Merging…' : 'Merge into Draft'}
      </Button>
    </>
  ) : undefined;

  return (
    <section style={{ width: 'min(1040px, 100%)' }}>
      <ReviewBackButton onClick={onBack} />
      <ViewHeader
        className="mb-5 mt-2"
        title={title}
        actions={actions}
        description={(
          <span className="flex items-center gap-2">
            {target.kind === 'local-draft' ? 'Compared with the latest checkpoint' : `Agent change · ${target.changeId.slice(0, 8)}`}
            {target.kind === 'local-agent' && change && (
              <span className={change.status === 'needsResolution' ? 'font-medium text-red' : 'text-text-tertiary'}>· {statusLabel(change.status)}</span>
            )}
          </span>
        )}
      />

      {error && <InlineFeedback className="mb-4" title="Could not update this Review" description={error} />}
      {loading || diffs == null ? (
        <ViewEmpty>Loading changes…</ViewEmpty>
      ) : diffs.length ? (
        <DiffView diffs={diffs} />
      ) : (
        <ViewEmpty>No file changes.</ViewEmpty>
      )}
    </section>
  );
}

function errorMessage(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}

function statusLabel(status: AgentChange['status']): string {
  switch (status) {
    case 'needsResolution': return 'Needs resolution';
    case 'merged': return 'Merged';
    case 'discarded': return 'Discarded';
    default: return 'Open';
  }
}
