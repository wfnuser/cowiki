import { useEffect, useState } from 'react';
import { Users, UserPlus, MoreHorizontal } from 'lucide-react';
import {
  DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import {
  Select, SelectContent, SelectItem, SelectTrigger, SelectValue,
} from '@/components/ui/select';
import {
  listMembers, removeMember, changeMemberRole,
  type MemberInfo,
} from '../../api';
import { C, spaceTileColors } from '@/lib/design';

// ── Helpers ──

function avatarColor(name: string): string {
  let hash = 0;
  for (let i = 0; i < name.length; i++) {
    hash = ((hash << 5) - hash) + name.charCodeAt(i);
    hash |= 0;
  }
  return spaceTileColors[Math.abs(hash) % spaceTileColors.length];
}

function relativeTime(iso: string | null): string {
  if (!iso) return '--';
  const diff = Date.now() - new Date(iso).getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return 'just now';
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days}d ago`;
  const months = Math.floor(days / 30);
  return `${months}mo ago`;
}

// ── Types ──

interface MembersViewProps {
  workspaceSlug: string;
  canManage: boolean;
  currentUserRole: string;
  currentUserId: string;
  isOwner: boolean;
  onInvite: () => void;
  onTransfer: () => void;
}

export function MembersView({ workspaceSlug, canManage, currentUserRole, currentUserId, isOwner, onInvite, onTransfer }: MembersViewProps) {
  const [members, setMembers] = useState<MemberInfo[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirmRemove, setConfirmRemove] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setMembers(null);
    setError(null);
    listMembers(workspaceSlug)
      .then((m) => { if (!cancelled) setMembers(m); })
      .catch((e) => { if (!cancelled) setError(e.message || 'Failed to load members'); });
    return () => { cancelled = true; };
  }, [workspaceSlug]);

  const canManageMember = (m: MemberInfo): boolean => {
    if (!canManage) return false;
    if (m.id === currentUserId) return false;
    if (currentUserRole === 'manager' && (m.role === 'owner' || m.role === 'manager')) return false;
    if (currentUserRole === 'owner' && m.role === 'owner') return false;
    return true;
  };

  const handleRoleChange = async (userId: string, newRole: string) => {
    const old = members;
    setMembers((prev) => prev?.map((m) => m.id === userId ? { ...m, role: newRole } : m) ?? null);
    try {
      await changeMemberRole(workspaceSlug, userId, newRole);
    } catch {
      setMembers(old);
    }
  };

  const handleRemove = async (userId: string) => {
    try {
      await removeMember(workspaceSlug, userId);
      setMembers((prev) => prev?.filter((m) => m.id !== userId) ?? null);
    } catch {
      // silently ignore
    } finally {
      setConfirmRemove(null);
    }
  };

  // ── State views ──

  if (error) {
    return <p style={{ color: C.red, fontSize: 14, padding: '16px 0' }}>Failed to load members: {error}</p>;
  }

  if (members == null) {
    return <p style={{ color: C.muted, fontSize: 14, padding: '16px 0' }}>Loading members...</p>;
  }

  if (members.length === 0) {
    return (
      <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', padding: '64px 0', gap: 12 }}>
        <Users size={32} color={C.faint} />
        <p style={{ color: C.muted, fontSize: 13 }}>No members found.</p>
      </div>
    );
  }

  // ── Loaded ──

  return (
    <div>
      {/* Header */}
      {canManage && (
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 20 }}>
          <div style={{ display: 'flex', alignItems: 'baseline', gap: 10 }}>
            <h1 className="page-title" style={{ marginBottom: 0 }}>
              Members
            </h1>
            <span style={{ fontSize: 24, color: C.faint, fontWeight: 400 }}>{members.length}</span>
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            {isOwner && (
              <button
                onClick={onTransfer}
                style={{
                  display: 'flex', alignItems: 'center', gap: 7,
                  padding: '8px 14px', borderRadius: 6, border: `1px solid ${C.line}`,
                  background: 'transparent', color: C.amber, fontSize: '13.5px', fontWeight: 600,
                  cursor: 'pointer',
                }}
              >
                <svg width={15} height={15} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round"><path d="M17 1l4 4-4 4"/><path d="M3 11V9a4 4 0 0 1 4-4h14"/><path d="M7 23l-4-4 4-4"/><path d="M21 13v2a4 4 0 0 1-4 4H3"/></svg>
                Transfer
              </button>
            )}
            <button
              onClick={onInvite}
              style={{
                display: 'flex', alignItems: 'center', gap: 7,
                padding: '8px 14px', borderRadius: 6, border: 'none',
                background: C.accent, color: '#fff', fontSize: '13.5px', fontWeight: 600,
                cursor: 'pointer',
              }}
            >
              <UserPlus size={15} /> Invite people
            </button>
          </div>
        </div>
      )}

      {/* Table */}
      <div style={{ border: `1px solid ${C.line}`, borderRadius: 6, background: C.panel }}>
        {/* Header row */}
        <div style={{
          display: 'grid', gridTemplateColumns: '1fr 130px 130px 48px',
          gap: 12, padding: '8px 16px', background: C.sidebar,
          borderBottom: `1px solid ${C.line}`,
          borderTopLeftRadius: 6, borderTopRightRadius: 6,
          fontSize: 12, fontWeight: 600,
          color: C.faint, textTransform: 'uppercase', letterSpacing: '0.05em',
        }}>
          <span>Member</span>
          <span>Role</span>
          <span>Active</span>
          <span></span>
        </div>

        {/* Rows */}
        {members.map((m, i) => {
          const manageable = canManageMember(m);
          const isLast = i === members.length - 1;

          return (
            <div
              key={m.id}
              style={{
                display: 'grid', gridTemplateColumns: '1fr 130px 130px 48px',
                gap: 12, padding: '10px 16px', alignItems: 'center',
                borderBottom: isLast ? 'none' : `1px solid ${C.line}`,
                borderBottomLeftRadius: isLast ? 6 : 0,
                borderBottomRightRadius: isLast ? 6 : 0,
                background: C.panel,
              }}
            >
              {/* Avatar + name */}
              <div style={{ display: 'flex', alignItems: 'center', gap: 10, minWidth: 0 }}>
                <div style={{
                  width: 36, height: 36, borderRadius: '50%',
                  background: avatarColor(m.name),
                  display: 'flex', alignItems: 'center', justifyContent: 'center',
                  fontSize: 14, fontWeight: 600, color: '#fff', flexShrink: 0,
                }}>
                  {m.name[0]?.toUpperCase() || '?'}
                </div>
                <div style={{ minWidth: 0 }}>
                  <div style={{
                    fontSize: '14.5px', fontWeight: 550, color: C.ink,
                    overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
                  }}>
                    {m.name}
                  </div>
                  <div style={{
                    fontSize: '12.5px', color: C.muted,
                    overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
                  }}>
                    {m.email || 'No email'}
                  </div>
                </div>
              </div>

              {/* Role — shadcn Select or read-only */}
              <div>
                {manageable ? (
                  <Select value={m.role} onValueChange={(v) => handleRoleChange(m.id, v)}>
                    <SelectTrigger className="h-7 w-28 text-xs">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="owner">Owner</SelectItem>
                      <SelectItem value="manager">Manager</SelectItem>
                      <SelectItem value="editor">Editor</SelectItem>
                      <SelectItem value="viewer">Viewer</SelectItem>
                    </SelectContent>
                  </Select>
                ) : (
                  <span style={{ fontSize: 12, color: C.muted, textTransform: 'capitalize' }}>
                    {m.role}
                  </span>
                )}
              </div>

              {/* Last active */}
              <span style={{ fontSize: 12, color: C.faint }}>
                {relativeTime(m.last_active_at)}
              </span>

              {/* Actions — shadcn DropdownMenu */}
              <div>
                {confirmRemove === m.id ? (
                  <div style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 11, whiteSpace: 'nowrap' }}>
                    <span style={{ color: C.muted }}>Remove {m.name}?</span>
                    <button
                      onClick={() => handleRemove(m.id)}
                      style={{
                        padding: '2px 6px', borderRadius: 4, border: 'none',
                        background: C.red, color: '#fff', fontSize: 11, fontWeight: 500, cursor: 'pointer',
                      }}
                    >
                      Remove
                    </button>
                    <button
                      onClick={() => setConfirmRemove(null)}
                      style={{
                        padding: '2px 6px', borderRadius: 4, border: `1px solid ${C.line}`,
                        background: 'transparent', color: C.ink2, fontSize: 11, cursor: 'pointer',
                      }}
                    >
                      Cancel
                    </button>
                  </div>
                ) : manageable ? (
                  <DropdownMenu>
                    <DropdownMenuTrigger asChild>
                      <button
                        style={{
                          background: 'none', border: 'none', padding: 2,
                          cursor: 'pointer', color: C.faint, borderRadius: 4,
                        }}
                        title="Actions"
                        onMouseEnter={(e) => { e.currentTarget.style.background = C.rail; }}
                        onMouseLeave={(e) => { e.currentTarget.style.background = 'none'; }}
                      >
                        <MoreHorizontal size={16} />
                      </button>
                    </DropdownMenuTrigger>
                    <DropdownMenuContent align="end">
                      <DropdownMenuItem
                        onClick={() => setConfirmRemove(m.id)}
                        style={{ color: C.red }}
                      >
                        Remove member
                      </DropdownMenuItem>
                    </DropdownMenuContent>
                  </DropdownMenu>
                ) : null}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

export default MembersView;
