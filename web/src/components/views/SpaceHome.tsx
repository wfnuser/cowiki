import { useState } from 'react';
import { Bot, FilePenLine, FileText, GitPullRequest } from 'lucide-react';

import { ViewEmpty, ViewHeader, ViewSectionHeading } from '@/components/ui/view-header';
import { shortAgo } from '@/lib/agent-session';

export interface HomeReview {
  id: string;
  title: string;
  /** One short line: who and how much, e.g. "Agent · 3 files". */
  meta: string;
  agent: boolean;
  onOpen: () => void;
}

export interface HomeRecentPage {
  path: string;
  title: string;
  /** Unix seconds. */
  modifiedAt: number;
}

interface SpaceHomeProps {
  name: string;
  reviews: HomeReview[];
  /** Pending reviews counted but not listed here (Cloud submissions). */
  moreReviews?: number;
  onSeeReviews?: () => void;
  /** null when this Space cannot list edit times (a Cloud Space in the browser). */
  recent: HomeRecentPage[] | null;
  onOpenPage: (path: string) => void;
  now?: number;
}

/**
 * A team Space's start page: what is waiting for someone to review, then what
 * changed lately. Like the overview of a Confluence space or a GitHub
 * repository, it is a place to start, not a page anyone writes.
 */
export function SpaceHome({
  name, reviews, moreReviews = 0, onSeeReviews, recent, onOpenPage, now: fixedNow,
}: SpaceHomeProps) {
  const [openedAt] = useState(() => Date.now());
  const now = fixedNow ?? openedAt;
  const waiting = reviews.length + moreReviews;
  return (
    <div style={{ width: 'min(760px, 100%)' }}>
      <ViewHeader title={name} />

      <section aria-label="Waiting for review" className="mb-9">
        <ViewSectionHeading meta={waiting || undefined}>Waiting for review</ViewSectionHeading>
        {waiting === 0 ? (
          <ViewEmpty className="justify-start px-1 py-3 text-left">Nothing is waiting for review.</ViewEmpty>
        ) : (
          <ul className="m-0 list-none divide-y divide-border-soft border-y border-border-soft p-0">
            {reviews.map((review) => (
              <li key={review.id}>
                <button type="button" onClick={review.onOpen} className="home-row">
                  <span className="home-row-icon">{review.agent ? <Bot size={15} /> : <FilePenLine size={15} />}</span>
                  <span className="min-w-0 flex-1 truncate text-text">{review.title}</span>
                  <span className="shrink-0 text-[12.5px] text-text-tertiary">{review.meta}</span>
                </button>
              </li>
            ))}
            {moreReviews > 0 && onSeeReviews && (
              <li>
                <button type="button" onClick={onSeeReviews} className="home-row">
                  <span className="home-row-icon"><GitPullRequest size={15} /></span>
                  <span className="min-w-0 flex-1 truncate text-text">
                    {moreReviews} {moreReviews === 1 ? 'submission' : 'submissions'} from members
                  </span>
                  <span className="shrink-0 text-[12.5px] text-text-tertiary">Open Reviews</span>
                </button>
              </li>
            )}
          </ul>
        )}
      </section>

      {recent && (
        <section aria-label="Recently edited">
          <ViewSectionHeading>Recently edited</ViewSectionHeading>
          {recent.length === 0 ? (
            <ViewEmpty className="justify-start px-1 py-3 text-left">No pages yet.</ViewEmpty>
          ) : (
            <ul className="m-0 list-none divide-y divide-border-soft border-y border-border-soft p-0">
              {recent.map((page) => {
                const folder = page.path.split('/').slice(0, -1).join(' / ');
                return (
                  <li key={page.path}>
                    <button type="button" onClick={() => onOpenPage(page.path)} className="home-row">
                      <span className="home-row-icon"><FileText size={15} /></span>
                      <span className="min-w-0 flex-1 truncate">
                        <span className="text-text">{page.title}</span>
                        {folder && <span className="ml-2 text-[12.5px] text-text-tertiary">{folder}</span>}
                      </span>
                      <span className="shrink-0 text-[12.5px] tabular-nums text-text-tertiary">
                        {shortAgo(page.modifiedAt * 1000, now)}
                      </span>
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </section>
      )}
    </div>
  );
}
