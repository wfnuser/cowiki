import { useCallback, useEffect, useState } from 'react';
import { Ban, Copy, Link2, Shield, Trash2, UserPlus } from 'lucide-react';
import { AvatarBadge } from '../components/ui/avatar-badge';
import { Button } from '../components/ui/button';
import { Input } from '../components/ui/input';
import type {
  CloudClient,
  CloudInvitation,
  CloudInvitableRole,
  CloudMember,
  CloudSpace,
} from './client';
import { memberManagementMode } from './cloud-shell-model';
import { CloudNotice } from './CloudHome';
import type { CloudRole } from './session';

const editableRoles: CloudRole[] = ['manager', 'editor', 'viewer'];
const invitationRoles: CloudInvitableRole[] = ['manager', 'editor', 'viewer'];

export function CloudMembersView({ client, space, currentUserId }: { client: CloudClient; space: CloudSpace; currentUserId: string }) {
  const [members, setMembers] = useState<CloudMember[]>([]);
  const [invitations, setInvitations] = useState<CloudInvitation[]>([]);
  const [handle, setHandle] = useState('');
  const [role, setRole] = useState<CloudRole>('editor');
  const [invitationRole, setInvitationRole] = useState<CloudInvitableRole>('editor');
  const [expiresInHours, setExpiresInHours] = useState(168);
  const [latestInviteUrl, setLatestInviteUrl] = useState('');
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
  useEffect(() => {
    let active = true;
    void client.listMembers(space.id)
      .then((value) => { if (active) setMembers(value); })
      .catch((cause) => {
        if (active) {
          setNotice({ tone: 'error', message: cause instanceof Error ? cause.message : 'Could not load members.' });
        }
      });
    return () => { active = false; };
  }, [client, space.id]);

  const loadInvitations = useCallback(async () => {
    if (mode !== 'manage') return;
    try {
      setInvitations(await client.listInvitations(space.id));
    } catch (cause) {
      setNotice({ tone: 'error', message: cause instanceof Error ? cause.message : 'Could not load invitations.' });
    }
  }, [client, mode, space.id]);
  useEffect(() => {
    if (mode !== 'manage') return;
    let active = true;
    void client.listInvitations(space.id)
      .then((value) => { if (active) setInvitations(value); })
      .catch((cause) => {
        if (active) {
          setNotice({ tone: 'error', message: cause instanceof Error ? cause.message : 'Could not load invitations.' });
        }
      });
    return () => { active = false; };
  }, [client, mode, space.id]);

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

  const createInvitation = async () => {
    setPending('create-invitation');
    setNotice(null);
    try {
      const invitation = await client.createInvitation(
        space.id,
        invitationRole,
        expiresInHours,
      );
      setLatestInviteUrl(invitation.inviteUrl || '');
      setNotice({ tone: 'success', message: 'Invitation link created for this Space.' });
    } catch (cause) {
      setNotice({ tone: 'error', message: cause instanceof Error ? cause.message : 'Could not create invitation.' });
    } finally {
      await loadInvitations();
      setPending('');
    }
  };

  const revokeInvitation = async (invitation: CloudInvitation) => {
    setPending(invitation.id);
    setNotice(null);
    try {
      await client.revokeInvitation(space.id, invitation.id);
      setNotice({ tone: 'success', message: 'Invitation revoked.' });
      setLatestInviteUrl('');
    } catch (cause) {
      setNotice({ tone: 'error', message: cause instanceof Error ? cause.message : 'Could not revoke invitation.' });
    } finally {
      await loadInvitations();
      setPending('');
    }
  };

  const copyInviteUrl = async () => {
    if (!latestInviteUrl) return;
    try {
      await navigator.clipboard.writeText(latestInviteUrl);
      setNotice({ tone: 'success', message: 'Invitation link copied.' });
    } catch {
      setNotice({ tone: 'error', message: 'Copy failed. Select the link and copy it manually.' });
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
        <>
          <section className="mb-7 rounded-xl border bg-panel p-5">
            <div className="mb-4 flex items-start gap-3">
              <div className="grid size-9 place-items-center rounded-lg bg-accent-soft text-accent"><Link2 size={17} /></div>
              <div>
                <h2 className="text-sm font-semibold">Invite link</h2>
                <p className="mt-1 text-xs text-text-tertiary">The link grants access only to this Space after GitHub sign-in.</p>
              </div>
            </div>
            <div className="grid grid-cols-[150px_150px_auto] gap-2">
              <select aria-label="Invitation role" className="h-9 rounded-md border bg-bg px-3 text-sm" value={invitationRole} onChange={(event) => setInvitationRole(event.target.value as CloudInvitableRole)}>
                {invitationRoles.map((value) => <option key={value} value={value}>{capitalize(value)}</option>)}
              </select>
              <select aria-label="Invitation expiry" className="h-9 rounded-md border bg-bg px-3 text-sm" value={expiresInHours} onChange={(event) => setExpiresInHours(Number(event.target.value))}>
                <option value={24}>One day</option>
                <option value={168}>Seven days</option>
                <option value={720}>Thirty days</option>
              </select>
              <Button disabled={!!pending} onClick={() => void createInvitation()}><Link2 /> Create link</Button>
            </div>
            {latestInviteUrl && (
              <div className="mt-3 flex gap-2">
                <Input aria-label="New invitation link" readOnly value={latestInviteUrl} onFocus={(event) => event.currentTarget.select()} />
                <Button variant="outline" onClick={() => void copyInviteUrl()}><Copy /> Copy link</Button>
              </div>
            )}
            {invitations.length > 0 && (
              <div className="mt-4 border-t pt-3">
                {invitations.map((invitation) => (
                  <div key={invitation.id} className="flex items-center gap-3 py-2 text-xs">
                    <span className="w-20 font-semibold capitalize">{invitation.role}</span>
                    <span className="flex-1 text-text-tertiary">
                      Expires {new Date(invitation.expiresAt).toLocaleString()} · {invitation.acceptedCount} joined
                    </span>
                    <Button variant="ghost" size="sm" disabled={!!pending} onClick={() => void revokeInvitation(invitation)}><Ban /> Revoke</Button>
                  </div>
                ))}
              </div>
            )}
          </section>
          <form className="mb-7 grid grid-cols-[1fr_150px_auto] gap-2 rounded-xl border bg-panel p-4" onSubmit={(event) => { event.preventDefault(); if (handle.trim()) void saveMember(handle.trim(), role); }}>
            <Input aria-label="GitHub handle" placeholder="Existing GitHub user" value={handle} onChange={(event) => setHandle(event.target.value)} />
            <select className="h-9 rounded-md border bg-bg px-3 text-sm" value={role} onChange={(event) => setRole(event.target.value as CloudRole)}>
              {editableRoles.map((value) => <option key={value} value={value}>{capitalize(value)}</option>)}
            </select>
            <Button type="submit" disabled={!handle.trim() || !!pending}><UserPlus /> Add directly</Button>
          </form>
        </>
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
