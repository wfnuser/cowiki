import type { LocalReviewSelection } from './local-review-model';

export type ReviewTarget =
  | LocalReviewSelection
  | { kind: 'cloud'; submissionId: string };

export type ParsedReviewRoute = {
  workspaceSlug: string;
  target: ReviewTarget | null;
};

export function reviewRoute(
  owner: string,
  workspaceSlug: string,
  target?: ReviewTarget | null,
): string {
  const root = `/${encodeURIComponent(owner)}/${encodeURIComponent(workspaceSlug)}/reviews`;
  if (!target) return root;
  if (target.kind === 'local-draft') return `${root}/draft`;
  if (target.kind === 'local-agent') {
    return `${root}/agent/${encodeURIComponent(target.changeId)}`;
  }
  return `${root}/cloud/${encodeURIComponent(target.submissionId)}`;
}

export function parseReviewRoute(
  pathname: string,
  workspaceSlugs: string[],
): ParsedReviewRoute | null {
  const parts = pathname.split('/').filter(Boolean).map(safeDecodeURIComponent);
  const reviewsIndex = parts.lastIndexOf('reviews');
  if (reviewsIndex < 1) return null;

  const workspaceSlug = parts[reviewsIndex - 1];
  if (!workspaceSlugs.includes(workspaceSlug)) return null;
  const detail = parts.slice(reviewsIndex + 1);
  if (detail.length === 0) return { workspaceSlug, target: null };
  if (detail.length === 1 && detail[0] === 'draft') {
    return { workspaceSlug, target: { kind: 'local-draft' } };
  }
  if (detail.length === 2 && detail[0] === 'agent') {
    return {
      workspaceSlug,
      target: { kind: 'local-agent', changeId: detail[1] },
    };
  }
  if (detail.length === 2 && detail[0] === 'cloud') {
    return {
      workspaceSlug,
      target: { kind: 'cloud', submissionId: detail[1] },
    };
  }
  return null;
}

function safeDecodeURIComponent(value: string): string {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}
