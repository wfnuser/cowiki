export type LocalReviewChangeIdentity = {
  id: string;
  createdAt: number;
};

export type LocalReviewRow<T extends LocalReviewChangeIdentity> =
  | { kind: 'draft'; id: 'current-draft' }
  | { kind: 'agent'; id: string; change: T };

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
