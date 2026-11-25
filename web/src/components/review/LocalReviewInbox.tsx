import { useEffect, useMemo, useState } from 'react';
import { Bot, FilePenLine } from 'lucide-react';

import {
  getLocalWorkingDiff,
  listLocalAgentChanges,
  type AgentChange,
  type FileDiff,
} from '@/api';
import { shortAgo } from '@/lib/agent-session';
import { useI18n } from '@/lib/I18nContext';
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
  const { t, tl, intl } = useI18n();
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
                title={t('Current Draft')}
                status={draftDiffs.length ? undefined : t('No changes')}
                files={draftDiffs.length || undefined}
                additions={sum(draftDiffs, 'additions')}
                deletions={sum(draftDiffs, 'deletions')}
                first={index === 0}
                icon={<FilePenLine size={16} />}
                onOpen={() => onOpen(localReviewSelectionForRow(row))}
              />
            );
          }
          const change = row.change;
          const status = statusLabel(change.status);
          return (
            <ReviewInboxRow
              key={row.id}
              title={change.title}
              subtitle={shortAgo(change.createdAt * 1000, undefined, intl)}
              status={status ? tl(status) : undefined}
              statusTone={change.status === 'needsResolution' ? 'danger' : 'muted'}
              files={change.diffs.length}
              additions={sum(change.diffs, 'additions')}
              deletions={sum(change.diffs, 'deletions')}
              first={index === 0}
              icon={<Bot size={16} />}
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

/** An open change says nothing extra; the others say what happened. */
function statusLabel(status: AgentChange['status']): string | undefined {
  switch (status) {
    case 'needsResolution': return 'Needs resolution';
    case 'merged': return 'Merged';
    case 'discarded': return 'Discarded';
    default: return undefined;
  }
}

function errorMessage(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}
