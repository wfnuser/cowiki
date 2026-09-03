import { useEffect, useState, type FormEvent } from 'react';
import { Link2 } from 'lucide-react';
import { useLocation, useNavigate } from 'react-router-dom';
import { SpaceRail } from '../components/layout/SpaceRail';
import { Button } from '../components/ui/button';
import { Input } from '../components/ui/input';
import { InlineFeedback } from '../components/ui/inline-feedback';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '../components/ui/select';
import { TooltipProvider } from '../components/ui/tooltip';
import {
  CloudApiError,
  type CloudClient,
  type CloudSpace,
  type CloudSpaceCreationCapability,
} from './client';
import { spaceCreationPanelMode } from './cloud-shell-model';
import { cloudSpaceRoute } from './routes';
import type { CloudSession } from './session';
import type { CloudVisibility } from './session';

interface CloudHomeProps {
  client: CloudClient;
  session: CloudSession;
  onSignOut: () => void;
}

type HomePanel = 'create' | null;

export function CloudHome({ client, session, onSignOut }: CloudHomeProps) {
  const navigate = useNavigate();
  const location = useLocation();
  const [spaces, setSpaces] = useState<CloudSpace[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [creating, setCreating] = useState(false);
  const [capability, setCapability] = useState<CloudSpaceCreationCapability | null>(null);
  const [capabilityLoading, setCapabilityLoading] = useState(false);
  const [inviteCode, setInviteCode] = useState('');
  const [redeeming, setRedeeming] = useState(false);
  const [name, setName] = useState('');
  const [slug, setSlug] = useState('');
  const [slugEdited, setSlugEdited] = useState(false);
  const [visibility, setVisibility] = useState<CloudVisibility>('private');
  const panel = panelFromSearch(location.search);

  useEffect(() => {
    let active = true;
    void client.listSpaces()
      .then((value) => { if (active) setSpaces(value); })
      .catch((cause) => {
        if (active) {
          setError(cause instanceof Error ? cause.message : 'Could not load Cloud Spaces.');
        }
      })
      .finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [client]);

  useEffect(() => {
    if (panel !== 'create') return undefined;
    let active = true;
    setCapabilityLoading(true);
    setError('');
    void client.getSpaceCreationCapability()
      .then((value) => { if (active) setCapability(value); })
      .catch((cause) => {
        if (active) {
          setError(cause instanceof Error ? cause.message : 'Could not check Space creation access.');
        }
      })
      .finally(() => { if (active) setCapabilityLoading(false); });
    return () => { active = false; };
  }, [client, panel]);

  const openCreatePanel = () => {
    setError('');
    navigate('/cloud?action=create', { replace: true });
  };

  const closePanel = () => {
    navigate('/cloud', { replace: true });
  };

  const createSharedSpace = async (event: FormEvent) => {
    event.preventDefault();
    setCreating(true);
    setError('');
    try {
      const created = await client.createSpace(name.trim(), slug.trim(), visibility);
      navigate(cloudSpaceRoute(created.id));
    } catch (cause) {
      if (cause instanceof CloudApiError
        && (cause.code === 'invite_required' || cause.code === 'limit_reached')) {
        try {
          setCapability(await client.getSpaceCreationCapability());
        } catch {
          // Keep the authoritative creation error when refreshing capability also fails.
        }
      }
      setError(cause instanceof Error ? cause.message : 'Could not create this shared Space.');
    } finally {
      setCreating(false);
    }
  };

  const redeemInvite = async (event: FormEvent) => {
    event.preventDefault();
    setRedeeming(true);
    setError('');
    try {
      const unlocked = await client.redeemSpaceCreationInvite(inviteCode);
      setCapability(unlocked);
      setInviteCode('');
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Could not redeem this invite code.');
    } finally {
      setRedeeming(false);
    }
  };

  const changeName = (value: string) => {
    setName(value);
    if (!slugEdited) setSlug(spaceSlug(value));
  };

  return (
    <TooltipProvider>
      <div className="flex h-screen overflow-hidden bg-bg text-text">
        <SpaceRail
          workspaces={spaces}
          activeWorkspaceId={null}
          userName={session.userName}
          onSelectWorkspace={(space) => navigate(cloudSpaceRoute(space.id))}
          onCreateWorkspace={openCreatePanel}
          onSettings={() => undefined}
          onDiscover={() => undefined}
          onLogout={onSignOut}
          onConnectCloud={() => undefined}
          notifUnread={0}
          onShowNotifications={() => undefined}
          showBell={false}
          showCloudActions
          showSettings={false}
          showDiscover={false}
          titlebarInset={false}
          createLabel="New shared Space"
        />

        <main className="relative min-w-0 flex-1 overflow-auto bg-bg">
          <div className="mx-auto flex min-h-full w-full max-w-3xl items-center justify-center px-8 py-12">
            <div className="w-full max-w-md">
              {error && <CloudNotice tone="error">{error}</CloudNotice>}

              {panel === 'create' ? (
                <section aria-labelledby="create-space-title" className="rounded-xl border bg-panel p-6">
                  <div className="mb-6 flex items-start justify-between gap-4">
                    <div>
                      <h1 id="create-space-title" className="font-serif text-2xl font-semibold tracking-[-0.02em]">
                        New shared Space
                      </h1>
                      <p className="mt-1.5 text-sm text-text-tertiary">
                        Connect a local repository after the Space is created.
                      </p>
                    </div>
                    <button className="text-xs text-text-tertiary hover:text-text" type="button" onClick={closePanel}>
                      Cancel
                    </button>
                  </div>
                  {capabilityLoading || !capability ? (
                    <div
                      aria-label="Checking Space creation access"
                      className="h-24 animate-pulse rounded-lg bg-secondary"
                    />
                  ) : spaceCreationPanelMode(capability) === 'invite' ? (
                    <div>
                      <p className="mb-4 text-sm leading-6 text-text-secondary">
                        Your GitHub account can join existing shared Spaces. Enter a one-time
                        invite code to unlock creation for this account.
                      </p>
                      <form className="space-y-3" onSubmit={(event) => void redeemInvite(event)}>
                        <Input
                          aria-label="Space creation invite code"
                          autoComplete="off"
                          placeholder="cw_space_…"
                          value={inviteCode}
                          onChange={(event) => setInviteCode(event.target.value)}
                        />
                        <Button
                          className="w-full"
                          type="submit"
                          disabled={redeeming || !inviteCode.trim()}
                        >
                          {redeeming ? 'Unlocking…' : 'Unlock Space creation'}
                        </Button>
                      </form>
                    </div>
                  ) : spaceCreationPanelMode(capability) === 'limit' ? (
                    <div className="rounded-lg bg-secondary px-4 py-5 text-sm leading-6 text-text-secondary">
                      <p className="font-medium text-text">
                        You have created {capability.createdCount} of {capability.limit} shared Spaces.
                      </p>
                      <p className="mt-1.5">
                        Your creation limit has been reached. You can still join existing shared Spaces.
                      </p>
                    </div>
                  ) : (
                    <form className="space-y-3" onSubmit={(event) => void createSharedSpace(event)}>
                      <p className="text-xs text-text-tertiary">
                        {capability.createdCount} of {capability.limit} shared Spaces created
                      </p>
                      <Input
                        aria-label="Shared Space name"
                        placeholder="Space name"
                        value={name}
                        onChange={(event) => changeName(event.target.value)}
                      />
                      <Select
                        value={visibility}
                        onValueChange={(value) => setVisibility(value as CloudVisibility)}
                      >
                        <SelectTrigger aria-label="Space visibility" className="w-full">
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent>
                          <SelectItem value="private">Private — members only</SelectItem>
                          <SelectItem value="public">Public — anyone can read merged pages</SelectItem>
                        </SelectContent>
                      </Select>
                      <Input
                        aria-label="Shared Space slug"
                        placeholder="space-name"
                        value={slug}
                        onChange={(event) => {
                          setSlug(event.target.value);
                          setSlugEdited(true);
                        }}
                      />
                      <Button className="w-full" type="submit" disabled={creating || !name.trim() || !slug.trim()}>
                        {creating ? 'Creating…' : 'Create Space'}
                      </Button>
                    </form>
                  )}
                </section>
              ) : loading ? (
                <div className="mx-auto size-6 animate-pulse rounded-md bg-secondary" aria-label="Loading Spaces" />
              ) : spaces.length === 0 ? (
                <section className="text-center">
                  <div className="mx-auto mb-5 grid size-11 place-items-center rounded-xl bg-accent-soft text-accent">
                    <Link2 size={19} />
                  </div>
                  <h1 className="font-serif text-2xl font-semibold tracking-[-0.02em]">Join a Space</h1>
                  <p className="mx-auto mt-2 max-w-xs text-sm leading-6 text-text-tertiary">
                    Open an invitation link from a Space owner or manager.
                  </p>
                </section>
              ) : (
                <section className="text-center">
                  <h1 className="font-serif text-2xl font-semibold tracking-[-0.02em]">Select a Space</h1>
                  <p className="mt-2 text-sm text-text-tertiary">Choose one from the rail.</p>
                </section>
              )}
            </div>
          </div>
        </main>
      </div>
    </TooltipProvider>
  );
}

export function SpaceMonogram({ name }: { name: string }) {
  return <div className="grid size-10 place-items-center rounded-[11px] bg-[#5d8a6c] text-sm font-bold text-white">{name[0]?.toUpperCase() || 'S'}</div>;
}

export function CloudNotice({ children, tone = 'neutral' }: { children: React.ReactNode; tone?: 'neutral' | 'error' | 'success' }) {
  if (tone === 'error') {
    return <InlineFeedback className="mb-5" title="Cloud action failed" description={children} />;
  }
  const styles = tone === 'success'
    ? 'border-green/20 bg-green-soft text-green'
    : 'border-border bg-secondary text-text-secondary';
  return <div className={`mb-5 rounded-lg border px-4 py-3 text-sm ${styles}`}>{children}</div>;
}

function panelFromSearch(search: string): HomePanel {
  const action = new URLSearchParams(search).get('action');
  return action === 'create' ? action : null;
}

function spaceSlug(value: string): string {
  return value
    .toLowerCase()
    .normalize('NFKD')
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 63);
}
