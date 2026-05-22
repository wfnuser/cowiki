import { useEffect, useState } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import { Plus, FolderOpen, Clock, LogOut, Globe, Lock, UserPlus, Users } from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs';
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarInset,
} from '@/components/ui/sidebar';
import { TooltipProvider } from '@/components/ui/tooltip';
import { listWorkspaces, listPublicWorkspaces, createWorkspace, joinWorkspace, type Workspace } from '../api';
import { getStoredAuth, clearAuth } from '../auth';

type SpaceTab = 'my' | 'team' | 'public';

export function HomePage() {
  const [allMySpaces, setAllMySpaces] = useState<Workspace[]>([]);
  const [publicSpaces, setPublicSpaces] = useState<Workspace[]>([]);
  const [loading, setLoading] = useState(true);
  const [tab, setTab] = useState<SpaceTab>('my');
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
      const [mine, pub] = await Promise.all([listWorkspaces(), listPublicWorkspaces()]);
      setAllMySpaces(mine);
      const myIds = new Set(mine.map((w) => w.id));
      setPublicSpaces(pub.filter((w) => !myIds.has(w.id)));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => { load(); }, []);

  // My Space = only me (role=owner and no other members, or private)
  // Team Space = shared with others (role=owner/member and >1 member, or I joined someone else's)
  const mySpaces = allMySpaces.filter((w) => w.visibility === 'private' && w.role === 'owner');
  const teamSpaces = allMySpaces.filter((w) => w.visibility !== 'private' || w.role !== 'owner');

  const currentSpaces = tab === 'my' ? mySpaces : tab === 'team' ? teamSpaces : publicSpaces;

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

  const handleJoin = async (slug: string) => {
    await joinWorkspace(slug);
    load();
  };

  const handleLogout = () => {
    clearAuth();
    navigate('/login');
  };

  const iconForSpace = (ws: Workspace) => {
    if (ws.visibility === 'public') return <Globe size={16} />;
    if (tab === 'team') return <Users size={16} />;
    return <Lock size={16} />;
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
              {/* Tabs */}
              <div className="px-2 mt-2">
                <Tabs value={tab} onValueChange={(v) => setTab(v as SpaceTab)}>
                  <TabsList className="w-full h-8">
                    <TabsTrigger value="my" className="text-xs flex-1">Mine</TabsTrigger>
                    <TabsTrigger value="team" className="text-xs flex-1">Team</TabsTrigger>
                    <TabsTrigger value="public" className="text-xs flex-1">Public</TabsTrigger>
                  </TabsList>
                </Tabs>
              </div>
            </SidebarHeader>
            <SidebarContent>
              <SidebarGroup>
                <SidebarMenu>
                  {currentSpaces.map((ws) => (
                    <SidebarMenuItem key={ws.id}>
                      <SidebarMenuButton asChild tooltip={ws.name}>
                        <Link to={`/w/${ws.slug}`}>
                          {iconForSpace(ws)}
                          <span>{ws.name}</span>
                        </Link>
                      </SidebarMenuButton>
                    </SidebarMenuItem>
                  ))}
                  {!loading && currentSpaces.length === 0 && (
                    <div className="px-2 py-6 text-xs text-sidebar-foreground/40 text-center">
                      {tab === 'public' ? 'No public spaces yet' : 'No spaces yet'}
                    </div>
                  )}
                  {/* Create / Join button at bottom of list */}
                  {tab !== 'public' && (
                    <SidebarMenuItem>
                      <SidebarMenuButton onClick={() => setShowCreate(true)} tooltip="Create space">
                        <Plus size={16} />
                        <span className="text-sidebar-foreground/50">New space</span>
                      </SidebarMenuButton>
                    </SidebarMenuItem>
                  )}
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
              <h1 className="text-4xl font-bold mb-1" style={{ fontFamily: 'var(--font-serif)' }}>
                {tab === 'my' ? 'My Space' : tab === 'team' ? 'Team Spaces' : 'Public Spaces'}
              </h1>
              <p className="text-[var(--color-text-tertiary)] text-sm mb-8">
                {tab === 'my' && 'Your personal knowledge spaces.'}
                {tab === 'team' && 'Spaces shared with your team.'}
                {tab === 'public' && 'Open knowledge spaces anyone can browse.'}
              </p>

              {/* Space cards */}
              {!loading && currentSpaces.length > 0 && (
                <div className="grid grid-cols-2 gap-3 mb-10">
                  {currentSpaces.map((ws) => (
                    tab === 'public' && ws.role === 'viewer' ? (
                      <div key={ws.id} className="rounded-lg border border-[var(--color-border)] bg-white p-5 flex items-center justify-between">
                        <div className="flex items-center gap-3">
                          <div className="w-10 h-10 rounded-lg bg-[var(--color-bg-hover)] flex items-center justify-center text-lg font-medium text-[var(--color-text-secondary)]">
                            {ws.name[0]?.toUpperCase()}
                          </div>
                          <div>
                            <div className="text-sm font-medium text-[var(--color-text)]">{ws.name}</div>
                          </div>
                        </div>
                        <button
                          onClick={() => handleJoin(ws.slug)}
                          className="flex items-center gap-1 text-xs text-[var(--color-accent)] hover:text-[var(--color-accent-hover)] transition-colors"
                        >
                          <UserPlus size={14} /> Join
                        </button>
                      </div>
                    ) : (
                      <Link
                        key={ws.id}
                        to={`/w/${ws.slug}`}
                        className="rounded-lg border border-[var(--color-border)] bg-white p-5 hover:border-[var(--color-border-hover)] hover:shadow-sm transition-all group"
                      >
                        <div className="flex items-center gap-3">
                          <div className="w-10 h-10 rounded-lg bg-[var(--color-bg-hover)] flex items-center justify-center text-lg font-medium text-[var(--color-text-secondary)]">
                            {ws.name[0]?.toUpperCase()}
                          </div>
                          <div className="min-w-0">
                            <div className="flex items-center gap-1.5">
                              <span className="text-sm font-medium text-[var(--color-text)] truncate">{ws.name}</span>
                              {ws.visibility === 'public' && <Globe size={11} className="shrink-0 text-[var(--color-text-tertiary)]" />}
                            </div>
                            <div className="text-xs text-[var(--color-text-tertiary)]">{ws.role}</div>
                          </div>
                        </div>
                      </Link>
                    )
                  ))}
                  {tab !== 'public' && (
                    <button
                      onClick={() => setShowCreate(true)}
                      className="rounded-lg border border-dashed border-[var(--color-border)] p-5 hover:border-[var(--color-border-hover)] hover:bg-[var(--color-bg-secondary)] transition-all flex items-center justify-center gap-2 text-sm text-[var(--color-text-tertiary)]"
                    >
                      <Plus size={16} /> New Space
                    </button>
                  )}
                </div>
              )}

              {/* Empty state */}
              {!loading && currentSpaces.length === 0 && (
                <div className="py-16 text-center">
                  <FolderOpen size={32} className="mx-auto text-[var(--color-text-tertiary)] mb-3" />
                  <p className="text-[var(--color-text-secondary)] text-sm mb-4">
                    {tab === 'public' ? 'No public spaces yet.' : 'No spaces yet.'}
                  </p>
                  {tab !== 'public' && (
                    <button
                      onClick={() => setShowCreate(true)}
                      className="rounded-md bg-[var(--color-text)] text-white px-4 py-1.5 text-sm hover:opacity-90 transition-opacity"
                    >
                      Create your first space
                    </button>
                  )}
                </div>
              )}

              {/* Recent */}
              <section>
                <h2 className="text-xs font-medium text-[var(--color-text-tertiary)] uppercase tracking-wider mb-3">
                  Recent
                </h2>
                <div className="py-8 text-center">
                  <Clock size={20} className="mx-auto text-[var(--color-text-tertiary)] mb-2" />
                  <p className="text-[var(--color-text-tertiary)] text-xs">
                    Recently opened pages will appear here.
                  </p>
                </div>
              </section>
            </div>
          </SidebarInset>
        </SidebarProvider>
      </TooltipProvider>

      {/* Create modal — OUTSIDE SidebarProvider to fix z-index */}
      <Dialog open={showCreate} onOpenChange={setShowCreate}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Create Knowledge Space</DialogTitle>
          </DialogHeader>
          <form onSubmit={handleCreate} className="space-y-4 mt-2">
            <div>
              <label className="text-sm text-[var(--color-text-secondary)] mb-1.5 block">Name</label>
              <Input
                value={newName}
                onChange={(e) => handleNameChange(e.target.value)}
                placeholder="e.g. Engineering Wiki, Product Knowledge"
                autoFocus
              />
            </div>
            <div>
              <label className="text-sm text-[var(--color-text-secondary)] mb-1.5 block">URL slug</label>
              <Input
                value={newSlug}
                onChange={(e) => setNewSlug(e.target.value)}
                placeholder="engineering-wiki"
                className="font-mono"
              />
              <p className="text-xs text-[var(--color-text-tertiary)] mt-1">
                cowiki.app/w/{newSlug || 'your-slug'}
              </p>
            </div>
            <div>
              <label className="text-sm text-[var(--color-text-secondary)] mb-1.5 block">Visibility</label>
              <div className="flex gap-2">
                <button
                  type="button"
                  onClick={() => setNewVisibility('private')}
                  className={`flex items-center gap-1.5 px-3 py-1.5 rounded-md border text-sm transition-colors ${
                    newVisibility === 'private'
                      ? 'border-[var(--color-text)] bg-[var(--color-bg-active)]'
                      : 'border-[var(--color-border)] hover:bg-[var(--color-bg-hover)]'
                  }`}
                >
                  <Lock size={14} /> Private
                </button>
                <button
                  type="button"
                  onClick={() => setNewVisibility('public')}
                  className={`flex items-center gap-1.5 px-3 py-1.5 rounded-md border text-sm transition-colors ${
                    newVisibility === 'public'
                      ? 'border-[var(--color-text)] bg-[var(--color-bg-active)]'
                      : 'border-[var(--color-border)] hover:bg-[var(--color-bg-hover)]'
                  }`}
                >
                  <Globe size={14} /> Public
                </button>
              </div>
              <p className="text-xs text-[var(--color-text-tertiary)] mt-1">
                {newVisibility === 'private'
                  ? 'Only you can see this. Share to make it a team space.'
                  : 'Anyone can browse. Members can contribute.'}
              </p>
            </div>
            <div className="flex justify-end gap-2 pt-2">
              <Button variant="outline" type="button" onClick={() => setShowCreate(false)}>
                Cancel
              </Button>
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
