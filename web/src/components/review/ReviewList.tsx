import { useEffect, useState } from 'react';
import { GitBranch, MessageSquare } from 'lucide-react';
import { listReviews, type Submission } from '../../api';

/* ── Design tokens ── */
const C = {
  bg: '#faf9f7',
  panel: '#fdfcfb',
  sidebar: '#f5f4f1',
  rail: '#efedea',
  ink: '#1d1c1a',
  ink2: '#403e3a',
  muted: '#8c897f',
  faint: '#a8a59b',
  line: '#e8e6e1',
  accent: '#e2590b',
  green: '#2f8a5b',
  amber: '#b5790f',
} as const;

type Filter = 'open' | 'merged' | 'all';

const statusBadge: Record<string, { bg: string; fg: string; label: string }> = {
  pending: { bg: '#fff8c5', fg: '#9a6700', label: 'Review needed' },
  approved: { bg: '#dafbe1', fg: '#1a7f37', label: 'Approved' },
  rejected: { bg: '#ffebe9', fg: '#cf222e', label: 'Changes req.' },
  merged: { bg: '#ddf4ff', fg: '#0969da', label: 'Merged' },
};

function timeAgo(iso: string): string {
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return '';
  const secs = Math.max(0, Math.floor((Date.now() - then) / 1000));
  if (secs < 60) return `${secs}s`;
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}m`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs}h`;
  return `${Math.floor(hrs / 24)}d`;
}

export function ReviewList({
  workspaceSlug,
  onOpen,
  refreshKey,
}: {
  workspaceSlug: string;
  onOpen: (id: string) => void;
  refreshKey?: number;
}) {
  const [subs, setSubs] = useState<Submission[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState<Filter>('open');

  useEffect(() => {
    let cancelled = false;
    setSubs(null);
    setError(null);
    listReviews(workspaceSlug)
      .then((s) => !cancelled && setSubs(s))
      .catch((e) => !cancelled && setError(e.message || 'Failed to load reviews'));
    return () => { cancelled = true; };
  }, [workspaceSlug, refreshKey]);

  const filtered = subs?.filter((s) => {
    if (filter === 'open') return s.status === 'pending' || s.status === 'approved' || s.status === 'rejected';
    if (filter === 'merged') return s.status === 'merged';
    return true;
  });

  if (error) {
    return <p style={{ color: '#cf222e', fontSize: 14 }}>Failed to load reviews: {error}</p>;
  }

  if (subs == null) {
    return <p style={{ color: C.muted, fontSize: 14, padding: '16px 0' }}>Loading reviews...</p>;
  }

  const openCount = subs.filter((s) => s.status !== 'merged').length;
  const mergedCount = subs.filter((s) => s.status === 'merged').length;

  return (
    <div>
      {/* Header */}
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 16 }}>
        <h1 className="page-title" style={{ marginBottom: 0 }}>
          Reviews
        </h1>
      </div>

      {/* Filter tabs */}
      <div style={{
        display: 'flex', gap: 2, marginBottom: 16, padding: 2, background: C.rail, borderRadius: 8,
        width: 'fit-content',
      }}>
        {([
          { key: 'open' as Filter, label: `Open (${openCount})` },
          { key: 'merged' as Filter, label: `Merged (${mergedCount})` },
          { key: 'all' as Filter, label: `All (${subs.length})` },
        ]).map((tab) => (
          <button
            key={tab.key}
            onClick={() => setFilter(tab.key)}
            style={{
              padding: '5px 14px', borderRadius: 6, border: 'none', cursor: 'pointer',
              fontSize: 12, fontWeight: filter === tab.key ? 600 : 400,
              background: filter === tab.key ? '#fff' : 'transparent',
              color: filter === tab.key ? C.ink : C.muted,
              boxShadow: filter === tab.key ? '0 1px 2px rgba(0,0,0,0.06)' : 'none',
              transition: 'all 0.15s',
            }}
          >
            {tab.label}
          </button>
        ))}
      </div>

      {/* List */}
      {filtered && filtered.length === 0 ? (
        <div style={{
          padding: 32, textAlign: 'center', color: C.muted, fontSize: 14,
          border: `1px solid ${C.line}`, borderRadius: 8, background: C.panel,
        }}>
          No {filter === 'all' ? '' : filter} reviews.
        </div>
      ) : (
        <div style={{ border: `1px solid ${C.line}`, borderRadius: 8, overflow: 'hidden' }}>
          {filtered?.map((s, i) => {
            const sb = statusBadge[s.status] ?? statusBadge.pending;
            return (
              <button
                key={s.id}
                onClick={() => onOpen(s.id)}
                style={{
                  display: 'flex', alignItems: 'center', gap: 12,
                  padding: '12px 16px', background: C.panel, border: 'none',
                  borderBottom: i < (filtered.length - 1) ? `1px solid ${C.line}` : 'none',
                  textAlign: 'left', cursor: 'pointer', width: '100%',
                  transition: 'background 0.1s',
                }}
                onMouseEnter={(e) => { e.currentTarget.style.background = C.sidebar; }}
                onMouseLeave={(e) => { e.currentTarget.style.background = C.panel; }}
              >
                {/* Branch icon */}
                <GitBranch size={16} color={s.status === 'pending' ? C.green : s.status === 'merged' ? '#8250df' : C.muted} />

                {/* Main content */}
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                    <span style={{ fontSize: 14, fontWeight: 500, color: C.ink, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                      {s.summary || s.page_slugs.join(', ') || 'Submission'}
                    </span>
                  </div>
                  <div style={{ fontSize: 12, color: C.muted, marginTop: 2 }}>
                    {s.source_branch} · {s.page_slugs.length} file{s.page_slugs.length === 1 ? '' : 's'} · {timeAgo(s.created_at)} ago
                  </div>
                </div>

                {/* Right side */}
                <div style={{ display: 'flex', alignItems: 'center', gap: 10, flexShrink: 0 }}>
                  {/* Comment count placeholder */}
                  <span style={{ display: 'flex', alignItems: 'center', gap: 3, fontSize: 12, color: C.faint }}>
                    <MessageSquare size={13} /> 0
                  </span>
                  {/* Status badge */}
                  <span style={{
                    fontSize: 11, fontWeight: 600, padding: '2px 8px', borderRadius: 12,
                    background: sb.bg, color: sb.fg, whiteSpace: 'nowrap',
                  }}>
                    {sb.label}
                  </span>
                  {/* Avatar */}
                  <div style={{
                    width: 24, height: 24, borderRadius: '50%', background: C.rail,
                    display: 'flex', alignItems: 'center', justifyContent: 'center',
                    fontSize: 11, fontWeight: 600, color: C.ink2,
                  }}>
                    {s.user_id.slice(0, 1).toUpperCase()}
                  </div>
                </div>
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}

export default ReviewList;
