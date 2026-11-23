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
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Button } from '@/components/ui/button';
import { InlineFeedback } from '@/components/ui/inline-feedback';
import { ViewEmpty, ViewHeader, ViewTabs } from '@/components/ui/view-header';

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

  const header = (
    <ViewHeader
      title="Notifications"
      className="mb-4"
      actions={(
        <Button size="sm" variant="ghost" onClick={markAll} disabled={unreadCount === 0}>
          <CheckCheck /> Mark all as read
        </Button>
      )}
    />
  );

  if (error) {
    return (
      <section style={{ width: 'min(860px, 100%)' }}>
        {header}
        <InlineFeedback title="Could not load notifications" description={error} />
      </section>
    );
  }

  return (
    <section style={{ width: 'min(860px, 100%)' }}>
      {header}

      <div className="mb-4 flex flex-wrap items-end gap-2 border-b border-border-soft">
        <ViewTabs
          label="Notification filters"
          className="mb-0 border-b-0"
          value={tab}
          onChange={setTab}
          tabs={[
            { id: 'unread', label: 'Unread', count: unreadCount },
            { id: 'all', label: 'All', count: (notifs ?? []).length },
          ]}
        />
        <div className="flex-1" />
        <div className="flex gap-2 pb-1.5">
          <FilterSelect
            value={kindFilter}
            onChange={setKindFilter}
            options={[{ value: 'all', label: 'All types' }, ...kinds.map((k) => ({ value: k, label: kindMeta(k).label }))]}
          />
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
      </div>

      {notifs == null ? (
        <ViewEmpty>Loading…</ViewEmpty>
      ) : filtered.length === 0 ? (
        <ViewEmpty><Inbox size={15} aria-hidden /> {tab === 'unread' ? 'You’re all caught up.' : 'No notifications.'}</ViewEmpty>
      ) : (
        <div className="divide-y divide-border-soft">
          {filtered.map((n) => {
            const meta = kindMeta(n.kind);
            const Icon = meta.icon;
            const actionable = (n.kind === 'invitation' || n.kind === 'ownership_transfer') && linkId(n);
            return (
              <div key={n.id} className="group flex items-start gap-3 px-1 py-3">
                <span className="flex w-2 shrink-0 justify-center pt-2">
                  {!n.read && <span aria-label="Unread" className="size-1.5 rounded-full bg-accent" />}
                </span>
                <Icon size={15} className="mt-0.5 shrink-0" style={{ color: meta.color }} aria-hidden />
                <div className="min-w-0 flex-1">
                  <div className={n.read ? 'text-[13.5px] leading-5 text-text-secondary' : 'text-[13.5px] font-medium leading-5 text-text'}>{n.title}</div>
                  {n.body && <div className="mt-0.5 text-[12.5px] leading-5 text-text-tertiary">{n.body}</div>}
                  <div className="mt-1 text-[12px] text-text-faint">
                    {[n.workspace_name, meta.label, timeAgo(n.created_at)].filter(Boolean).join(' · ')}
                  </div>
                  {actionable && !n.read && (
                    <div className="mt-2 flex gap-2">
                      <Button size="xs" variant="outline" className="h-7 px-3" onClick={() => act(n, 'decline')} disabled={acting === n.id}>Decline</Button>
                      <Button size="xs" className="h-7 px-3" onClick={() => act(n, 'accept')} disabled={acting === n.id}>Accept</Button>
                    </div>
                  )}
                </div>
                <button
                  type="button"
                  onClick={() => setRead(n, !n.read)}
                  aria-label={n.read ? 'Mark as unread' : 'Mark as read'}
                  title={n.read ? 'Mark as unread' : 'Mark as read'}
                  className="flex size-7 shrink-0 items-center justify-center rounded-md text-text-faint opacity-0 transition-opacity hover:bg-bg-hover hover:text-text group-hover:opacity-100 focus-visible:opacity-100"
                >
                  {n.read ? <Circle size={14} /> : <Check size={14} />}
                </button>
              </div>
            );
          })}
        </div>
      )}
    </section>
  );
}

/** A compact filter built on the shared Select (never the platform's). */
function FilterSelect({
  value, onChange, options,
}: {
  value: string;
  onChange: (v: string) => void;
  options: { value: string; label: string }[];
}) {
  // Radix reserves the empty value; map it to a private one.
  const EMPTY = '__all__';
  return (
    <Select value={value === '' ? EMPTY : value} onValueChange={(next) => onChange(next === EMPTY ? '' : next)}>
      <SelectTrigger className="h-7 min-w-28 border-transparent bg-transparent text-text-secondary shadow-none hover:bg-bg-hover">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {options.map((o) => (
          <SelectItem key={o.value || EMPTY} value={o.value === '' ? EMPTY : o.value}>{o.label}</SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

export default NotificationsPage;
