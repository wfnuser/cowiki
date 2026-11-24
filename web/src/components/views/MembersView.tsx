import { useEffect, useState } from 'react';
import { ArrowLeftRight, Users, UserPlus, MoreHorizontal } from 'lucide-react';
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
import { AvatarBadge } from '@/components/ui/avatar-badge';
import { Button } from '@/components/ui/button';
import { InlineFeedback } from '@/components/ui/inline-feedback';
import { ViewEmpty, ViewHeader } from '@/components/ui/view-header';

// ── Helpers ──

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

  if (error || members == null || members.length === 0) {
    return (
      <section style={{ width: 'min(860px, 100%)' }}>
        <ViewHeader title="Members" />
        {error ? (
          <InlineFeedback title="Could not load members" description={error} />
        ) : (
          <ViewEmpty>{members == null ? 'Loading…' : <><Users size={15} aria-hidden /> No members yet.</>}</ViewEmpty>
        )}
      </section>
    );
  }

  // ── Loaded ──

  const columns = 'minmax(0,1fr) 120px 110px 32px';
  return (
    <section style={{ width: 'min(860px, 100%)' }}>
      <ViewHeader
        title={<>Members <span className="ml-1 font-normal text-text-faint">{members.length}</span></>}
        actions={canManage ? (
          <>
            {isOwner && (
              <Button size="sm" variant="ghost" onClick={onTransfer}>
                <ArrowLeftRight /> Transfer
              </Button>
            )}
            <Button size="sm" onClick={onInvite}>
              <UserPlus /> Invite
            </Button>
          </>
        ) : undefined}
      />

      <div role="table" aria-label="Members">
        <div role="row" className="grid items-center gap-3 border-b border-border-soft px-1 pb-1.5 text-meta text-text-tertiary" style={{ gridTemplateColumns: columns }}>
          <span role="columnheader">Name</span>
          <span role="columnheader">Role</span>
          <span role="columnheader">Active</span>
          <span role="columnheader" aria-label="Actions" />
        </div>
        {members.map((m) => {
          const manageable = canManageMember(m);
          return (
            <div key={m.id} role="row" className="grid min-h-12 items-center gap-3 border-b border-border-soft px-1 py-2" style={{ gridTemplateColumns: columns }}>
              <div role="cell" className="flex min-w-0 items-center gap-2.5">
                <AvatarBadge name={m.name} size={28} kind="member" />
                <div className="min-w-0">
                  <div className="truncate text-small font-medium text-text">{m.name}</div>
                  {m.email && <div className="truncate text-meta text-text-tertiary">{m.email}</div>}
                </div>
              </div>

              <div role="cell">
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
                  <span className="text-small capitalize text-text-secondary">{m.role}</span>
                )}
              </div>

              <span role="cell" className="text-meta text-text-tertiary">{relativeTime(m.last_active_at)}</span>

              <div role="cell" className="flex justify-end">
                {manageable && (
                  <DropdownMenu>
                    <DropdownMenuTrigger asChild>
                      <button type="button" aria-label={`Actions for ${m.name}`} title="Actions" className="flex size-7 items-center justify-center rounded-md text-text-faint hover:bg-bg-hover hover:text-text">
                        <MoreHorizontal size={16} />
                      </button>
                    </DropdownMenuTrigger>
                    <DropdownMenuContent align="end">
                      <DropdownMenuItem onClick={() => setConfirmRemove(m.id)} className="text-red focus:text-red">
                        Remove member
                      </DropdownMenuItem>
                    </DropdownMenuContent>
                  </DropdownMenu>
                )}
              </div>
              {confirmRemove === m.id && (
                <div role="cell" className="col-span-full flex items-center justify-end gap-2 pb-1 text-small text-text-secondary">
                  Remove {m.name} from this Space?
                  <Button size="xs" variant="outline" onClick={() => setConfirmRemove(null)}>Cancel</Button>
                  <Button size="xs" variant="destructive" onClick={() => handleRemove(m.id)}>Remove</Button>
                </div>
              )}
            </div>
          );
        })}
      </div>
    </section>
  );
}

export default MembersView;
