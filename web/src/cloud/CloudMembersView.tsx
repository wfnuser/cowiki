import { useEffect, useState } from 'react';
import { Copy, Globe2, LockKeyhole, Users } from 'lucide-react';
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
import {
  canManageMembers,
  canManageTarget,
  type CloudRole,
  type CloudVisibility,
} from './session';

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
  const [visibility, setVisibility] = useState<CloudVisibility>(space.visibility);
  const [inviteRole, setInviteRole] = useState<'editor' | 'viewer'>('editor');
  const [creatingInvite, setCreatingInvite] = useState(false);
  const [inviteUrl, setInviteUrl] = useState('');
  const [notice, setNotice] = useState<{ tone: 'error' | 'success'; message: string } | null>(null);
  const canManageRoles = canManageMembers(space.role);

  useEffect(() => setVisibility(space.visibility), [space.visibility]);

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

  const changeVisibility = async (next: CloudVisibility) => {
    setPending('visibility');
    setNotice(null);
    try {
      const updated = await client.updateSpaceVisibility(space.id, next);
      setVisibility(updated.visibility);
      setNotice({
        tone: 'success',
        message: next === 'public'
          ? 'This Space is now readable by anyone with its link.'
          : 'This Space is now visible only to members.',
      });
    } catch (cause) {
      setNotice({
        tone: 'error',
        message: cause instanceof Error ? cause.message : 'Could not update visibility.',
      });
    } finally {
      setPending('');
    }
  };

  const createInvite = async () => {
    setCreatingInvite(true);
    setNotice(null);
    try {
      const invitation = await client.createInvitation(space.id, inviteRole, 168);
      setInviteUrl(invitation.inviteUrl || '');
      setNotice({ tone: 'success', message: 'Invite link created. It expires in 7 days.' });
    } catch (cause) {
      setNotice({
        tone: 'error',
        message: cause instanceof Error ? cause.message : 'Could not create an invitation.',
      });
    } finally {
      setCreatingInvite(false);
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

        <section className="mb-6 grid gap-4 rounded-xl border bg-panel p-4 md:grid-cols-2">
          <div>
            <div className="mb-2 flex items-center gap-2 text-sm font-semibold text-text">
              {visibility === 'public' ? <Globe2 size={16} /> : <LockKeyhole size={16} />}
              Visibility
            </div>
            {canManageRoles ? (
              <Select
                value={visibility}
                disabled={pending === 'visibility'}
                onValueChange={(value) => void changeVisibility(value as CloudVisibility)}
              >
                <SelectTrigger aria-label="Space visibility" className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="private">Private — members only</SelectItem>
                  <SelectItem value="public">Public — anyone can read</SelectItem>
                </SelectContent>
              </Select>
            ) : (
              <p className="text-sm capitalize text-text-tertiary">{visibility}</p>
            )}
            {visibility === 'public' && (
              <a
                className="mt-2 block truncate text-xs text-accent hover:underline"
                href={space.publicUrl}
                rel="noreferrer"
                target="_blank"
              >
                {space.publicUrl}
              </a>
            )}
          </div>

          <div>
            <div className="mb-2 text-sm font-semibold text-text">Invite link</div>
            {canManageRoles ? (
              <div className="space-y-2">
                <div className="flex gap-2">
                  <Select
                    value={inviteRole}
                    onValueChange={(value) => setInviteRole(value as 'editor' | 'viewer')}
                  >
                    <SelectTrigger aria-label="Invitation role" className="min-w-28 flex-1">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="editor">Editor — submit PRs</SelectItem>
                      <SelectItem value="viewer">Viewer — read only</SelectItem>
                    </SelectContent>
                  </Select>
                  <button
                    className="rounded-md bg-accent px-3 text-xs font-semibold text-on-accent disabled:opacity-50"
                    disabled={creatingInvite}
                    type="button"
                    onClick={() => void createInvite()}
                  >
                    {creatingInvite ? 'Creating…' : 'Create'}
                  </button>
                </div>
                {inviteUrl && (
                  <div className="flex items-center gap-2 rounded-md bg-secondary px-3 py-2">
                    <span className="min-w-0 flex-1 truncate text-xs text-text-secondary">{inviteUrl}</span>
                    <button
                      aria-label="Copy invite link"
                      className="text-text-tertiary hover:text-text"
                      type="button"
                      onClick={() => void navigator.clipboard.writeText(inviteUrl)}
                    >
                      <Copy size={14} />
                    </button>
                  </div>
                )}
              </div>
            ) : (
              <p className="text-sm text-text-tertiary">Ask an Owner or Manager for an invitation.</p>
            )}
          </div>
        </section>

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
              const editable = canManageRoles && canManageTarget(space.role, member.role);
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
