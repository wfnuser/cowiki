import { useEffect, useState } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import { Plus, Clock, LogOut, Compass, Users, FileText, Folder, ChevronRight } from 'lucide-react';
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
import { listWorkspaces, listPages, createWorkspace, type Workspace, type PageMeta } from '../api';
import { getStoredAuth, clearAuth } from '../auth';

type ContentView = 'home' | 'discover';

/** Renders a page or folder in the sidebar tree */
function PageTreeItem({ page, wsSlug, authId }: { page: PageMeta; wsSlug: string; authId?: string }) {
  const [open, setOpen] = useState(false);

  if (page.kind === 'folder') {
    return (
      <>
        <SidebarMenuItem>
          <SidebarMenuButton onClick={() => setOpen(!open)} tooltip={page.title}>
            <ChevronRight size={14} className={`transition-transform ${open ? 'rotate-90' : ''}`} />
            <Folder size={16} />
            <span>{page.title || page.slug}</span>
          </SidebarMenuButton>
        </SidebarMenuItem>
        {open && page.children?.map((child) => (
          <SidebarMenuItem key={child.slug} className="pl-4">
            <SidebarMenuButton asChild tooltip={child.title}>
              <Link to={`/w/${wsSlug}/page/${child.slug}?branch=user/${authId}`}>
                <FileText size={16} />
                <span>{child.title || child.slug}</span>
              </Link>
            </SidebarMenuButton>
          </SidebarMenuItem>
        ))}
      </>
    );
  }

  return (
    <SidebarMenuItem>
      <SidebarMenuButton asChild tooltip={page.title}>
        <Link to={`/w/${wsSlug}/page/${page.slug}?branch=user/${authId}`}>
          <FileText size={16} />
          <span>{page.title || page.slug}</span>
        </Link>
      </SidebarMenuButton>
    </SidebarMenuItem>
  );
}

export function HomePage() {
  const [mySpaces, setMySpaces] = useState<Workspace[]>([]);
  const [personalPages, setPersonalPages] = useState<PageMeta[]>([]);
  const [loading, setLoading] = useState(true);
  const [contentView, setContentView] = useState<ContentView>('home');
  const [showCreate, setShowCreate] = useState(false);
  const [showNewPage, setShowNewPage] = useState(false);
  const [showNewFolder, setShowNewFolder] = useState(false);
  const [newPageTitle, setNewPageTitle] = useState('');
  const [newFolderName, setNewFolderName] = useState('');
  const [newName, setNewName] = useState('');
  const [newSlug, setNewSlug] = useState('');
  const [creating, setCreating] = useState(false);
  const navigate = useNavigate();
  const auth = getStoredAuth();

  const load = async () => {
    setLoading(true);
    try {
      const mine = await listWorkspaces();
      setMySpaces(mine);
      // Load personal space pages (first private space, or user branch)
      const personalWs = mine.find((w) => w.visibility === 'private' && w.role === 'owner');
      if (personalWs) {
        const pages = await listPages(`user/${auth?.id || 'default'}`);
        setPersonalPages(pages);
      }
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => { load(); }, []);

  const teamspaces = mySpaces.filter((w) => !(w.visibility === 'private' && w.role === 'owner'));
  const personalWs = mySpaces.find((w) => w.visibility === 'private' && w.role === 'owner');

  const handleNameChange = (name: string) => {
    setNewName(name);
    setNewSlug(name.toLowerCase().replace(/[^a-z0-9\s-]/g, '').replace(/\s+/g, '-').replace(/-+/g, '-').trim());
  };

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newName.trim() || !newSlug.trim()) return;
    setCreating(true);
    try {
      const ws = await createWorkspace(newName.trim(), newSlug.trim(), 'public');
      setShowCreate(false);
      setNewName('');
      setNewSlug('');
      navigate(`/w/${ws.slug}`);
    } finally {
      setCreating(false);
    }
  };

  const handleNewPage = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newPageTitle.trim() || !personalWs) return;
    const slug = newPageTitle.toLowerCase().replace(/[^a-z0-9\s-]/g, '').replace(/\s+/g, '-').trim();
    const body = `---\ntitle: "${newPageTitle.trim()}"\nsummary: ""\nkind: concept\n---\n\n`;
    const { writePage } = await import('../api');
    await writePage(slug, body, `user/${auth?.id}`);
    setShowNewPage(false);
    setNewPageTitle('');
    load();
    // Navigate to the new page
    navigate(`/w/${personalWs.slug}/page/${slug}?branch=user/${auth?.id}`);
  };

  const handleNewFolder = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newFolderName.trim()) return;
    const { createFolder } = await import('../api');
    await createFolder(newFolderName.trim(), `user/${auth?.id}`);
    setShowNewFolder(false);
    setNewFolderName('');
    load();
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
              {/* Personal Space — show pages/folders as tree */}
              <SidebarGroup>
                <SidebarGroupLabel>Personal Space</SidebarGroupLabel>
                <SidebarMenu>
                  {personalWs && personalPages.map((p) => (
                    <PageTreeItem
                      key={p.slug}
                      page={p}
                      wsSlug={personalWs.slug}
                      authId={auth?.id}
                    />
                  ))}
                  <SidebarMenuItem>
                    <SidebarMenuButton onClick={() => setShowNewPage(true)} tooltip="Add new page">
                      <Plus size={16} />
                      <span className="text-sidebar-foreground/50">New page</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                  <SidebarMenuItem>
                    <SidebarMenuButton onClick={() => setShowNewFolder(true)} tooltip="Add new folder">
                      <Plus size={16} />
                      <span className="text-sidebar-foreground/50">New folder</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                </SidebarMenu>
              </SidebarGroup>

              {/* Team Spaces — show space list */}
              <SidebarGroup>
                <SidebarGroupLabel>Team Spaces</SidebarGroupLabel>
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
                  <SidebarMenuItem>
                    <SidebarMenuButton
                      onClick={() => setShowCreate(true)}
                      tooltip="Create a new team space"
                    >
                      <Plus size={16} />
                      <span className="text-sidebar-foreground/50">Add new team space</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
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
                <HomeView pages={personalPages} personalWs={personalWs} authId={auth?.id} />
              )}
            </div>
          </SidebarInset>
        </SidebarProvider>
      </TooltipProvider>

      {/* Create team space modal */}
      <Dialog open={showCreate} onOpenChange={setShowCreate}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Create Team Space</DialogTitle>
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
            </div>
            <div className="flex justify-end gap-2 pt-2">
              <Button variant="outline" type="button" onClick={() => setShowCreate(false)}>Cancel</Button>
              <Button type="submit" disabled={creating || !newName.trim() || !newSlug.trim()}>
                {creating ? 'Creating...' : 'Create'}
              </Button>
            </div>
          </form>
        </DialogContent>
      </Dialog>

      {/* New folder modal */}
      <Dialog open={showNewFolder} onOpenChange={setShowNewFolder}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>New Folder</DialogTitle>
          </DialogHeader>
          <form onSubmit={handleNewFolder} className="space-y-4 mt-2">
            <div>
              <label className="text-sm text-[var(--color-text-secondary)] mb-1.5 block">Name</label>
              <Input
                value={newFolderName}
                onChange={(e) => setNewFolderName(e.target.value)}
                placeholder="e.g. Research, Projects, Notes"
                autoFocus
              />
            </div>
            <div className="flex justify-end gap-2 pt-2">
              <Button variant="outline" type="button" onClick={() => setShowNewFolder(false)}>Cancel</Button>
              <Button type="submit" disabled={!newFolderName.trim()}>Create</Button>
            </div>
          </form>
        </DialogContent>
      </Dialog>

      {/* New page modal */}
      <Dialog open={showNewPage} onOpenChange={setShowNewPage}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>New Page</DialogTitle>
          </DialogHeader>
          <form onSubmit={handleNewPage} className="space-y-4 mt-2">
            <div>
              <label className="text-sm text-[var(--color-text-secondary)] mb-1.5 block">Title</label>
              <Input
                value={newPageTitle}
                onChange={(e) => setNewPageTitle(e.target.value)}
                placeholder="e.g. Meeting Notes, Research Ideas"
                autoFocus
              />
            </div>
            <div className="flex justify-end gap-2 pt-2">
              <Button variant="outline" type="button" onClick={() => setShowNewPage(false)}>Cancel</Button>
              <Button type="submit" disabled={!newPageTitle.trim()}>Create</Button>
            </div>
          </form>
        </DialogContent>
      </Dialog>
    </>
  );
}

function HomeView({ pages, personalWs, authId }: { pages: PageMeta[]; personalWs?: Workspace; authId?: string }) {
  return (
    <>
      <h1 className="text-4xl font-bold mb-6" style={{ fontFamily: 'var(--font-serif)' }}>Home</h1>

      {/* Pages list */}
      {pages.length > 0 ? (
        <section>
          <h2 className="text-xs font-medium text-[var(--color-text-tertiary)] uppercase tracking-wider mb-3">
            My Pages
          </h2>
          <div>
            {pages.map((p) => (
              <Link
                key={p.slug}
                to={personalWs ? `/w/${personalWs.slug}/page/${p.slug}?branch=user/${authId}` : '#'}
                className="flex items-center gap-2.5 px-2 py-2 -mx-2 rounded-md hover:bg-[var(--color-bg-hover)] transition-colors group"
              >
                <FileText size={16} className="shrink-0 text-[var(--color-text-tertiary)]" strokeWidth={1.5} />
                <div className="min-w-0">
                  <div className="text-sm text-[var(--color-text)]">{p.title || p.slug}</div>
                  {p.summary && (
                    <div className="text-xs text-[var(--color-text-tertiary)] truncate">{p.summary}</div>
                  )}
                </div>
              </Link>
            ))}
          </div>
        </section>
      ) : (
        <section>
          <div className="py-8 text-center">
            <Clock size={20} className="mx-auto text-[var(--color-text-tertiary)] mb-2" />
            <p className="text-[var(--color-text-tertiary)] text-xs">Your pages will appear here.</p>
          </div>
        </section>
      )}
    </>
  );
}

function DiscoverView() {
  const [spaces, setSpaces] = useState<{ id: string; name: string; slug: string; visibility: string; role: string }[]>([]);
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
