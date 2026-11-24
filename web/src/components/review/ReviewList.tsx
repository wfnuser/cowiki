import { useEffect, useState } from 'react';
import { GitBranch } from 'lucide-react';
import { listReviews, type Submission } from '../../api';
import { InlineFeedback } from '@/components/ui/inline-feedback';
import { ViewEmpty, ViewHeader, ViewTabs } from '@/components/ui/view-header';
import { timeAgo } from '../../lib/time';
import { AvatarBadge } from '@/components/ui/avatar-badge';
import { statusBadge } from '@/lib/review';
import { isDesktopClient } from '@/runtime';
import { LocalReviewInbox } from './LocalReviewInbox';
import type { ReviewTarget } from './review-navigation';

type Filter = 'open' | 'merged' | 'all';

type ReviewListProps = {
  workspaceSlug: string;
  onOpen: (target: ReviewTarget) => void;
  refreshKey?: number;
};

export function ReviewList(props: ReviewListProps) {
  return isDesktopClient()
    ? (
      <LocalReviewInbox
        workspaceSlug={props.workspaceSlug}
        refreshKey={props.refreshKey}
        onOpen={props.onOpen}
      />
    )
    : <CloudReviewList {...props} />;
}

function CloudReviewList({
  workspaceSlug,
  onOpen,
  refreshKey,
}: ReviewListProps) {
  const [subs, setSubs] = useState<Submission[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState<Filter>('open');

  useEffect(() => {
    let cancelled = false;
    listReviews(workspaceSlug)
      .then((s) => {
        if (cancelled) return;
        setError(null);
        setSubs(s);
      })
      .catch((e) => !cancelled && setError(e.message || 'Failed to load reviews'));
    return () => { cancelled = true; };
  }, [workspaceSlug, refreshKey]);

  const filtered = subs?.filter((s) => {
    if (filter === 'open') return s.status === 'pending' || s.status === 'approved' || s.status === 'rejected';
    if (filter === 'merged') return s.status === 'merged';
    return true;
  });

  if (error || subs == null) {
    return (
      <section style={{ width: 'min(860px, 100%)' }}>
        <ViewHeader title="Reviews" />
        {error ? <InlineFeedback title="Could not load Reviews" description={error} /> : <ViewEmpty>Loading…</ViewEmpty>}
      </section>
    );
  }

  const openCount = subs.filter((s) => s.status !== 'merged').length;
  const mergedCount = subs.filter((s) => s.status === 'merged').length;

  return (
    <section style={{ width: 'min(860px, 100%)' }}>
      <ViewHeader title="Reviews" className="mb-4" />
      <ViewTabs
        label="Review filters"
        value={filter}
        onChange={setFilter}
        tabs={[
          { id: 'open', label: 'Open', count: openCount },
          { id: 'merged', label: 'Merged', count: mergedCount },
          { id: 'all', label: 'All', count: subs.length },
        ]}
      />

      {filtered && filtered.length === 0 ? (
        <ViewEmpty>No {filter === 'all' ? '' : `${filter} `}reviews.</ViewEmpty>
      ) : (
        <div className="divide-y divide-border-soft border-y border-border-soft">
          {filtered?.map((s) => {
            const sb = statusBadge[s.status] ?? statusBadge.pending;
            const author = s.author_name || s.user_id.slice(0, 8);
            return (
              <button
                key={s.id}
                type="button"
                onClick={() => onOpen({ kind: 'cloud', submissionId: s.id })}
                className="flex w-full min-w-0 items-center gap-3 px-1 py-2.5 text-left transition-colors hover:bg-bg-hover/60"
              >
                <GitBranch size={16} className="shrink-0 text-text-faint" />
                <div className="min-w-0 flex-1">
                  <div className="truncate text-small font-medium text-text">{s.summary || s.paths.join(', ') || 'Submission'}</div>
                  <div className="mt-px truncate text-meta text-text-tertiary">
                    {author} · {s.paths.length} file{s.paths.length === 1 ? '' : 's'} · {timeAgo(s.created_at)}
                  </div>
                </div>
                <span className="flex shrink-0 items-center gap-1.5 text-meta" style={{ color: sb.fg }}>
                  <span aria-hidden className="size-1.5 rounded-full" style={{ background: sb.fg }} />
                  {sb.label}
                </span>
                <AvatarBadge name={author} size={22} />
              </button>
            );
          })}
        </div>
      )}
    </section>
  );
}

export default ReviewList;
