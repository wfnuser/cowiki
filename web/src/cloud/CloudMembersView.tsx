import { useCallback, useEffect, useState } from 'react';
import { Shield, Trash2, UserPlus } from 'lucide-react';
import { AvatarBadge } from '../components/ui/avatar-badge';
import { Button } from '../components/ui/button';
import { Input } from '../components/ui/input';
import type { CloudClient, CloudMember, CloudSpace } from './client';
import { memberManagementMode } from './cloud-shell-model';
import { CloudNotice } from './CloudHome';
import type { CloudRole } from './session';

const editableRoles: CloudRole[] = ['manager', 'editor', 'viewer'];

export function CloudMembersView({ client, space, currentUserId }: { client: CloudClient; space: CloudSpace; currentUserId: string }) {
  const [members, setMembers] = useState<CloudMember[]>([]);
  const [handle, setHandle] = useState('');
  const [role, setRole] = useState<CloudRole>('editor');
  const [pending, setPending] = useState('');
  const [notice, setNotice] = useState<{ tone: 'error' | 'success'; message: string } | null>(null);
  const mode = memberManagementMode(space.role);

  const loadMembers = useCallback(async () => {
    try {
      setMembers(await client.listMembers(space.id));
    } catch (cause) {
      setNotice({ tone: 'error', message: cause instanceof Error ? cause.message : 'Could not load members.' });
    }
  }, [client, space.id]);
  useEffect(() => { void loadMembers(); }, [loadMembers]);

  const saveMember = async (memberHandle: string, nextRole: CloudRole) => {
    setPending(memberHandle);
    setNotice(null);
    try {
      await client.setMember(space.id, memberHandle, nextRole);
      setHandle('');
      setNotice({ tone: 'success', message: `${memberHandle} now has the ${nextRole} role.` });
    } catch (cause) {
      setNotice({ tone: 'error', message: cause instanceof Error ? cause.message : 'Could not update this member.' });
    } finally {
      await loadMembers();
      setPending('');
    }
  };

  const removeMember = async (member: CloudMember) => {
    setPending(member.handle);
    setNotice(null);
    try {
      await client.removeMember(space.id, member.userId);
      setNotice({ tone: 'success', message: `${member.displayName} was removed from this Space.` });
    } catch (cause) {
      setNotice({ tone: 'error', message: cause instanceof Error ? cause.message : 'Could not remove this member.' });
    } finally {
      await loadMembers();
      setPending('');
    }
  };

  return (
    <div className="mx-auto w-full max-w-4xl px-10 py-12">
      <div className="mb-8">
        <h1 className="font-serif text-4xl font-bold">Members</h1>
        <p className="mt-2 text-sm text-text-tertiary">Roles govern Cloud content access, user-branch pushes, and merges into main.</p>
      </div>
      {notice && <CloudNotice tone={notice.tone}>{notice.message}</CloudNotice>}
      {mode === 'manage' && (
        <form className="mb-7 grid grid-cols-[1fr_150px_auto] gap-2 rounded-xl border bg-panel p-4" onSubmit={(event) => { event.preventDefault(); if (handle.trim()) void saveMember(handle.trim(), role); }}>
          <Input aria-label="GitHub handle" placeholder="GitHub handle" value={handle} onChange={(event) => setHandle(event.target.value)} />
          <select className="h-9 rounded-md border bg-bg px-3 text-sm" value={role} onChange={(event) => setRole(event.target.value as CloudRole)}>
            {editableRoles.map((value) => <option key={value} value={value}>{capitalize(value)}</option>)}
          </select>
          <Button type="submit" disabled={!handle.trim() || !!pending}><UserPlus /> Add or update</Button>
        </form>
      )}
      {mode === 'read' && <CloudNotice>You can view membership. Owners and managers can change roles.</CloudNotice>}

      <div className="overflow-hidden rounded-xl border bg-panel">
        {members.map((member) => (
          <div key={member.userId} className="flex min-h-16 items-center gap-3 border-b px-5 py-3 last:border-b-0">
            <AvatarBadge name={member.displayName} size={32} />
            <div className="min-w-0 flex-1"><div className="truncate text-sm font-semibold">{member.displayName}{member.userId === currentUserId && <span className="ml-2 text-xs font-normal text-text-tertiary">You</span>}</div><div className="text-xs text-text-tertiary">@{member.handle}</div></div>
            {mode === 'manage' && member.role !== 'owner' ? (
              <>
                <select aria-label={`Role for ${member.handle}`} className="h-8 rounded-md border bg-bg px-2 text-xs capitalize" value={member.role} disabled={!!pending} onChange={(event) => void saveMember(member.handle, event.target.value as CloudRole)}>
                  {editableRoles.map((value) => <option key={value} value={value}>{capitalize(value)}</option>)}
                </select>
                <Button variant="ghost" size="icon-sm" aria-label={`Remove ${member.handle}`} disabled={!!pending} onClick={() => void removeMember(member)}><Trash2 /></Button>
              </>
            ) : (
              <span className="inline-flex items-center gap-1.5 rounded-full bg-secondary px-3 py-1 text-xs font-semibold capitalize text-text-secondary"><Shield size={12} />{member.role}</span>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}

function capitalize(value: string): string {
  return value[0].toUpperCase() + value.slice(1);
}

