import { useEffect, useMemo, useState } from 'react';
import {
  Mail, ArrowRightLeft, GitPullRequest, CheckCircle2, Users, Bell,
  Check, Circle, CheckCheck, Inbox,
} from 'lucide-react';
import {
  listNotifications, markNotificationRead, markNotificationUnread,
  markAllNotificationsRead, acceptInvitation, rejectInvitation,
  acceptTransfer, rejectTransfer, type Notification,
} from '../../api';
import { C, spaceTileColors } from '@/lib/design';
import { timeAgo } from '../../lib/time';

type Tab = 'unread' | 'all';

/** Per-kind icon + accent colour. Unknown kinds fall back to a neutral bell. */
function kindMeta(kind: string): { icon: typeof Bell; color: string; label: string } {
  switch (kind) {
    case 'invitation':
      return { icon: Mail, color: C.accent, label: 'Invitation' };
    case 'ownership_transfer':
      return { icon: ArrowRightLeft, color: C.purple, label: 'Ownership transfer' };
    case 'transfer_accepted':
      return { icon: CheckCircle2, color: C.green, label: 'Transfer accepted' };
    case 'review_request':
      return { icon: GitPullRequest, color: spaceTileColors[0], label: 'Review request' };
    case 'review_decision':
      return { icon: CheckCircle2, color: C.green, label: 'Review decision' };
    case 'member_joined':
      return { icon: Users, color: spaceTileColors[0], label: 'Member joined' };
    default:
      return { icon: Bell, color: C.muted, label: kind };
  }
}

function linkId(n: Notification): string | null {
  if (!n.link) return null;
  const parts = n.link.split('/');
  return parts[parts.length - 1] || null;
}

/**
 * Cross-space notifications inbox (GitHub-style). Replaces the bell dropdown as
 * the place to actually manage notifications: Unread/All tabs, filter by type and
 * by space, per-row read/unread toggle, inline accept/decline for actionable kinds,
 * and mark-all-as-read. Filtering is client-side over the fetched list — fine at
 * current volume; move to query params if it grows.
 */
export function NotificationsPage({ onUnreadChange }: { onUnreadChange?: (n: number) => void }) {
  const [notifs, setNotifs] = useState<Notification[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [tab, setTab] = useState<Tab>('unread');
  const [kindFilter, setKindFilter] = useState<string>('all');
  const [spaceFilter, setSpaceFilter] = useState<string>('all');
  const [acting, setActing] = useState<string | null>(null);

  const reload = () => {
    listNotifications(100)
      .then((list) => {
        setNotifs(list);
        onUnreadChange?.(list.filter((n) => !n.read).length);
      })
      .catch((e) => setError(e.message || 'Failed to load notifications'));
  };

  useEffect(() => {
    reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Distinct kinds and spaces present, for the filter dropdowns.
  const kinds = useMemo(
    () => Array.from(new Set((notifs ?? []).map((n) => n.kind))),
    [notifs],
  );
  const spaces = useMemo(() => {
    const m = new Map<string, string>();
    for (const n of notifs ?? []) {
      if (n.workspace_slug) m.set(n.workspace_slug, n.workspace_name || n.workspace_slug);
    }
    return Array.from(m.entries()); // [slug, name][]
  }, [notifs]);

  const filtered = (notifs ?? []).filter((n) => {
    if (tab === 'unread' && n.read) return false;
    if (kindFilter !== 'all' && n.kind !== kindFilter) return false;
    if (spaceFilter !== 'all') {
      if (spaceFilter === '__none__' ? n.workspace_slug : n.workspace_slug !== spaceFilter) return false;
    }
    return true;
  });

  const unreadCount = (notifs ?? []).filter((n) => !n.read).length;

  const setRead = async (n: Notification, read: boolean) => {
    // Optimistic; revert on failure.
    setNotifs((prev) => {
      const next = (prev ?? []).map((x) => (x.id === n.id ? { ...x, read } : x));
      onUnreadChange?.(next.filter((x) => !x.read).length);
      return next;
    });
    try {
      await (read ? markNotificationRead(n.id) : markNotificationUnread(n.id));
    } catch {
      reload();
    }
  };

  const markAll = async () => {
    setNotifs((prev) => (prev ?? []).map((x) => ({ ...x, read: true })));
    onUnreadChange?.(0);
    try {
      await markAllNotificationsRead();
    } catch {
      reload();
    }
  };

  // Accept/decline for actionable kinds. Membership-changing accepts reload the
  // app so the rail/space list reflects the new state (matches prior bell flow).
  const act = async (n: Notification, action: 'accept' | 'decline') => {
    const id = linkId(n);
    if (!id) return;
    setActing(n.id);
    try {
      if (n.kind === 'invitation') {
        await (action === 'accept' ? acceptInvitation(id) : rejectInvitation(id));
      } else if (n.kind === 'ownership_transfer') {
        await (action === 'accept' ? acceptTransfer(id) : rejectTransfer(id));
      }
      await markNotificationRead(n.id).catch(() => {});
      if (action === 'accept') {
        window.location.reload();
        return;
      }
      reload();
    } catch {
      /* leave it; user can retry */
    } finally {
      setActing(null);
    }
  };

  if (error) {
    return <p style={{ color: C.red, fontSize: 14 }}>Failed to load notifications: {error}</p>;
  }

  return (
    <div style={{ maxWidth: 860 }}>
      {/* Header */}
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 18 }}>
        <h1 className="page-title" style={{ marginBottom: 0 }}>Notifications</h1>
        <button
          onClick={markAll}
          disabled={unreadCount === 0}
          style={{
            display: 'flex', alignItems: 'center', gap: 6,
            padding: '7px 12px', borderRadius: 8, border: `1px solid ${C.line}`,
            background: C.panel, cursor: unreadCount === 0 ? 'default' : 'pointer',
            color: unreadCount === 0 ? C.faint : C.ink2, fontSize: 13, fontWeight: 550,
            opacity: unreadCount === 0 ? 0.6 : 1,
          }}
        >
          <CheckCheck size={14} /> Mark all as read
        </button>
      </div>

      {/* Filter bar */}
      <div style={{
        display: 'flex', alignItems: 'center', gap: 10, marginBottom: 14, flexWrap: 'wrap',
      }}>
        {/* Unread / All segmented */}
        <div style={{ display: 'flex', gap: 6 }}>
          {([
            { key: 'unread' as Tab, label: 'Unread', count: unreadCount },
            { key: 'all' as Tab, label: 'All', count: (notifs ?? []).length },
          ]).map((t) => (
            <button
              key={t.key}
              onClick={() => setTab(t.key)}
              style={{
                padding: '6px 12px', borderRadius: 7, border: 'none', cursor: 'pointer',
                fontWeight: 550, fontSize: 13.5,
                background: tab === t.key ? C.ink : 'transparent',
                color: tab === t.key ? C.onAccent : C.muted,
              }}
            >
              {t.label} · {t.count}
            </button>
          ))}
        </div>

        <div style={{ flex: 1 }} />

        {/* Type filter */}
        <FilterSelect
          value={kindFilter}
          onChange={setKindFilter}
          options={[{ value: 'all', label: 'All types' }, ...kinds.map((k) => ({ value: k, label: kindMeta(k).label }))]}
        />
        {/* Space filter */}
        {spaces.length > 0 && (
          <FilterSelect
            value={spaceFilter}
            onChange={setSpaceFilter}
            options={[
              { value: 'all', label: 'All spaces' },
              ...spaces.map(([slug, name]) => ({ value: slug, label: name })),
              { value: '__none__', label: 'No space' },
            ]}
          />
        )}
      </div>

      {/* List */}
      {notifs == null ? (
        <p style={{ color: C.muted, fontSize: 14, padding: '16px 0' }}>Loading…</p>
      ) : filtered.length === 0 ? (
        <div style={{
          display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 10,
          padding: '56px 24px', color: C.faint,
          border: `1px solid ${C.line}`, borderRadius: 12, background: C.panel,
        }}>
          <Inbox size={32} strokeWidth={1.5} />
          <span style={{ fontSize: 14 }}>
            {tab === 'unread' ? "You're all caught up." : 'No notifications.'}
          </span>
        </div>
      ) : (
        <div style={{ border: `1px solid ${C.line}`, borderRadius: 12, overflow: 'hidden', background: C.panel }}>
          {filtered.map((n, i) => {
            const meta = kindMeta(n.kind);
            const Icon = meta.icon;
            const actionable = (n.kind === 'invitation' || n.kind === 'ownership_transfer') && linkId(n);
            return (
              <div
                key={n.id}
                style={{
                  display: 'flex', alignItems: 'flex-start', gap: 13,
                  padding: '14px 16px',
                  borderTop: i ? `1px solid ${C.lineSoft}` : 'none',
                  background: n.read
                    ? C.panel
                    : `color-mix(in srgb, ${C.accentSoft} 33%, transparent)`,
                }}
              >
                {/* unread dot */}
                <div style={{ width: 8, paddingTop: 7, flexShrink: 0 }}>
                  {!n.read && (
                    <span style={{ display: 'block', width: 8, height: 8, borderRadius: 4, background: C.accent }} />
                  )}
                </div>

                {/* kind icon */}
                <div style={{
                  width: 32, height: 32, borderRadius: 9, flexShrink: 0,
                  background: `color-mix(in srgb, ${meta.color} 10%, transparent)`, color: meta.color,
                  display: 'flex', alignItems: 'center', justifyContent: 'center',
                }}>
                  <Icon size={16} />
                </div>

                {/* body */}
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{
                    fontSize: 14, color: C.ink, fontWeight: n.read ? 450 : 600, lineHeight: 1.4,
                  }}>
                    {n.title}
                  </div>
                  {n.body && (
                    <div style={{ fontSize: 13, color: C.muted, marginTop: 2, lineHeight: 1.45 }}>
                      {n.body}
                    </div>
                  )}
                  <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginTop: 6, fontSize: 12, color: C.faint }}>
                    {n.workspace_name && (
                      <span style={{
                        padding: '1px 7px', borderRadius: 999, background: C.rail, color: C.muted, fontWeight: 550,
                      }}>
                        {n.workspace_name}
                      </span>
                    )}
                    <span>{meta.label}</span>
                    <span>·</span>
                    <span>{timeAgo(n.created_at)}</span>
                  </div>

                  {/* inline actions */}
                  {actionable && !n.read && (
                    <div style={{ display: 'flex', gap: 8, marginTop: 10 }}>
                      <button
                        onClick={() => act(n, 'accept')}
                        disabled={acting === n.id}
                        style={{
                          padding: '5px 14px', borderRadius: 7, border: 'none', cursor: 'pointer',
                          background: C.accent, color: C.onAccent, fontSize: 13, fontWeight: 550,
                          opacity: acting === n.id ? 0.6 : 1,
                        }}
                      >
                        Accept
                      </button>
                      <button
                        onClick={() => act(n, 'decline')}
                        disabled={acting === n.id}
                        style={{
                          padding: '5px 14px', borderRadius: 7, border: `1px solid ${C.line}`, cursor: 'pointer',
                          background: C.panel, color: C.ink2, fontSize: 13, fontWeight: 550,
                          opacity: acting === n.id ? 0.6 : 1,
                        }}
                      >
                        Decline
                      </button>
                    </div>
                  )}
                </div>

                {/* read/unread toggle */}
                <button
                  onClick={() => setRead(n, !n.read)}
                  title={n.read ? 'Mark as unread' : 'Mark as read'}
                  style={{
                    flexShrink: 0, width: 28, height: 28, borderRadius: 7, border: 'none',
                    background: 'transparent', cursor: 'pointer', color: C.faint,
                    display: 'flex', alignItems: 'center', justifyContent: 'center',
                  }}
                  onMouseEnter={(e) => { e.currentTarget.style.background = C.rail; e.currentTarget.style.color = C.muted; }}
                  onMouseLeave={(e) => { e.currentTarget.style.background = 'transparent'; e.currentTarget.style.color = C.faint; }}
                >
                  {n.read ? <Circle size={15} /> : <Check size={15} />}
                </button>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

/** Small unstyled-native select that inherits the design language. */
function FilterSelect({
  value, onChange, options,
}: {
  value: string;
  onChange: (v: string) => void;
  options: { value: string; label: string }[];
}) {
  return (
    <select
      value={value}
      onChange={(e) => onChange(e.target.value)}
      style={{
        padding: '6px 10px', borderRadius: 7, border: `1px solid ${C.line}`,
        background: C.panel, color: C.ink2, fontSize: 13, cursor: 'pointer',
        fontFamily: 'inherit',
      }}
    >
      {options.map((o) => (
        <option key={o.value} value={o.value}>{o.label}</option>
      ))}
    </select>
  );
}

export default NotificationsPage;
