import { ChevronRight } from 'lucide-react';

import { InlineFeedback } from '@/components/ui/inline-feedback';
import { ViewEmpty, ViewHeader } from '@/components/ui/view-header';
import { useI18n } from '@/lib/I18nContext';
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
  const { t } = useI18n();
  return (
    <section style={{ width: 'min(860px, 100%)' }}>
      <ViewHeader title={t('Reviews')} description={description} actions={action} />

      {error && <InlineFeedback className="mb-4" title={t('Could not load Reviews')} description={error} />}
      {loading ? (
        <ViewEmpty>{t('Loading…')}</ViewEmpty>
      ) : children ? (
        <ul aria-label={t('Reviews')} className="view-list">{children}</ul>
      ) : (
        <ViewEmpty>{empty ?? t('No reviews yet.')}</ViewEmpty>
      )}
    </section>
  );
}

/** One change to review: what it is, then how big it is; a status only when it asks for something. */
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
  /** Kept for callers; rows are separated by space, not rules. */
  first?: boolean;
  icon: React.ReactNode;
  onOpen: () => void;
  /** Leave out the ordinary state ("Open"); say only what differs. */
  status?: string;
  statusTone?: 'muted' | 'danger';
  /** A second thing about the change, shown after its title (an author, a time). */
  subtitle?: string;
  title: string;
  trailing?: React.ReactNode;
}) {
  return (
    <li>
      <button type="button" onClick={onOpen} className="view-row group">
        <span className="view-row-icon">{icon}</span>
        <span className="view-row-title">
          {title}
          {subtitle && <span className="view-row-detail">{subtitle}</span>}
        </span>
        {status && (
          <span className={cn('view-row-meta', statusTone === 'danger' && 'font-medium text-red')}>{status}</span>
        )}
        {files != null && additions != null && deletions != null && (
          <span className="view-row-meta">
            {files} {files === 1 ? 'file' : 'files'}
            <span className={cn('ml-2', additions ? 'text-green' : 'text-text-faint')}>+{additions}</span>
            <span className={cn('ml-1', deletions ? 'text-red' : 'text-text-faint')}>−{deletions}</span>
          </span>
        )}
        {trailing}
        <ChevronRight size={14} className="-mr-1 shrink-0 text-text-faint opacity-0 transition-opacity group-hover:opacity-100" />
      </button>
    </li>
  );
}
