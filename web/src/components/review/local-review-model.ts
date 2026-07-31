export type LocalReviewChangeIdentity = {
  id: string;
  createdAt: number;
};

export type LocalReviewRow<T extends LocalReviewChangeIdentity> =
  | { kind: 'draft'; id: 'current-draft' }
  | { kind: 'agent'; id: string; change: T };

export type LocalReviewSelection =
  | { kind: 'local-draft' }
  | { kind: 'local-agent'; changeId: string };

export function orderedLocalReviewRows<T extends LocalReviewChangeIdentity>(
  changes: T[],
): LocalReviewRow<T>[] {
  const newestFirst = [...changes].sort((left, right) => (
    right.createdAt - left.createdAt || right.id.localeCompare(left.id)
  ));
  return [
    { kind: 'draft', id: 'current-draft' },
    ...newestFirst.map((change) => ({ kind: 'agent' as const, id: change.id, change })),
  ];
}

export function localReviewSelectionForRow<T extends LocalReviewChangeIdentity>(
  row: LocalReviewRow<T>,
): LocalReviewSelection {
  return row.kind === 'draft'
    ? { kind: 'local-draft' }
    : { kind: 'local-agent', changeId: row.change.id };
}

export type LocalReviewAction = 'commit' | 'merge' | 'discard';

export function localReviewActionRefreshesDraft(action: LocalReviewAction): boolean {
  return action === 'merge';
}

export function agentMergeResult(status: 'merged' | 'needsResolution'): {
  draftChanged: boolean;
  message: string | null;
} {
  if (status === 'needsResolution') {
    return {
      draftChanged: false,
      message: 'Merge needs resolution. Current Draft was left unchanged. Continue with Agent to resolve it.',
    };
  }
  return { draftChanged: true, message: null };
}
