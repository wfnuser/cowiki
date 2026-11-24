import { useState, type ReactNode } from 'react';
import { FileText } from 'lucide-react';

import { ViewEmpty, ViewSectionHeading } from '@/components/ui/view-header';
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
