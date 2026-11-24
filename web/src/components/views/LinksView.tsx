import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { CircleCheck, FileText, LoaderCircle, RefreshCw } from 'lucide-react';

import { listBrokenLinks, type BrokenLink } from '@/api';
import { InlineFeedback } from '@/components/ui/inline-feedback';
import { ViewEmpty, ViewHeader, ViewIconButton, ViewSectionHeading } from '@/components/ui/view-header';
import { fonts } from '@/lib/design';
import {
  brokenLinkSummary,
  groupBrokenLinks,
  linkDiagnosticsMode,
} from '@/lib/link-diagnostics';

interface LinksViewProps {
  workspaceSlug: string;
  onOpenPage: (path: string) => void;
}

export function LinksView({ workspaceSlug, onOpenPage }: LinksViewProps) {
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
        title="Links"
        description="Internal links whose target page doesn’t exist."
        actions={(
          <ViewIconButton label="Refresh" onClick={() => { void refresh(); }} disabled={refreshing}>
            <RefreshCw size={14} className={refreshing ? 'animate-spin' : undefined} />
          </ViewIconButton>
        )}
      />

      {mode === 'loading' ? (
        <ViewEmpty><LoaderCircle size={15} className="animate-spin" /> Checking links…</ViewEmpty>
      ) : mode === 'error' ? (
        <InlineFeedback
          title="Could not inspect links"
          description={error}
          action={(
            <button type="button" onClick={() => { void refresh(); }} disabled={refreshing} className="inline-flex items-center gap-1 text-[12px] font-semibold text-red">
              <RefreshCw size={12} /> Retry
            </button>
          )}
        />
      ) : mode === 'clean' ? (
        <ViewEmpty><CircleCheck size={15} className="text-green" aria-hidden /> No broken links.</ViewEmpty>
      ) : (
        <div aria-live="polite">
          <ViewSectionHeading meta={brokenLinkSummary(links.length)}>Needs attention</ViewSectionHeading>
          <ol className="m-0 list-none divide-y divide-border-soft border-y border-border-soft p-0">
            {groups.map((group) => (
              <li key={group.sourcePath} className="py-2.5">
                <button type="button" onClick={() => onOpenPage(group.sourcePath)} className="-mx-2 flex w-[calc(100%+16px)] min-w-0 items-center gap-2.5 rounded-md px-2 py-0.5 text-left hover:bg-bg-hover">
                  <FileText size={15} className="shrink-0 text-text-faint" aria-hidden />
                  <span className="min-w-0 truncate text-[13.5px] font-medium text-text">{group.sourceTitle || group.sourcePath}</span>
                  <span className="min-w-0 flex-1 truncate text-[12px] text-text-faint">{group.sourcePath}</span>
                </button>
                <ul className="m-0 mt-1 grid list-none gap-1 p-0 pl-[25px]">
                  {group.targets.map((target) => (
                    <li key={target} className="flex min-w-0 items-center gap-2 text-[12.5px] text-text-secondary">
                      <span aria-hidden className="size-1.5 shrink-0 rounded-full bg-red" />
                      <span className="shrink-0">Missing</span>
                      <code className="min-w-0 break-all text-[12px] text-red" style={{ fontFamily: fonts.mono }}>{target}</code>
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
