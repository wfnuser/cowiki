import { useEffect, useState } from 'react';
import { Users } from 'lucide-react';
import { AvatarBadge } from '../components/ui/avatar-badge';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '../components/ui/select';
import { C } from '../lib/design';
import type { CloudClient, CloudMember, CloudSpace } from './client';
import { CloudNotice } from './CloudHome';
import type { CloudRole } from './session';

const editableRoles: Array<Exclude<CloudRole, 'owner'>> = ['manager', 'editor', 'viewer'];

export function CloudMembersView({
  client,
  space,
  currentUserId,
}: {
  client: CloudClient;
  space: CloudSpace;
  currentUserId: string;
}) {
  const [members, setMembers] = useState<CloudMember[] | null>(null);
  const [pending, setPending] = useState('');
  const [notice, setNotice] = useState<{ tone: 'error' | 'success'; message: string } | null>(null);
  const canManageRoles = space.role === 'owner';

  const loadMembers = async () => {
    try {
      setMembers(await client.listMembers(space.id));
    } catch (cause) {
      setNotice({
        tone: 'error',
        message: cause instanceof Error ? cause.message : 'Could not load members.',
      });
    }
  };

  useEffect(() => {
    let active = true;
    void client.listMembers(space.id)
      .then((value) => { if (active) setMembers(value); })
      .catch((cause) => {
        if (active) {
          setNotice({
            tone: 'error',
            message: cause instanceof Error ? cause.message : 'Could not load members.',
          });
        }
      });
    return () => { active = false; };
  }, [client, space.id]);

  const changeRole = async (member: CloudMember, role: Exclude<CloudRole, 'owner'>) => {
    setPending(member.userId);
    setNotice(null);
    try {
      await client.setMember(space.id, member.handle, role);
      setNotice({ tone: 'success', message: `${member.displayName} is now ${capitalize(role)}.` });
    } catch (cause) {
      setNotice({
        tone: 'error',
        message: cause instanceof Error ? cause.message : 'Could not update this role.',
      });
    } finally {
      await loadMembers();
      setPending('');
    }
  };

  return (
    <div className="h-full overflow-y-auto">
      <div style={{ padding: '36px 56px 56px' }}>
        <div className="mb-5 flex items-baseline gap-3">
          <h1 className="page-title mb-0">Members</h1>
          {members && <span className="text-2xl font-normal text-text-tertiary">{members.length}</span>}
        </div>

        {notice && <CloudNotice tone={notice.tone}>{notice.message}</CloudNotice>}

        {members == null ? (
          <p className="py-4 text-sm text-text-tertiary">Loading members…</p>
        ) : members.length === 0 ? (
          <div className="flex flex-col items-center gap-3 py-16 text-sm text-text-tertiary">
            <Users size={30} /> No members found.
          </div>
        ) : (
          <div className="overflow-hidden rounded-xl border bg-panel">
            <div
              className="grid gap-3 border-b bg-bg-secondary px-4 py-2 text-xs font-semibold uppercase tracking-[0.05em] text-text-tertiary"
              style={{ gridTemplateColumns: 'minmax(0, 1fr) 150px' }}
            >
              <span>Member</span>
              <span>Role</span>
            </div>

            {members.map((member, index) => {
              const editable = canManageRoles && member.role !== 'owner';
              return (
                <div
                  key={member.userId}
                  className="grid min-h-16 items-center gap-3 px-4 py-2.5"
                  style={{
                    gridTemplateColumns: 'minmax(0, 1fr) 150px',
                    borderBottom: index === members.length - 1 ? 'none' : `1px solid ${C.line}`,
                  }}
                >
                  <div className="flex min-w-0 items-center gap-3">
                    <AvatarBadge name={member.displayName} size={36} />
                    <div className="min-w-0">
                      <div className="truncate text-sm font-semibold text-text">
                        {member.displayName}
                        {member.userId === currentUserId && (
                          <span className="ml-2 text-xs font-normal text-text-tertiary">You</span>
                        )}
                      </div>
                      <div className="truncate text-xs text-text-tertiary">@{member.handle}</div>
                    </div>
                  </div>

                  {editable ? (
                    <Select
                      value={member.role}
                      disabled={pending === member.userId}
                      onValueChange={(value) => void changeRole(
                        member,
                        value as Exclude<CloudRole, 'owner'>,
                      )}
                    >
                      <SelectTrigger className="h-8 w-32 text-xs">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        {editableRoles.map((role) => (
                          <SelectItem key={role} value={role}>{capitalize(role)}</SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  ) : (
                    <span className="text-xs capitalize text-text-tertiary">{member.role}</span>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}

function capitalize(value: string): string {
  return value[0]?.toUpperCase() + value.slice(1);
}
