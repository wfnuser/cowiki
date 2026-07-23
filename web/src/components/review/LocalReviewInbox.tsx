import { useEffect, useMemo, useState } from 'react';
import { ChevronRight, GitBranch, GitCommitHorizontal } from 'lucide-react';

import {
  getLocalWorkingDiff,
  listLocalAgentChanges,
  type AgentChange,
  type FileDiff,
} from '@/api';
import { C } from '@/lib/design';
import {
  localReviewSelectionForRow,
  orderedLocalReviewRows,
} from './local-review-model';
import type { ReviewTarget } from './review-navigation';

type LocalReviewInboxProps = {
  workspaceSlug: string;
  refreshKey?: number;
  onOpen: (target: ReviewTarget) => void;
};

export function LocalReviewInbox({
  workspaceSlug,
  refreshKey,
  onOpen,
}: LocalReviewInboxProps) {
  const [draftDiffs, setDraftDiffs] = useState<FileDiff[] | null>(null);
  const [changes, setChanges] = useState<AgentChange[] | null>(null);
  const [error, setError] = useState<string | null>(null);

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

  return (
    <div>
      <div style={{ marginBottom: 20 }}>
        <h1 className="page-title" style={{ marginBottom: 5 }}>Reviews</h1>
        <p style={{ color: C.muted, fontSize: 13, margin: 0 }}>
          Checkpoint the Current Draft or review isolated Agent Changes.
        </p>
      </div>
      {error && <p style={{ color: C.red, fontSize: 13 }}>{error}</p>}
      {draftDiffs == null || changes == null ? (
        <p style={{ color: C.muted, fontSize: 14 }}>Loading Reviews…</p>
      ) : (
        <div style={{ border: `1px solid ${C.line}`, borderRadius: 12, overflow: 'hidden', background: C.panel }}>
          {rows.map((row, index) => {
            if (row.kind === 'draft') {
              return (
                <ReviewListRow
                  key={row.id}
                  title="Current Draft"
                  subtitle="Working tree · compared with the latest checkpoint"
                  status={draftDiffs.length ? 'Uncheckpointed' : 'Clean'}
                  diffs={draftDiffs}
                  first={index === 0}
                  icon={<GitCommitHorizontal size={17} color={C.faint} />}
                  onOpen={() => onOpen(localReviewSelectionForRow(row))}
                />
              );
            }
            const change = row.change;
            return (
              <ReviewListRow
                key={row.id}
                title={change.title}
                subtitle={`agent/${change.id.slice(0, 8)} → Current Draft`}
                status={statusLabel(change.status)}
                diffs={change.diffs}
                first={index === 0}
                icon={<GitBranch size={17} color={C.faint} />}
                onOpen={() => onOpen(localReviewSelectionForRow(row))}
              />
            );
          })}
        </div>
      )}
    </div>
  );
}

function ReviewListRow({
  diffs,
  first,
  icon,
  onOpen,
  status,
  subtitle,
  title,
}: {
  diffs: FileDiff[];
  first: boolean;
  icon: React.ReactNode;
  onOpen: () => void;
  status: string;
  subtitle: string;
  title: string;
}) {
  const additions = diffs.reduce((total, diff) => total + diff.additions, 0);
  const deletions = diffs.reduce((total, diff) => total + diff.deletions, 0);
  return (
    <button
      type="button"
      onClick={onOpen}
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: 12,
        width: '100%',
        minWidth: 0,
        padding: '15px 17px',
        border: 0,
        borderTop: first ? 'none' : `1px solid ${C.line}`,
        background: C.panel,
        textAlign: 'left',
        cursor: 'pointer',
      }}
      onMouseEnter={(event) => { event.currentTarget.style.background = C.sidebar; }}
      onMouseLeave={(event) => { event.currentTarget.style.background = C.panel; }}
    >
      {icon}
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontSize: 14.5, fontWeight: 650, color: C.ink }}>{title}</div>
        <div style={{ marginTop: 3, fontSize: 12.5, color: C.muted, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
          {subtitle}
        </div>
      </div>
      <span style={{ color: C.muted, fontSize: 12, whiteSpace: 'nowrap' }}>
        {diffs.length} file{diffs.length === 1 ? '' : 's'} ·{' '}
        <span style={{ color: C.green }}>+{additions}</span> ·{' '}
        <span style={{ color: C.red }}>−{deletions}</span>
      </span>
      <span style={{ fontSize: 11.5, fontWeight: 650, color: status === 'Needs resolution' ? C.red : C.muted }}>
        {status}
      </span>
      <ChevronRight size={15} color={C.faint} />
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
