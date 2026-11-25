import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { CircleCheck, FileText, LoaderCircle, RefreshCw } from 'lucide-react';

import { listBrokenLinks, type BrokenLink } from '@/api';
import { InlineFeedback } from '@/components/ui/inline-feedback';
import { ViewEmpty, ViewHeader, ViewIconButton, ViewSectionHeading } from '@/components/ui/view-header';
import {
  brokenLinkSummary,
  groupBrokenLinks,
  linkDiagnosticsMode,
} from '@/lib/link-diagnostics';
import { useI18n } from '@/lib/I18nContext';

interface LinksViewProps {
  workspaceSlug: string;
  onOpenPage: (path: string) => void;
}

export function LinksView({ workspaceSlug, onOpenPage }: LinksViewProps) {
  const { t } = useI18n();
  const [links, setLinks] = useState<BrokenLink[]>([]);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState('');
  const requestEpoch = useRef(0);
  const readingSlug = useRef<string | null>(null);
  const loadedSlug = useRef<string | null>(null);
  const groups = useMemo(() => groupBrokenLinks(links), [links]);
  const mode = linkDiagnosticsMode({ loading, error, count: links.length });

  const refresh = useCallback(async () => {
    if (readingSlug.current === workspaceSlug) return;
    const epoch = requestEpoch.current;
    readingSlug.current = workspaceSlug;
    if (loadedSlug.current !== workspaceSlug) setLoading(true);
    setRefreshing(true);
    try {
      const next = await listBrokenLinks(workspaceSlug);
      if (requestEpoch.current !== epoch) return;
      loadedSlug.current = workspaceSlug;
      setLinks(next);
      setError('');
    } catch (caught) {
      if (requestEpoch.current !== epoch) return;
      setError(caught instanceof Error ? caught.message : 'Could not inspect links');
    } finally {
      if (readingSlug.current === workspaceSlug) readingSlug.current = null;
      if (requestEpoch.current === epoch) {
        setLoading(false);
        setRefreshing(false);
      }
    }
  }, [workspaceSlug]);

  useEffect(() => {
    const initialRead = window.setTimeout(() => { void refresh(); }, 0);
    const refreshOnFocus = () => { void refresh(); };
    window.addEventListener('focus', refreshOnFocus);
    return () => {
      requestEpoch.current += 1;
      window.clearTimeout(initialRead);
      window.removeEventListener('focus', refreshOnFocus);
    };
  }, [refresh]);

  return (
    <section style={{ width: 'min(860px, 100%)' }}>
      <ViewHeader
        title={t('Broken links')}
        description={t('Links to pages that don’t exist yet.')}
        actions={(
          <ViewIconButton label={t('Refresh')} onClick={() => { void refresh(); }} disabled={refreshing}>
            <RefreshCw size={14} className={refreshing ? 'animate-spin' : undefined} />
          </ViewIconButton>
        )}
      />

      {mode === 'loading' ? (
        <ViewEmpty><LoaderCircle size={15} className="animate-spin" /> {t('Checking links…')}</ViewEmpty>
      ) : mode === 'error' ? (
        <InlineFeedback
          title={t('Could not inspect links')}
          description={error}
          action={(
            <button type="button" onClick={() => { void refresh(); }} disabled={refreshing} className="inline-flex items-center gap-1 text-meta font-semibold text-red">
              <RefreshCw size={12} /> {t('Retry')}
            </button>
          )}
        />
      ) : mode === 'clean' ? (
        <ViewEmpty><CircleCheck size={15} className="text-green" aria-hidden /> {t('No broken links.')}</ViewEmpty>
      ) : (
        <div aria-live="polite">
          <ViewSectionHeading meta={brokenLinkSummary(links.length)}>{t('Needs attention')}</ViewSectionHeading>
          <ol className="view-list">
            {groups.map((group) => (
              <li key={group.sourcePath} className="pb-1.5">
                <button type="button" onClick={() => onOpenPage(group.sourcePath)} className="view-row">
                  <span className="view-row-icon"><FileText size={16} aria-hidden /></span>
                  <span className="view-row-title">
                    {group.sourceTitle || group.sourcePath}
                    <span className="view-row-detail">{group.sourcePath}</span>
                  </span>
                </button>
                <ul aria-label={`Missing pages linked from ${group.sourceTitle || group.sourcePath}`} className="m-0 grid list-none gap-0.5 p-0 pl-[34px]">
                  {group.targets.map((target) => (
                    <li key={target} className="flex min-w-0 items-center gap-2 text-small text-text-secondary">
                      <span aria-hidden className="size-1.5 shrink-0 rounded-full bg-red" />
                      <span className="min-w-0 break-words">{target}</span>
                    </li>
                  ))}
                </ul>
              </li>
            ))}
          </ol>
        </div>
      )}
    </section>
  );
}
