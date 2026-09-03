import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  CircleCheck,
  FileText,
  LoaderCircle,
  RefreshCw,
  Unlink,
} from 'lucide-react';

import { listBrokenLinks, type BrokenLink } from '@/api';
import { InlineFeedback } from '@/components/ui/inline-feedback';
import { C } from '@/lib/design';
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
    <section style={{ width: 'min(760px, 100%)', margin: '0 auto' }}>
      <header style={{ marginBottom: 28 }}>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 16, marginBottom: 8 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
            <Unlink size={20} color={C.accent} strokeWidth={1.8} />
            <h1 className="page-title page-title--compact" style={{ margin: 0 }}>Links</h1>
          </div>
          <button type="button" onClick={() => { void refresh(); }} disabled={refreshing} style={refreshButtonStyle}>
            <RefreshCw size={13} className={refreshing ? 'animate-spin' : undefined} />
            <span>Refresh</span>
          </button>
        </div>
        <p style={{ margin: 0, maxWidth: 610, color: C.muted, fontSize: 13.5, lineHeight: 1.65 }}>
          A read-only check of internal links. Paths are matched exactly, including letter
          case. Results update when you open this view, choose Refresh, or return to this window.
        </p>
      </header>

      {mode === 'loading' ? (
        <div style={emptyStyle}><LoaderCircle size={17} className="animate-spin" /> Checking links…</div>
      ) : mode === 'error' ? (
        <InlineFeedback
          title="Could not inspect links"
          description={error}
          action={(
            <button type="button" onClick={() => { void refresh(); }} disabled={refreshing} style={retryStyle}>
              <RefreshCw size={12} /> Retry
            </button>
          )}
        />
      ) : mode === 'clean' ? (
        <div style={cleanStyle} aria-live="polite">
          <span style={cleanIconStyle}><CircleCheck size={21} /></span>
          <div>
            <strong style={{ display: 'block', color: C.ink2, fontSize: 14, marginBottom: 3 }}>
              No broken links
            </strong>
            <span style={{ color: C.muted, fontSize: 12.5 }}>
              All checked internal targets resolve in the current Space tree.
            </span>
          </div>
        </div>
      ) : (
        <div aria-live="polite">
          <div style={{ display: 'flex', alignItems: 'baseline', justifyContent: 'space-between', marginBottom: 12 }}>
            <h2 style={{ margin: 0, color: C.ink2, fontSize: 12, fontWeight: 700, letterSpacing: '0.08em', textTransform: 'uppercase' }}>
              Needs attention
            </h2>
            <span style={{ color: C.red, fontSize: 11.5, fontWeight: 650 }}>
              {brokenLinkSummary(links.length)}
            </span>
          </div>
          <ol style={{ display: 'grid', gap: 10, margin: 0, padding: 0, listStyle: 'none' }}>
            {groups.map((group) => (
              <li key={group.sourcePath} style={sourceCardStyle}>
                <button type="button" onClick={() => onOpenPage(group.sourcePath)} style={sourceButtonStyle}>
                  <span style={fileIconStyle}><FileText size={16} /></span>
                  <span style={{ minWidth: 0, textAlign: 'left' }}>
                    <strong style={{ display: 'block', color: C.ink2, fontSize: 13.5, fontWeight: 630, overflow: 'hidden', textOverflow: 'ellipsis' }}>
                      {group.sourceTitle || group.sourcePath}
                    </strong>
                    <code style={{ color: C.muted, fontSize: 11.5 }}>{group.sourcePath}</code>
                  </span>
                </button>
                <ul style={{ margin: '12px 0 0 42px', padding: 0, listStyle: 'none', display: 'grid', gap: 7 }}>
                  {group.targets.map((target) => (
                    <li key={target} style={{ display: 'flex', alignItems: 'center', gap: 8, color: C.ink2, fontSize: 12.5 }}>
                      <span aria-hidden style={{ width: 5, height: 5, borderRadius: '50%', background: C.red, flexShrink: 0 }} />
                      Missing target <code style={targetCodeStyle}>{target}</code>
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

const retryStyle: React.CSSProperties = {
  display: 'inline-flex', alignItems: 'center', gap: 5, border: 'none', background: 'transparent',
  color: C.red, fontSize: 12, fontWeight: 650, cursor: 'pointer', padding: '3px 5px',
};

const refreshButtonStyle: React.CSSProperties = {
  display: 'inline-flex', alignItems: 'center', gap: 6, padding: '6px 9px',
  border: `1px solid ${C.line}`, borderRadius: 7, background: C.panel,
  color: C.ink2, fontSize: 12, fontWeight: 620, cursor: 'pointer',
};

const emptyStyle: React.CSSProperties = {
  display: 'flex', alignItems: 'center', justifyContent: 'center', gap: 8, minHeight: 150,
  border: `1px dashed ${C.line}`, borderRadius: 11, color: C.muted, fontSize: 12.5,
};

const cleanStyle: React.CSSProperties = {
  display: 'flex', alignItems: 'center', gap: 14, padding: '20px 18px', borderRadius: 11,
  border: `1px solid ${C.greenTintBorder}`, background: C.greenBgSoft,
};

const cleanIconStyle: React.CSSProperties = {
  width: 38, height: 38, display: 'grid', placeItems: 'center', flexShrink: 0,
  borderRadius: 11, color: C.green, background: C.panel,
  border: `1px solid ${C.greenTintBorder}`,
};

const sourceCardStyle: React.CSSProperties = {
  padding: '15px 16px 16px', border: `1px solid ${C.line}`, borderRadius: 11,
  background: C.panel,
};

const sourceButtonStyle: React.CSSProperties = {
  display: 'flex', alignItems: 'center', gap: 10, width: '100%', padding: 0,
  border: 'none', background: 'transparent', cursor: 'pointer',
};

const fileIconStyle: React.CSSProperties = {
  width: 32, height: 32, display: 'grid', placeItems: 'center', flexShrink: 0,
  borderRadius: 9, color: C.accent, background: C.accentSoft,
};

const targetCodeStyle: React.CSSProperties = {
  padding: '2px 6px', borderRadius: 5, color: C.red, background: C.redSoft,
  fontSize: 11.5, overflowWrap: 'anywhere',
};
