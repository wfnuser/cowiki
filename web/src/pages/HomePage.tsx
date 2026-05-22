import { useEffect, useState } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import { Plus, Clock, LogOut, Globe, Lock, FileText, Compass, Users } from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarInset,
} from '@/components/ui/sidebar';
import { TooltipProvider } from '@/components/ui/tooltip';
import { listWorkspaces, createWorkspace, type Workspace } from '../api';
import { getStoredAuth, clearAuth } from '../auth';

type ContentView = 'home' | 'discover';

export function HomePage() {
  const [mySpaces, setMySpaces] = useState<Workspace[]>([]);
  const [loading, setLoading] = useState(true);
  const [contentView, setContentView] = useState<ContentView>('home');
  const [showCreate, setShowCreate] = useState(false);
  const [newName, setNewName] = useState('');
  const [newSlug, setNewSlug] = useState('');
  const [newVisibility, setNewVisibility] = useState<'private' | 'public'>('private');
  const [creating, setCreating] = useState(false);
  const navigate = useNavigate();
  const auth = getStoredAuth();

  const load = async () => {
    setLoading(true);
    try {
      const mine = await listWorkspaces();
      setMySpaces(mine);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => { load(); }, []);

  // Split into personal (private, only me) and teamspaces (shared)
  const personalSpaces = mySpaces.filter((w) => w.visibility === 'private' && w.role === 'owner');
  const teamspaces = mySpaces.filter((w) => !(w.visibility === 'private' && w.role === 'owner'));

  const handleNameChange = (name: string) => {
    setNewName(name);
    setNewSlug(name.toLowerCase().replace(/[^a-z0-9\s-]/g, '').replace(/\s+/g, '-').replace(/-+/g, '-').trim());
  };

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newName.trim() || !newSlug.trim()) return;
    setCreating(true);
    try {
      const ws = await createWorkspace(newName.trim(), newSlug.trim(), newVisibility);
      setShowCreate(false);
      setNewName('');
      setNewSlug('');
      setNewVisibility('private');
      navigate(`/w/${ws.slug}`);
    } finally {
      setCreating(false);
    }
  };

  const handleLogout = () => {
    clearAuth();
    navigate('/login');
  };

  return (
    <>
      <TooltipProvider>
        <SidebarProvider>
          <Sidebar>
            <SidebarHeader>
              <div className="flex items-center gap-2 px-2 py-1">
                <div className="flex h-6 w-6 items-center justify-center rounded-md bg-sidebar-primary text-sidebar-primary-foreground">
                  <span className="text-xs font-bold">c</span>
                </div>
                <span className="font-semibold text-sm">CoWiki</span>
              </div>
            </SidebarHeader>
            <SidebarContent>
              {/* Personal Space */}
              <SidebarGroup>
                <SidebarGroupLabel>Personal Space</SidebarGroupLabel>
                <SidebarMenu>
                  {personalSpaces.map((ws) => (
                    <SidebarMenuItem key={ws.id}>
                      <SidebarMenuButton asChild tooltip={ws.name}>
                        <Link to={`/w/${ws.slug}`}>
                          <FileText size={16} />
                          <span>{ws.name}</span>
                        </Link>
                      </SidebarMenuButton>
                    </SidebarMenuItem>
                  ))}
                  {!loading && personalSpaces.length === 0 && (
                    <SidebarMenuItem>
                      <SidebarMenuButton
                        onClick={() => { setNewVisibility('private'); setShowCreate(true); }}
                        tooltip="Create personal space"
                      >
                        <Plus size={16} />
                        <span className="text-sidebar-foreground/50">New personal space</span>
                      </SidebarMenuButton>
                    </SidebarMenuItem>
                  )}
                </SidebarMenu>
              </SidebarGroup>

              {/* Teamspaces */}
              <SidebarGroup>
                <SidebarGroupLabel>
                  <span>Teamspaces</span>
                  <button
                    onClick={() => { setNewVisibility('public'); setShowCreate(true); }}
                    className="ml-auto text-sidebar-foreground/40 hover:text-sidebar-foreground/70 transition-colors"
                  >
                    <Plus size={14} />
                  </button>
                </SidebarGroupLabel>
                <SidebarMenu>
                  {teamspaces.map((ws) => (
                    <SidebarMenuItem key={ws.id}>
                      <SidebarMenuButton asChild tooltip={ws.name}>
                        <Link to={`/w/${ws.slug}`}>
                          <Users size={16} />
                          <span>{ws.name}</span>
                        </Link>
                      </SidebarMenuButton>
                    </SidebarMenuItem>
                  ))}
                  {!loading && teamspaces.length === 0 && (
                    <div className="px-2 py-3 text-xs text-sidebar-foreground/40 text-center">
                      No teamspaces yet
                    </div>
                  )}
                </SidebarMenu>
              </SidebarGroup>

              {/* Discover */}
              <SidebarGroup>
                <SidebarMenu>
                  <SidebarMenuItem>
                    <SidebarMenuButton
                      onClick={() => setContentView('discover')}
                      isActive={contentView === 'discover'}
                      tooltip="Discover public wikis"
                    >
                      <Compass size={16} />
                      <span>Discover</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                </SidebarMenu>
              </SidebarGroup>
            </SidebarContent>
            <SidebarFooter>
              <div className="flex items-center justify-between px-2 py-1">
                <div className="flex items-center gap-2">
                  <div className="flex h-6 w-6 items-center justify-center rounded-full bg-sidebar-accent text-sidebar-accent-foreground text-xs font-medium">
                    {auth?.name?.[0]?.toUpperCase() || 'U'}
                  </div>
                  <span className="text-xs text-sidebar-foreground/70">{auth?.name}</span>
                </div>
                <button onClick={handleLogout} className="text-sidebar-foreground/40 hover:text-sidebar-foreground/70 transition-colors" title="Sign out">
                  <LogOut size={14} />
                </button>
              </div>
            </SidebarFooter>
          </Sidebar>

          <SidebarInset>
            <div className="max-w-3xl px-16 py-10">
              {contentView === 'discover' ? (
                <DiscoverView />
              ) : (
                <HomeView
                  personalSpaces={personalSpaces}
                  teamspaces={teamspaces}
                  loading={loading}
                  onCreatePersonal={() => { setNewVisibility('private'); setShowCreate(true); }}
                  onCreateTeam={() => { setNewVisibility('public'); setShowCreate(true); }}
                />
              )}
            </div>
          </SidebarInset>
        </SidebarProvider>
      </TooltipProvider>

      {/* Create modal */}
      <Dialog open={showCreate} onOpenChange={setShowCreate}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>
              {newVisibility === 'private' ? 'Create Personal Space' : 'Create Teamspace'}
            </DialogTitle>
          </DialogHeader>
          <form onSubmit={handleCreate} className="space-y-4 mt-2">
            <div>
              <label className="text-sm text-[var(--color-text-secondary)] mb-1.5 block">Name</label>
              <Input
                value={newName}
                onChange={(e) => handleNameChange(e.target.value)}
                placeholder={newVisibility === 'private' ? 'e.g. My Notes, Research' : 'e.g. Engineering Wiki'}
                autoFocus
              />
            </div>
            <div>
              <label className="text-sm text-[var(--color-text-secondary)] mb-1.5 block">URL slug</label>
              <Input
                value={newSlug}
                onChange={(e) => setNewSlug(e.target.value)}
                placeholder="my-notes"
                className="font-mono"
              />
            </div>
            {newVisibility !== 'private' && (
              <div>
                <label className="text-sm text-[var(--color-text-secondary)] mb-1.5 block">Visibility</label>
                <div className="flex gap-2">
                  <button
                    type="button"
                    onClick={() => setNewVisibility('private')}
                    className={`flex items-center gap-1.5 px-3 py-1.5 rounded-md border text-sm transition-colors ${
                      newVisibility === 'private' ? 'border-[var(--color-text)] bg-[var(--color-bg-active)]' : 'border-[var(--color-border)] hover:bg-[var(--color-bg-hover)]'
                    }`}
                  >
                    <Lock size={14} /> Private
                  </button>
                  <button
                    type="button"
                    onClick={() => setNewVisibility('public')}
                    className={`flex items-center gap-1.5 px-3 py-1.5 rounded-md border text-sm transition-colors ${
                      newVisibility === 'public' ? 'border-[var(--color-text)] bg-[var(--color-bg-active)]' : 'border-[var(--color-border)] hover:bg-[var(--color-bg-hover)]'
                    }`}
                  >
                    <Globe size={14} /> Public
                  </button>
                </div>
              </div>
            )}
            <div className="flex justify-end gap-2 pt-2">
              <Button variant="outline" type="button" onClick={() => setShowCreate(false)}>Cancel</Button>
              <Button type="submit" disabled={creating || !newName.trim() || !newSlug.trim()}>
                {creating ? 'Creating...' : 'Create'}
              </Button>
            </div>
          </form>
        </DialogContent>
      </Dialog>
    </>
  );
}

function HomeView({
  personalSpaces, teamspaces, loading, onCreatePersonal, onCreateTeam,
}: {
  personalSpaces: Workspace[]; teamspaces: Workspace[]; loading: boolean;
  onCreatePersonal: () => void; onCreateTeam: () => void;
}) {
  return (
    <>
      <h1 className="text-4xl font-bold mb-8" style={{ fontFamily: 'var(--font-serif)' }}>Home</h1>

      {/* Quick actions */}
      {!loading && personalSpaces.length === 0 && teamspaces.length === 0 && (
        <div className="py-12 text-center mb-8">
          <p className="text-[var(--color-text-secondary)] text-sm mb-4">Get started by creating a space.</p>
          <div className="flex justify-center gap-3">
            <button onClick={onCreatePersonal} className="rounded-md bg-[var(--color-text)] text-white px-4 py-1.5 text-sm hover:opacity-90">
              Personal Space
            </button>
            <button onClick={onCreateTeam} className="rounded-md border border-[var(--color-text)] px-4 py-1.5 text-sm hover:bg-[var(--color-bg-hover)]">
              Teamspace
            </button>
          </div>
        </div>
      )}

      {/* Recent */}
      <section>
        <h2 className="text-xs font-medium text-[var(--color-text-tertiary)] uppercase tracking-wider mb-3">Recent</h2>
        <div className="py-8 text-center">
          <Clock size={20} className="mx-auto text-[var(--color-text-tertiary)] mb-2" />
          <p className="text-[var(--color-text-tertiary)] text-xs">Recently opened pages will appear here.</p>
        </div>
      </section>
    </>
  );
}

function DiscoverView() {
  const [spaces, setSpaces] = useState<Workspace[]>([]);
  const [loading, setLoading] = useState(true);
  const [joining, setJoining] = useState<string | null>(null);

  useEffect(() => {
    import('../api').then(({ listPublicWorkspaces }) =>
      listPublicWorkspaces().then(setSpaces).finally(() => setLoading(false))
    );
  }, []);

  const handleJoin = async (slug: string) => {
    setJoining(slug);
    try {
      const { joinWorkspace } = await import('../api');
      await joinWorkspace(slug);
      // Remove from list after joining
      setSpaces((prev) => prev.filter((w) => w.slug !== slug));
    } finally {
      setJoining(null);
    }
  };

  return (
    <>
      <h1 className="text-4xl font-bold mb-1" style={{ fontFamily: 'var(--font-serif)' }}>Discover</h1>
      <p className="text-[var(--color-text-tertiary)] text-sm mb-8">Public knowledge spaces you can browse and join.</p>

      {loading ? (
        <div className="py-8 text-center text-[var(--color-text-tertiary)] text-sm">Loading...</div>
      ) : spaces.length === 0 ? (
        <div className="py-12 text-center">
          <Compass size={24} className="mx-auto text-[var(--color-text-tertiary)] mb-3" />
          <p className="text-[var(--color-text-tertiary)] text-sm">No public spaces yet.</p>
        </div>
      ) : (
        <div className="space-y-3">
          {spaces.map((ws) => (
            <div key={ws.id} className="rounded-lg border border-[var(--color-border)] bg-white p-5 flex items-center justify-between">
              <div className="flex items-center gap-3">
                <div className="w-10 h-10 rounded-lg bg-[var(--color-bg-hover)] flex items-center justify-center text-lg font-medium text-[var(--color-text-secondary)]">
                  {ws.name[0]?.toUpperCase()}
                </div>
                <div>
                  <div className="text-sm font-medium text-[var(--color-text)]">{ws.name}</div>
                  <div className="text-xs text-[var(--color-text-tertiary)]">/{ws.slug}</div>
                </div>
              </div>
              <div className="flex gap-2">
                <Link to={`/w/${ws.slug}`} className="text-xs text-[var(--color-text-secondary)] hover:text-[var(--color-text)] transition-colors px-3 py-1.5 rounded-md border border-[var(--color-border)]">
                  Browse
                </Link>
                <button
                  onClick={() => handleJoin(ws.slug)}
                  disabled={joining === ws.slug}
                  className="text-xs text-white bg-[var(--color-text)] px-3 py-1.5 rounded-md hover:opacity-90 disabled:opacity-40 transition-opacity"
                >
                  {joining === ws.slug ? 'Joining...' : 'Join'}
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </>
  );
}
