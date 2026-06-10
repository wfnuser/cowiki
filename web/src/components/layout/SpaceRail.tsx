import { Plus, Settings, Compass, LogOut, Bell } from 'lucide-react';
import { useState, useEffect } from 'react';
import type { Workspace, Notification } from '../../api';
import {
  listNotifications, notificationUnreadCount, markAllNotificationsRead,
  markNotificationRead, acceptInvitation, rejectInvitation,
  acceptTransfer, rejectTransfer,
} from '../../api';
import {
  Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import {
  Tooltip, TooltipContent, TooltipTrigger,
} from '@/components/ui/tooltip';
import {
  DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { C, spaceTileColors } from '@/lib/design';

function tileColor(index: number): string {
  return spaceTileColors[index % spaceTileColors.length];
}

interface SpaceRailProps {
  workspaces: Workspace[];
  activeWorkspaceId: string | null;
  userName: string;
  onSelectWorkspace: (ws: Workspace) => void;
  onCreateWorkspace: () => void;
  onSettings: () => void;
  onDiscover: () => void;
  onLogout: () => void;
}

export function SpaceRail({
  workspaces,
  activeWorkspaceId,
  userName,
  onSelectWorkspace,
  onCreateWorkspace,
  onSettings,
  onDiscover,
  onLogout,
}: SpaceRailProps) {
  const personalSpaces = workspaces.filter((w) => w.visibility === 'private' && w.role === 'owner');
  const teamSpaces = workspaces.filter((w) => !(w.visibility === 'private' && w.role === 'owner'));

  return (
    <aside style={{
      width: 64, minWidth: 64, height: '100vh',
      background: C.rail, borderRight: `1px solid ${C.line}`,
      display: 'flex', flexDirection: 'column', alignItems: 'center',
      padding: '0 0 12px', gap: 0, position: 'sticky', top: 0,
      zIndex: 20,
    }}>
      {/* Logo — matches panel header height (52px) */}
      <div style={{
        width: 64, height: 52, display: 'flex', alignItems: 'center', justifyContent: 'center',
        cursor: 'default', flexShrink: 0,
      }}>
        <span style={{
          fontFamily: '"Source Serif 4 Variable", Georgia, serif',
          fontWeight: 700, fontSize: 22, color: C.ink, letterSpacing: '-0.02em',
        }}>
          C<span style={{ color: C.accent }}>.</span>
        </span>
      </div>

      {/* Separator */}
      <div style={{ width: 26, height: 1, background: C.line, marginBottom: 6 }} />

      {/* Personal space tiles */}
      {personalSpaces.map((ws) => (
        <SpaceTile
          key={ws.id}
          workspace={ws}
          active={activeWorkspaceId === ws.id}
          onClick={() => onSelectWorkspace(ws)}
          color={C.accent}
          style={{
            background: C.accent,
            color: '#fff',
          }}
          icon={<svg width={21} height={21} viewBox="0 0 24 24" fill="none" stroke="#fff" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round"><circle cx="10" cy="7" r="3.4"/><path d="M4 19.5a6 6 0 0 1 10.5-4"/><rect x="14.5" y="15" width="6.5" height="5.5" rx="1"/><path d="M16 15v-1.2a1.75 1.75 0 0 1 3.5 0V15"/></svg>}
        />
      ))}

      {/* Team space tiles */}
      {teamSpaces.map((ws, i) => (
        <SpaceTile
          key={ws.id}
          workspace={ws}
          active={activeWorkspaceId === ws.id}
          onClick={() => onSelectWorkspace(ws)}
          color={tileColor(i)}
          style={{
            background: tileColor(i),
            color: '#fff',
          }}
        />
      ))}

      {/* Add button */}
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            onClick={onCreateWorkspace}
            style={{
              width: 42, height: 42, borderRadius: 13, border: `1.5px dashed ${C.faint}`,
              background: 'transparent', cursor: 'pointer',
              display: 'flex', alignItems: 'center', justifyContent: 'center',
              marginTop: 2, color: C.faint, transition: 'all 0.15s',
            }}
            onMouseEnter={(e) => { e.currentTarget.style.borderColor = C.muted; e.currentTarget.style.color = C.muted; }}
            onMouseLeave={(e) => { e.currentTarget.style.borderColor = C.faint; e.currentTarget.style.color = C.faint; }}
          >
            <Plus size={16} />
          </button>
        </TooltipTrigger>
        <TooltipContent side="right">New team space</TooltipContent>
      </Tooltip>

      {/* Spacer */}
      <div style={{ flex: 1 }} />

      {/* Notification bell */}
      <NotificationBell />

      {/* User avatar + menu */}
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button
            style={{
              width: 34, height: 34, borderRadius: '50%',
              background: C.sidebar, border: `1px solid ${C.line}`,
              cursor: 'pointer', display: 'flex', alignItems: 'center', justifyContent: 'center',
              fontSize: 13, fontWeight: 600, color: C.ink2,
            }}
          >
            {userName?.[0]?.toUpperCase() || 'U'}
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent side="right" align="end" className="w-44">
          <div className="px-2 py-1.5 text-xs text-gray-500">{userName}</div>
          <DropdownMenuSeparator />
          <DropdownMenuItem onClick={onSettings}>
            <Settings size={14} className="mr-2" /> Settings
          </DropdownMenuItem>
          <DropdownMenuItem onClick={onDiscover}>
            <Compass size={14} className="mr-2" /> Discover
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem onClick={onLogout}>
            <LogOut size={14} className="mr-2" /> Sign out
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </aside>
  );
}

function SpaceTile({
  workspace,
  active,
  onClick,
  style,
  icon,
  color,
}: {
  workspace: Workspace;
  active: boolean;
  onClick: () => void;
  style: React.CSSProperties;
  icon?: React.ReactNode;
  color: string;
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <div style={{ position: 'relative', marginBottom: 8 }}>
          {/* Active indicator bar */}
          {active && (
            <div style={{
              position: 'absolute', left: -11, top: '50%', transform: 'translateY(-50%)',
              width: 3.5, height: 20, borderRadius: 3, background: color,
            }} />
          )}
          <button
            onClick={onClick}
            style={{
              width: 42, height: 42, borderRadius: 13, border: 'none', cursor: 'pointer',
              display: 'flex', alignItems: 'center', justifyContent: 'center',
              fontSize: 15, fontWeight: 700, transition: 'all 0.15s',
              boxShadow: active ? `0 0 0 2px ${C.rail}, 0 0 0 4px ${color}` : 'none',
              ...style,
            }}
          >
            {icon ?? workspace.name[0]?.toUpperCase()}
          </button>
        </div>
      </TooltipTrigger>
      <TooltipContent side="right">{workspace.name}</TooltipContent>
    </Tooltip>
  );
}

// ── Notification Bell ──

function NotificationBell() {
  const [unread, setUnread] = useState(0);
  const [notifs, setNotifs] = useState<Notification[]>([]);
  const [open, setOpen] = useState(false);
  const [detail, setDetail] = useState<Notification | null>(null);
  const [acting, setActing] = useState(false);

  useEffect(() => {
    let cancelled = false;
    notificationUnreadCount()
      .then((c) => { if (!cancelled) setUnread(c); })
      .catch(() => {});
    return () => { cancelled = true; };
  }, []);

  const loadNotifs = async () => {
    try {
      const list = await listNotifications(30);
      setNotifs(list);
    } catch { /* ignore */ }
  };

  const handleBellClick = () => {
    const next = !open;
    setOpen(next);
    if (next) loadNotifs();
  };

  const handleMarkAllRead = async () => {
    try {
      await markAllNotificationsRead();
      setUnread(0);
      setNotifs((prev) => prev.map((n) => ({ ...n, read: true })));
    } catch { /* ignore */ }
  };

  const markRead = async (id: string) => {
    try {
      await markNotificationRead(id);
      setNotifs((prev) => prev.map((n) => (n.id === id ? { ...n, read: true } : n)));
      setUnread((prev) => Math.max(0, prev - 1));
    } catch { /* ignore */ }
  };

  // Extract resource id from notification link (format: /invitations/{uuid} or /transfers/{uuid})
  const extractLinkId = (n: Notification): string | null => {
    if (!n.link) return null;
    const parts = n.link.split('/');
    return parts[parts.length - 1] || null;
  };

  const handleItemClick = (n: Notification) => {
    setDetail(n);
    if (!n.read) markRead(n.id);
  };

  const handleAccept = async () => {
    if (!detail) return;
    const invId = extractLinkId(detail);
    if (!invId) return;
    setActing(true);
    try {
      await acceptInvitation(invId);
      markRead(detail.id);
      setDetail(null);
      window.location.reload();
    } catch { setActing(false); }
  };

  const handleReject = async () => {
    if (!detail) return;
    const invId = extractLinkId(detail);
    if (!invId) return;
    setActing(true);
    try {
      await rejectInvitation(invId);
      markRead(detail.id);
      setDetail(null);
    } catch { setActing(false); }
  };

  const timeAgo = (iso: string): string => {
    const diff = Date.now() - new Date(iso).getTime();
    const mins = Math.floor(diff / 60000);
    if (mins < 1) return 'just now';
    if (mins < 60) return `${mins}m`;
    const hours = Math.floor(mins / 60);
    if (hours < 24) return `${hours}h`;
    return `${Math.floor(hours / 24)}d`;
  };

  const formatTime = (iso: string): string => {
    const d = new Date(iso);
    return d.toLocaleString();
  };

  const isInvitation = detail?.kind === 'invitation' && extractLinkId(detail);
  const isTransfer = detail?.kind === 'ownership_transfer' && extractLinkId(detail);

  const handleTransferAccept = async () => {
    if (!detail) return;
    const tId = extractLinkId(detail);
    if (!tId) return;
    setActing(true);
    try {
      await acceptTransfer(tId);
      markRead(detail.id);
      setDetail(null);
      window.location.reload();
    } catch { setActing(false); }
  };

  const handleTransferReject = async () => {
    if (!detail) return;
    const tId = extractLinkId(detail);
    if (!tId) return;
    setActing(true);
    try {
      await rejectTransfer(tId);
      markRead(detail.id);
      setDetail(null);
    } catch { setActing(false); }
  };

  return (
    <div style={{ position: 'relative' }}>
      <button
        onClick={handleBellClick}
        style={{
          width: 34, height: 34, borderRadius: '50%',
          border: `1px solid ${C.line}`, background: 'transparent', cursor: 'pointer',
          display: 'flex', alignItems: 'center', justifyContent: 'center',
          marginBottom: 6, color: C.ink2, position: 'relative',
        }}
      >
        <Bell size={16} />
        {unread > 0 && (
          <span style={{
            position: 'absolute', top: -2, right: -2,
            minWidth: 16, height: 16, borderRadius: 8,
            background: C.accent, color: '#fff', fontSize: 10,
            fontWeight: 600, display: 'flex', alignItems: 'center',
            justifyContent: 'center', padding: '0 4px',
          }}>
            {unread > 99 ? '99+' : unread}
          </span>
        )}
      </button>

      {/* Notification list dropdown */}
      {open && (
        <>
          <div
            style={{ position: 'fixed', inset: 0, zIndex: 90 }}
            onClick={() => setOpen(false)}
          />
          <div style={{
            position: 'fixed', left: 72, bottom: 60, zIndex: 100,
            width: 360, maxHeight: 400,
            border: `1px solid ${C.line}`, borderRadius: 10,
            background: C.panel, boxShadow: '0 8px 28px rgba(29,28,26,0.15)',
            display: 'flex', flexDirection: 'column',
          }}>
            <div style={{
              display: 'flex', alignItems: 'center', justifyContent: 'space-between',
              padding: '12px 16px', borderBottom: `1px solid ${C.line}`,
            }}>
              <span style={{ fontSize: 14, fontWeight: 600, color: C.ink }}>Notifications</span>
              {unread > 0 && (
                <button onClick={handleMarkAllRead} style={{
                  background: 'none', border: 'none', cursor: 'pointer',
                  fontSize: 12, color: C.accent, fontWeight: 500,
                }}>
                  Mark all read
                </button>
              )}
            </div>
            <div style={{ flex: 1, overflow: 'auto' }}>
              {notifs.length === 0 ? (
                <div style={{ padding: '32px 16px', textAlign: 'center', color: C.muted, fontSize: 13 }}>
                  No notifications
                </div>
              ) : (
                notifs.map((n) => (
                  <button
                    key={n.id}
                    onClick={() => { setOpen(false); handleItemClick(n); }}
                    style={{
                      display: 'flex', gap: 10, padding: '12px 16px',
                      width: '100%', border: 'none', cursor: 'pointer',
                      textAlign: 'left', background: n.read ? C.panel : C.sidebar,
                      borderBottom: `1px solid ${C.lineSoft}`,
                    }}
                  >
                    <div style={{
                      width: 8, height: 8, borderRadius: '50%',
                      background: n.read ? 'transparent' : C.accent,
                      marginTop: 6, flexShrink: 0,
                    }} />
                    <div style={{ flex: 1, minWidth: 0 }}>
                      <div style={{
                        fontSize: 13, fontWeight: n.read ? 400 : 600, color: C.ink,
                        overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
                      }}>
                        {n.title}
                      </div>
                      {n.body && (
                        <div style={{
                          fontSize: 12, color: C.muted, marginTop: 2,
                          overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
                        }}>
                          {n.body}
                        </div>
                      )}
                      <div style={{ fontSize: 11, color: C.faint, marginTop: 4 }}>
                        {timeAgo(n.created_at)}
                      </div>
                    </div>
                  </button>
                ))
              )}
            </div>
          </div>
        </>
      )}

      {/* Detail modal */}
      <Dialog open={!!detail} onOpenChange={(open) => { if (!open) setDetail(null); }}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle style={{ fontSize: 16 }}>
              {detail?.kind === 'invitation' ? 'Workspace Invitation'
                : detail?.kind === 'ownership_transfer' ? 'Ownership Transfer'
                : 'Notification'}
            </DialogTitle>
            <DialogDescription />
          </DialogHeader>

          {detail && (
            <div className="space-y-4 mt-2">
              <div>
                <div style={{ fontSize: 15, fontWeight: 600, color: C.ink, marginBottom: 4 }}>
                  {detail.title}
                </div>
                {detail.body && (
                  <div style={{ fontSize: 13, color: C.ink2, lineHeight: 1.5 }}>
                    {detail.body}
                  </div>
                )}
                <div style={{ fontSize: 12, color: C.faint, marginTop: 8 }}>
                  {formatTime(detail.created_at)}
                </div>
              </div>

              {/* Action buttons for invitation */}
              {isInvitation && (
                <div style={{
                  display: 'flex', gap: 10, paddingTop: 8,
                  borderTop: `1px solid ${C.line}`,
                }}>
                  <Button
                    onClick={handleAccept}
                    disabled={acting}
                    style={{
                      flex: 1, background: C.green, border: 'none',
                      color: '#fff', fontWeight: 600,
                    }}
                  >
                    {acting ? 'Accepting...' : 'Accept Invitation'}
                  </Button>
                  <Button
                    variant="outline"
                    onClick={handleReject}
                    disabled={acting}
                    style={{ flex: 1 }}
                  >
                    Decline
                  </Button>
                </div>
              )}

              {/* Action buttons for ownership transfer */}
              {isTransfer && (
                <div style={{
                  display: 'flex', gap: 10, paddingTop: 8,
                  borderTop: `1px solid ${C.line}`,
                }}>
                  <Button
                    onClick={handleTransferAccept}
                    disabled={acting}
                    style={{
                      flex: 1, background: C.accent, border: 'none',
                      color: '#fff', fontWeight: 600,
                    }}
                  >
                    {acting ? 'Accepting...' : 'Accept Transfer'}
                  </Button>
                  <Button
                    variant="outline"
                    onClick={handleTransferReject}
                    disabled={acting}
                    style={{ flex: 1 }}
                  >
                    Decline
                  </Button>
                </div>
              )}

              {/* Read-only: other notification types */}
              {!isInvitation && !isTransfer && (
                <div className="flex justify-end pt-2">
                  <Button variant="outline" onClick={() => setDetail(null)}>
                    Close
                  </Button>
                </div>
              )}
            </div>
          )}
        </DialogContent>
      </Dialog>
    </div>
  );
}

export default SpaceRail;
