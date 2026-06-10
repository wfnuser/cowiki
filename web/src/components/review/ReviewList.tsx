import { useEffect, useState } from 'react';
import { GitBranch, MessageSquare } from 'lucide-react';
import { listReviews, type Submission } from '../../api';
import { C } from '@/lib/design';
import { timeAgo } from '../../lib/time';
import { AvatarBadge } from '@/components/ui/avatar-badge';
import { statusBadge } from '@/lib/review';

type Filter = 'open' | 'merged' | 'all';

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
    return <p style={{ color: C.red, fontSize: 14 }}>Failed to load reviews: {error}</p>;
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

      {/* Filter pills (design: active pill = ink, others plain) */}
      <div style={{ display: 'flex', gap: 8, marginBottom: 18, fontSize: 13.5 }}>
        {([
          { key: 'open' as Filter, label: 'Open', count: openCount },
          { key: 'merged' as Filter, label: 'Merged', count: mergedCount },
          { key: 'all' as Filter, label: 'All', count: subs.length },
        ]).map((tab) => (
          <button
            key={tab.key}
            onClick={() => setFilter(tab.key)}
            style={{
              padding: '6px 12px', borderRadius: 7, border: 'none', cursor: 'pointer',
              fontWeight: 550,
              background: filter === tab.key ? C.ink : 'transparent',
              color: filter === tab.key ? '#fff' : C.muted,
              transition: 'all 0.15s',
            }}
          >
            {tab.label} · {tab.count}
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
        <div style={{ border: `1px solid ${C.line}`, borderRadius: 12, overflow: 'hidden', background: C.panel }}>
          {filtered?.map((s, i) => {
            const sb = statusBadge[s.status] ?? statusBadge.pending;
            const author = s.author_name || s.user_id.slice(0, 8);
            return (
              <button
                key={s.id}
                onClick={() => onOpen(s.id)}
                style={{
                  display: 'flex', alignItems: 'center', gap: 14,
                  padding: '15px 18px', background: C.panel, border: 'none',
                  borderTop: i ? `1px solid ${C.line}` : 'none',
                  textAlign: 'left', cursor: 'pointer', width: '100%',
                  transition: 'background 0.1s',
                }}
                onMouseEnter={(e) => { e.currentTarget.style.background = C.sidebar; }}
                onMouseLeave={(e) => { e.currentTarget.style.background = C.panel; }}
              >
                <GitBranch size={17} color={C.faint} />

                {/* Main content */}
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{ fontSize: 15, fontWeight: 550, color: C.ink, marginBottom: 3, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                    {s.summary || s.page_slugs.join(', ') || 'Submission'}
                  </div>
                  <div style={{ fontSize: 12.5, color: C.muted, display: 'flex', alignItems: 'center', gap: 8 }}>
                    <span>{author}</span>
                    <span>·</span>
                    <span>{s.page_slugs.length} file{s.page_slugs.length === 1 ? '' : 's'}</span>
                    <span>·</span>
                    <span>{timeAgo(s.created_at)}</span>
                  </div>
                </div>

                {/* Right side */}
                <div style={{ display: 'flex', alignItems: 'center', gap: 10, flexShrink: 0 }}>
                  <span style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 12.5, color: C.faint }}>
                    <MessageSquare size={14} /> 0
                  </span>
                  <span style={{
                    fontSize: 12, fontWeight: 600, padding: '5px 10px', borderRadius: 999,
                    background: sb.bg, color: sb.fg, whiteSpace: 'nowrap',
                  }}>
                    {sb.label}
                  </span>
                  <AvatarBadge name={author} size={26} />
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
