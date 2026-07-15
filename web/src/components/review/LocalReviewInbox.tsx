import { useCallback, useEffect, useMemo, useState } from 'react';
import { ChevronDown, ChevronRight, GitBranch, GitCommitHorizontal } from 'lucide-react';

import {
  discardLocalAgentChange,
  getLocalWorkingDiff,
  keepLocalWorkingDiff,
  listLocalAgentChanges,
  mergeLocalAgentChange,
  type AgentChange,
  type FileDiff,
} from '@/api';
import { C } from '@/lib/design';
import { DiffView } from './DiffView';
import { orderedLocalReviewRows } from './local-review-model';

type LocalReviewInboxProps = {
  workspaceSlug: string;
  refreshKey?: number;
};

export function LocalReviewInbox({ workspaceSlug, refreshKey }: LocalReviewInboxProps) {
  const [draftDiffs, setDraftDiffs] = useState<FileDiff[] | null>(null);
  const [changes, setChanges] = useState<AgentChange[] | null>(null);
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set(['current-draft']));
  const [pendingAction, setPendingAction] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    const [nextDraft, nextChanges] = await Promise.all([
      getLocalWorkingDiff(workspaceSlug),
      listLocalAgentChanges(workspaceSlug),
    ]);
    setDraftDiffs(nextDraft);
    setChanges(nextChanges);
  }, [workspaceSlug]);

  useEffect(() => {
    let cancelled = false;
    Promise.all([getLocalWorkingDiff(workspaceSlug), listLocalAgentChanges(workspaceSlug)])
      .then(([nextDraft, nextChanges]) => {
        if (cancelled) return;
        setError(null);
        setDraftDiffs(nextDraft);
        setChanges(nextChanges);
      })
      .catch((cause) => {
        if (!cancelled) setError(errorMessage(cause));
      });
    return () => { cancelled = true; };
  }, [refreshKey, workspaceSlug]);

  const rows = useMemo(() => orderedLocalReviewRows(changes ?? []), [changes]);
  const toggle = (id: string) => {
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const runAction = async (id: string, action: () => Promise<unknown>) => {
    if (pendingAction) return;
    setPendingAction(id);
    setError(null);
    try {
      await action();
      await reload();
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setPendingAction(null);
    }
  };

  return (
    <div>
      <div style={{ marginBottom: 20 }}>
        <h1 className="page-title" style={{ marginBottom: 5 }}>Reviews</h1>
        <p style={{ color: C.muted, fontSize: 13, margin: 0 }}>
          Review and commit the Current Draft, or merge isolated Agent Changes into its working tree.
        </p>
      </div>
      {error && <p style={{ color: C.red, fontSize: 13 }}>{error}</p>}
      {draftDiffs == null || changes == null ? (
        <p style={{ color: C.muted, fontSize: 14 }}>Loading Reviews…</p>
      ) : (
        <div style={{ display: 'grid', gap: 10 }}>
          {rows.map((row) => {
            if (row.kind === 'draft') {
              return (
                <ReviewRow
                  key={row.id}
                  id={row.id}
                  title="Current Draft"
                  subtitle="Local working tree · relative to HEAD"
                  status={draftDiffs.length ? 'Uncommitted' : 'Clean'}
                  diffs={draftDiffs}
                  expanded={expanded.has(row.id)}
                  onToggle={() => toggle(row.id)}
                  icon={<GitCommitHorizontal size={17} color={C.faint} />}
                  actions={(
                    <ActionButton
                      disabled={!draftDiffs.length || pendingAction != null}
                      onClick={() => runAction(row.id, () => keepLocalWorkingDiff(workspaceSlug, draftDiffs))}
                    >
                      {pendingAction === row.id ? 'Committing…' : 'Commit Draft'}
                    </ActionButton>
                  )}
                />
              );
            }
            const change = row.change;
            const active = change.status === 'open' || change.status === 'needsResolution';
            return (
              <ReviewRow
                key={row.id}
                id={row.id}
                title={change.title}
                subtitle={`Agent Change · ${new Date(change.createdAt * 1000).toLocaleString()}`}
                status={statusLabel(change.status)}
                diffs={change.diffs}
                expanded={expanded.has(row.id)}
                onToggle={() => toggle(row.id)}
                icon={<GitBranch size={17} color={C.faint} />}
                actions={active ? (
                  <>
                    <ActionButton
                      disabled={pendingAction != null}
                      onClick={() => runAction(row.id, () => mergeLocalAgentChange(workspaceSlug, change.id))}
                    >
                      {pendingAction === row.id ? 'Working…' : 'Merge into Draft'}
                    </ActionButton>
                    <ActionButton
                      subtle
                      disabled={pendingAction != null}
                      onClick={() => runAction(row.id, () => discardLocalAgentChange(workspaceSlug, change.id))}
                    >
                      Discard
                    </ActionButton>
                  </>
                ) : undefined}
              />
            );
          })}
        </div>
      )}
    </div>
  );
}

function ReviewRow({
  actions,
  diffs,
  expanded,
  icon,
  id,
  onToggle,
  status,
  subtitle,
  title,
}: {
  actions?: React.ReactNode;
  diffs: FileDiff[];
  expanded: boolean;
  icon: React.ReactNode;
  id: string;
  onToggle: () => void;
  status: string;
  subtitle: string;
  title: string;
}) {
  const additions = diffs.reduce((total, diff) => total + diff.additions, 0);
  const deletions = diffs.reduce((total, diff) => total + diff.deletions, 0);
  return (
    <section style={{ border: `1px solid ${C.line}`, borderRadius: 10, overflow: 'hidden', background: C.panel }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 12, padding: '14px 16px' }}>
        <button
          type="button"
          aria-controls={`${id}-diff`}
          aria-expanded={expanded}
          onClick={onToggle}
          style={{ display: 'flex', flex: 1, minWidth: 0, alignItems: 'center', gap: 12, border: 0, padding: 0, background: 'transparent', textAlign: 'left', cursor: 'pointer' }}
        >
          {expanded ? <ChevronDown size={15} color={C.faint} /> : <ChevronRight size={15} color={C.faint} />}
          {icon}
          <div style={{ flex: 1, minWidth: 0 }}>
            <div style={{ fontSize: 14.5, fontWeight: 650, color: C.ink }}>{title}</div>
            <div style={{ marginTop: 3, fontSize: 12.5, color: C.muted }}>{subtitle}</div>
          </div>
          <DiffSummary diffs={diffs} additions={additions} deletions={deletions} />
          <span style={{ fontSize: 11.5, fontWeight: 650, color: status === 'Needs resolution' ? C.red : C.muted }}>
            {status}
          </span>
        </button>
        {actions && <div style={{ display: 'flex', gap: 7 }}>{actions}</div>}
      </div>
      {expanded && (
        <div id={`${id}-diff`} style={{ borderTop: `1px solid ${C.line}`, padding: diffs.length ? 14 : 20 }}>
          {diffs.length ? (
            <DiffView diffs={diffs} />
          ) : (
            <div style={{ color: C.muted, fontSize: 13, textAlign: 'center' }}>No file changes.</div>
          )}
        </div>
      )}
    </section>
  );
}

function DiffSummary({
  additions,
  deletions,
  diffs,
}: {
  additions: number;
  deletions: number;
  diffs: FileDiff[];
}) {
  return (
    <span style={{ color: C.muted, fontSize: 12, whiteSpace: 'nowrap' }}>
      {diffs.length} file{diffs.length === 1 ? '' : 's'} · <span style={{ color: C.green }}>+{additions}</span> · <span style={{ color: C.red }}>−{deletions}</span>
    </span>
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
        padding: '7px 10px',
        background: subtle ? C.panel : C.ink,
        color: subtle ? C.muted : '#fff',
        cursor: disabled ? 'default' : 'pointer',
        fontSize: 12,
        fontWeight: 650,
        opacity: disabled ? 0.55 : 1,
        whiteSpace: 'nowrap',
      }}
    >
      {children}
    </button>
  );
}

function statusLabel(status: AgentChange['status']): string {
  switch (status) {
    case 'needsResolution': return 'Needs resolution';
    case 'merged': return 'Merged';
    case 'discarded': return 'Discarded';
    default: return 'Open';
  }
}

function errorMessage(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}
