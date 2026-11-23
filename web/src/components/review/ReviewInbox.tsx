import { ChevronRight } from 'lucide-react';

import { InlineFeedback } from '@/components/ui/inline-feedback';
import { ViewEmpty, ViewHeader } from '@/components/ui/view-header';
import { cn } from '@/lib/utils';

export function ReviewInbox({
  action,
  children,
  description,
  empty,
  error,
  loading,
}: {
  action?: React.ReactNode;
  children?: React.ReactNode;
  description?: string;
  empty?: string;
  error?: string | null;
  loading?: boolean;
}) {
  return (
    <section style={{ width: 'min(860px, 100%)' }}>
      <ViewHeader title="Reviews" description={description} actions={action} />

      {error && <InlineFeedback className="mb-4" title="Could not load Reviews" description={error} />}
      {loading ? (
        <ViewEmpty>Loading…</ViewEmpty>
      ) : children ? (
        <div className="divide-y divide-border-soft border-y border-border-soft">{children}</div>
      ) : (
        <ViewEmpty>{empty ?? 'No reviews yet.'}</ViewEmpty>
      )}
    </section>
  );
}

export function ReviewInboxRow({
  additions,
  deletions,
  files,
  icon,
  onOpen,
  status,
  statusTone = 'muted',
  subtitle,
  title,
  trailing,
}: {
  additions?: number;
  deletions?: number;
  files?: number;
  /** Kept for callers; rows are separated by the list's hairlines. */
  first?: boolean;
  icon: React.ReactNode;
  onOpen: () => void;
  status: string;
  statusTone?: 'muted' | 'danger';
  subtitle: string;
  title: string;
  trailing?: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onOpen}
      className="group flex w-full min-w-0 items-center gap-3 px-1 py-2.5 text-left transition-colors hover:bg-bg-hover/60"
    >
      {icon}
      <div className="min-w-0 flex-1">
        <div className="truncate text-[13.5px] font-medium text-text">{title}</div>
        <div className="mt-px truncate text-[12px] text-text-tertiary">{subtitle}</div>
      </div>
      {files != null && additions != null && deletions != null && (
        <span className="whitespace-nowrap text-[12px] tabular-nums text-text-tertiary">
          {files} file{files === 1 ? '' : 's'}{' '}
          <span className="text-green">+{additions}</span>{' '}
          <span className="text-red">−{deletions}</span>
        </span>
      )}
      <span className={cn('whitespace-nowrap text-[12px]', statusTone === 'danger' ? 'font-medium text-red' : 'text-text-tertiary')}>
        {status}
      </span>
      {trailing}
      <ChevronRight size={14} className="shrink-0 text-text-faint opacity-0 transition-opacity group-hover:opacity-100" />
    </button>
  );
}
