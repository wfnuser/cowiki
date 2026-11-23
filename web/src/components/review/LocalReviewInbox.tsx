import { useEffect, useMemo, useState } from 'react';
import { GitBranch, GitCommitHorizontal } from 'lucide-react';

import {
  getLocalWorkingDiff,
  listLocalAgentChanges,
  type AgentChange,
  type FileDiff,
} from '@/api';
import { ReviewInbox, ReviewInboxRow } from './ReviewInbox';
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
    <ReviewInbox
      error={error}
      loading={draftDiffs == null || changes == null}
    >
      {draftDiffs != null && changes != null
        ? rows.map((row, index) => {
          if (row.kind === 'draft') {
            return (
              <ReviewInboxRow
                key={row.id}
                title="Current Draft"
                subtitle="Compared with the latest checkpoint"
                status={draftDiffs.length ? 'Not checkpointed' : 'Clean'}
                files={draftDiffs.length}
                additions={sum(draftDiffs, 'additions')}
                deletions={sum(draftDiffs, 'deletions')}
                first={index === 0}
                icon={<GitCommitHorizontal size={16} className="shrink-0 text-text-faint" />}
                onOpen={() => onOpen(localReviewSelectionForRow(row))}
              />
            );
          }
          const change = row.change;
          return (
            <ReviewInboxRow
              key={row.id}
              title={change.title}
              subtitle={`Agent change · ${change.id.slice(0, 8)}`}
              status={statusLabel(change.status)}
              statusTone={change.status === 'needsResolution' ? 'danger' : 'muted'}
              files={change.diffs.length}
              additions={sum(change.diffs, 'additions')}
              deletions={sum(change.diffs, 'deletions')}
              first={index === 0}
              icon={<GitBranch size={16} className="shrink-0 text-text-faint" />}
              onOpen={() => onOpen(localReviewSelectionForRow(row))}
            />
          );
        })
        : undefined}
    </ReviewInbox>
  );
}

function sum(diffs: FileDiff[], field: 'additions' | 'deletions'): number {
  return diffs.reduce((total, diff) => total + diff[field], 0);
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
