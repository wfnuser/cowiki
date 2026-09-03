import { useCallback, useEffect, useState } from 'react';
import { GitBranch, GitCommitHorizontal } from 'lucide-react';

import {
  discardLocalAgentChange,
  getLocalWorkingDiff,
  keepLocalWorkingDiff,
  listLocalAgentChanges,
  mergeLocalAgentChange,
  type AgentChange,
  type FileDiff,
} from '@/api';
import { C, fonts } from '@/lib/design';
import { InlineFeedback } from '@/components/ui/inline-feedback';
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

  return (
    <div>
      <ReviewBackButton onClick={onBack} />

      <div style={{ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', gap: 16, marginTop: 10 }}>
        <div style={{ minWidth: 0 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 9 }}>
            {target.kind === 'local-draft'
              ? <GitCommitHorizontal size={19} color={C.faint} />
              : <GitBranch size={19} color={C.faint} />}
            <h1 style={{ margin: 0, color: C.ink, fontFamily: fonts.serif, fontSize: 28 }}>
              {title}
            </h1>
          </div>
          <p style={{ margin: '6px 0 0 28px', color: C.muted, fontSize: 13 }}>
            {target.kind === 'local-draft'
              ? 'Working tree · compared with the latest checkpoint'
              : `agent/${target.changeId.slice(0, 8)} → Current Draft`}
          </p>
          {target.kind === 'local-agent' && change && (
            <span style={{
              display: 'inline-block',
              margin: '8px 0 0 28px',
              padding: '3px 7px',
              borderRadius: 999,
              background: change.status === 'needsResolution' ? C.redBg : C.panel,
              color: change.status === 'needsResolution' ? C.red : C.muted,
              fontSize: 11.5,
              fontWeight: 650,
            }}>
              {statusLabel(change.status)}
            </span>
          )}
        </div>

        {!loading && !error && target.kind === 'local-draft' && draftDiffs && (
          <ActionButton
            disabled={!draftDiffs.length || pending != null}
            onClick={() => run(
              'checkpoint',
              () => keepLocalWorkingDiff(workspaceSlug, draftDiffs),
            )}
          >
            {pending === 'checkpoint' ? 'Creating…' : 'Create Checkpoint'}
          </ActionButton>
        )}

        {!loading && target.kind === 'local-agent' && change && activeChange && (
          <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap', justifyContent: 'flex-end' }}>
            {onContinueAgent && (
              <ActionButton subtle disabled={pending != null} onClick={() => onContinueAgent(change)}>
                Continue with Agent
              </ActionButton>
            )}
            <ActionButton
              disabled={pending != null}
              onClick={() => run(
                'merge',
                () => mergeLocalAgentChange(workspaceSlug, change.id),
                true,
              )}
            >
              {pending === 'merge' ? 'Merging…' : 'Merge into Draft'}
            </ActionButton>
            <ActionButton
              subtle
              disabled={pending != null}
              onClick={() => run(
                'discard',
                () => discardLocalAgentChange(workspaceSlug, change.id),
              )}
            >
              {pending === 'discard' ? 'Discarding…' : 'Discard'}
            </ActionButton>
          </div>
        )}
      </div>

      {error && <InlineFeedback className="mt-[18px]" title="Could not update this Review" description={error} />}
      {loading || diffs == null ? (
        <p style={{ color: C.muted, fontSize: 14, marginTop: 24 }}>Loading changes…</p>
      ) : (
        <div style={{ marginTop: 24 }}>
          {diffs.length ? (
            <DiffView diffs={diffs} />
          ) : (
            <div style={{ padding: 28, border: `1px solid ${C.line}`, borderRadius: 10, color: C.muted, textAlign: 'center' }}>
              No file changes.
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function ActionButton({
  children,
  disabled,
  onClick,
  subtle = false,
}: {
  children: React.ReactNode;
  disabled?: boolean;
  onClick: () => void;
  subtle?: boolean;
}) {
  return (
    <button
      type="button"
      disabled={disabled}
      onClick={onClick}
      style={{
        border: subtle ? `1px solid ${C.line}` : 'none',
        borderRadius: 7,
        padding: '8px 11px',
        background: subtle ? C.panel : C.ink,
        color: subtle ? C.muted : C.onAccent,
        cursor: disabled ? 'default' : 'pointer',
        fontSize: 12.5,
        fontWeight: 650,
        opacity: disabled ? 0.55 : 1,
        whiteSpace: 'nowrap',
      }}
    >
      {children}
    </button>
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
