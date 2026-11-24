import { useState, type ReactNode } from 'react';
import { FileText } from 'lucide-react';

import { ViewSectionHeading } from '@/components/ui/view-header';
import { shortAgo } from '@/lib/agent-session';

/** One thing Home asks you to do, e.g. "3 tasks due". Only non-zero ones are passed. */
export interface HomeNeed {
  id: string;
  count: number;
  /** Words after the count: "tasks due", "cards to review". */
  label: string;
  onOpen: () => void;
}

export interface HomeRecentPage {
  path: string;
  title: string;
  /** Unix seconds. */
  modifiedAt: number;
}

interface SpaceHomeProps {
  /** Due tasks, cards to review, changes waiting for review; empty hides the line. */
  needs: HomeNeed[];
  /** Today's journal day, written in place (when the Journal module is on). */
  today?: ReactNode;
  /** null when this Space cannot list edit times (a Cloud Space in the browser). */
  recent: HomeRecentPage[] | null;
  onOpenPage: (path: string) => void;
  now?: number;
}

/**
 * Every Space opens here. Small on purpose: one quiet line of what needs you,
 * today's journal to write in, and the pages that changed lately.
 */
export function SpaceHome({ needs, today, recent, onOpenPage, now: fixedNow }: SpaceHomeProps) {
  const [openedAt] = useState(() => Date.now());
  const now = fixedNow ?? openedAt;
  return (
    <div className="space-home">
      {needs.length > 0 && (
        <nav aria-label="Needs you" className="home-needs">
          {needs.map((need, index) => (
            <span key={need.id}>
              {index > 0 && <span aria-hidden className="home-needs-dot">·</span>}
              <button type="button" onClick={need.onOpen}>
                <span className="tabular-nums text-text">{need.count}</span> {need.label}
              </button>
            </span>
          ))}
        </nav>
      )}

      {today}

      {recent && (
        <section aria-label="Recently edited" className="home-recent">
          <ViewSectionHeading>Recently edited</ViewSectionHeading>
          {recent.length === 0 ? (
            <p className="m-0 py-2 text-small text-text-tertiary">No pages yet.</p>
          ) : (
            <ul className="view-list">
              {recent.map((page) => {
                const folder = page.path.split('/').slice(0, -1).join(' / ');
                return (
                  <li key={page.path}>
                    <button type="button" onClick={() => onOpenPage(page.path)} className="view-row">
                      <span className="view-row-icon"><FileText size={16} /></span>
                      <span className="view-row-title">
                        {page.title}
                        {folder && <span className="view-row-detail">{folder}</span>}
                      </span>
                      <span className="view-row-meta">{shortAgo(page.modifiedAt * 1000, now)}</span>
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
