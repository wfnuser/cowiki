import { useState, useEffect, useCallback } from 'react';
import { useNavigate, useLocation } from 'react-router-dom';
import {
  Plus, LogOut, Compass, Library, FileText, Folder, Search,
  ChevronRight, FolderPlus, Upload, Wand2, ArrowUpRight, MoreHorizontal, RefreshCw, Pencil,
} from 'lucide-react';
import {
  Sidebar, SidebarContent, SidebarFooter, SidebarGroup, SidebarGroupLabel,
  SidebarHeader, SidebarMenu, SidebarMenuButton, SidebarMenuItem,
  SidebarProvider, SidebarInset,
} from '@/components/ui/sidebar';
import {
  DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import {
  Dialog, DialogContent, DialogHeader, DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { TooltipProvider } from '@/components/ui/tooltip';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import {
  listWorkspaces, listPages, getPage, createWorkspace, writePage, createFolder,
  ingest, compile, submit,
  type Workspace, type PageMeta, type PageFull,
} from '../api';
import { IngestForm } from '../components/IngestForm';
import { getStoredAuth, clearAuth } from '../auth';

interface ActivePage {
  workspace: Workspace;
  slug: string;
}

export function MainLayout() {
  const [auth, setAuth] = useState(getStoredAuth);
  const navigate = useNavigate();
  const location = useLocation();

  // Re-read auth when localStorage might have changed (e.g. after OAuth redirect)
  useEffect(() => {
    const checkAuth = () => setAuth(getStoredAuth());
    checkAuth();
    // Poll briefly in case OAuthInterceptor writes after mount
    const timer = setTimeout(checkAuth, 100);
    return () => clearTimeout(timer);
  }, [location.pathname]);

  // Data
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [expandedSpaces, setExpandedSpaces] = useState<Set<string>>(new Set());
  const [spacePages, setSpacePages] = useState<Record<string, PageMeta[]>>({});
  const [activePage, setActivePage] = useState<ActivePage | null>(null);
  const [pageContent, setPageContent] = useState<PageFull | null>(null);
  const [loading, setLoading] = useState(true);

  // Modals
  const [showCreate, setShowCreate] = useState(false);
  const [showNewPage, setShowNewPage] = useState<Workspace | null>(null);
  const [showNewFolder, setShowNewFolder] = useState<Workspace | null>(null);
  const [newName, setNewName] = useState('');
  const [newSlug, setNewSlug] = useState('');
  const [creating, setCreating] = useState(false);
  const [showIngest, setShowIngest] = useState(false);
  const [compiling, setCompiling] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [showRename, setShowRename] = useState<Workspace | null>(null);
  const [renameValue, setRenameValue] = useState('');
  const [message, setMessage] = useState<{ text: string; type: 'success' | 'error' } | null>(null);
  const [searchQuery, setSearchQuery] = useState('');

  const userBranch = `user/${auth?.id}`;

  // Load workspaces + restore state from URL
  const loadWorkspaces = useCallback(async () => {
    setLoading(true);
    const ws = await listWorkspaces();
    setWorkspaces(ws);

    // Auto-expand personal space and load its pages
    const personal = ws.find((w) => w.visibility === 'private' && w.role === 'owner');
    if (personal) {
      setExpandedSpaces((prev) => new Set([...prev, personal.id]));
      await loadSpacePages(personal);
    }

    // Restore page from URL: /:owner/:wsSlug/:pageSlug
    const pathParts = location.pathname.split('/').filter(Boolean);
    if (pathParts.length >= 3) {
      const [, wsSlug, ...pageParts] = pathParts;
      const pageSlug = pageParts.join('/');
      const targetWs = ws.find((w) => w.slug === wsSlug);
      if (targetWs) {
        // Expand the target workspace
        setExpandedSpaces((prev) => new Set([...prev, targetWs.id]));
        if (!spacePages[targetWs.id]) {
          await loadSpacePages(targetWs);
        }
        // Load the page
        const branch = targetWs.visibility === 'private' ? userBranch : 'main';
        try {
          const page = await getPage(pageSlug, branch, targetWs.slug);
          setActivePage({ workspace: targetWs, slug: pageSlug });
          setPageContent(page);
        } catch {
          // Page not found, just show home
        }
      }
    } else if (personal) {
      // No page in URL — default to first page in personal space
      const personalPages = spacePages[personal.id] || await listPages(userBranch, personal.slug);
      const firstPage = personalPages.find((p) => p.kind === 'page');
      if (firstPage) {
        selectPage(personal, firstPage.slug);
      }
    }

    setLoading(false);
  }, [auth?.id]);

  useEffect(() => {
    if (auth) loadWorkspaces();
  }, [auth?.id, loadWorkspaces]);

  // Load pages for a space
  const loadSpacePages = async (ws: Workspace) => {
    const branch = ws.visibility === 'private' ? userBranch : 'main';
    const pages = await listPages(branch, ws.slug);
    setSpacePages((prev) => ({ ...prev, [ws.id]: pages }));
  };

  // Toggle space expansion
  const toggleSpace = (ws: Workspace) => {
    setExpandedSpaces((prev) => {
      const next = new Set(prev);
      if (next.has(ws.id)) {
        next.delete(ws.id);
      } else {
        next.add(ws.id);
        if (!spacePages[ws.id]) loadSpacePages(ws);
      }
      return next;
    });
  };

  // Select a page
  const selectPage = async (ws: Workspace, slug: string) => {
    setActivePage({ workspace: ws, slug });
    const branch = ws.visibility === 'private' ? userBranch : 'main';
    const page = await getPage(slug, branch, ws.slug);
    setPageContent(page);
    // Update URL
    const owner = auth?.name || 'user';
    navigate(`/${owner}/${ws.slug}/${slug}`, { replace: true });
  };

  // Create workspace
  const handleCreateWorkspace = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newName.trim() || !newSlug.trim()) return;
    setCreating(true);
    try {
      await createWorkspace(newName.trim(), newSlug.trim(), 'public');
      setShowCreate(false);
      setNewName('');
      setNewSlug('');
      loadWorkspaces();
    } finally {
      setCreating(false);
    }
  };

  // Create page in a workspace
  const handleCreatePage = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newName.trim() || !showNewPage) return;
    const ws = showNewPage; // save ref before closing modal
    const slug = newName.toLowerCase().replace(/[^a-z0-9\s-]/g, '').replace(/\s+/g, '-').trim();
    const body = `---\ntitle: "${newName.trim()}"\nsummary: ""\nkind: concept\n---\n\n`;
    await writePage(slug, body, userBranch, ws.slug);
    setShowNewPage(null);
    setNewName('');
    await loadSpacePages(ws);
    selectPage(ws, slug);
  };

  // Create folder in a workspace
  const handleCreateFolder = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newName.trim() || !showNewFolder) return;
    const ws = showNewFolder; // save ref before closing modal
    await createFolder(newName.trim(), userBranch, undefined, ws.slug);
    setShowNewFolder(null);
    setNewName('');
    await loadSpacePages(ws);
  };

  // Compile pages in the active workspace
  const handleCompile = async () => {
    if (!activePage) return;
    setCompiling(true);
    setMessage(null);
    try {
      const res = await compile(userBranch, activePage.workspace.slug);
      const count = res.pages?.length || 0;
      const skipped = res.skipped || 0;
      setMessage({ text: `Compiled ${count} page(s)${skipped > 0 ? `, ${skipped} skipped` : ''}`, type: 'success' });
      loadSpacePages(activePage.workspace);
    } catch {
      setMessage({ text: 'Compilation failed', type: 'error' });
    } finally {
      setCompiling(false);
    }
  };

  // Submit pages for review (or direct commit for personal)
  const handleSubmit = async () => {
    if (!activePage) return;
    setSubmitting(true);
    setMessage(null);
    try {
      const pages = spacePages[activePage.workspace.id] || [];
      if (pages.length === 0) {
        setMessage({ text: 'No pages to submit', type: 'error' });
        return;
      }
      const slugs = pages.map((p) => p.slug);
      const isPersonal = activePage.workspace.visibility === 'private';
      await submit(userBranch, slugs, isPersonal);
      setMessage({ text: isPersonal ? 'Committed.' : 'Submitted for review.', type: 'success' });
    } catch {
      setMessage({ text: 'Submit failed', type: 'error' });
    } finally {
      setSubmitting(false);
    }
  };

  // Handle ingest completion
  const handleIngestDone = () => {
    setShowIngest(false);
    if (activePage) loadSpacePages(activePage.workspace);
  };

  const handleRename = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!renameValue.trim() || !showRename) return;
    const { renameWorkspace } = await import('../api');
    await renameWorkspace(showRename.slug, renameValue.trim());
    setShowRename(null);
    setRenameValue('');
    loadWorkspaces();
  };

  const handleLogout = () => {
    clearAuth();
    navigate('/login');
  };

  const handleNameChange = (name: string) => {
    setNewName(name);
    setNewSlug(name.toLowerCase().replace(/[^a-z0-9\s-]/g, '').replace(/\s+/g, '-').trim());
  };

  const personalSpaces = workspaces.filter((w) => w.visibility === 'private' && w.role === 'owner');
  const teamSpaces = workspaces.filter((w) => !(w.visibility === 'private' && w.role === 'owner'));

  // Client-side search: filter pages across all loaded spaces
  const searchResults = searchQuery.trim() ? (() => {
    const q = searchQuery.toLowerCase();
    const results: { workspace: Workspace; page: PageMeta }[] = [];
    for (const ws of workspaces) {
      const pages = spacePages[ws.id] || [];
      for (const p of pages) {
        if (p.title.toLowerCase().includes(q) || p.summary.toLowerCase().includes(q) || p.slug.toLowerCase().includes(q)) {
          results.push({ workspace: ws, page: p });
        }
        // Also search folder children
        if (p.children) {
          for (const child of p.children) {
            if (child.title.toLowerCase().includes(q) || child.summary.toLowerCase().includes(q) || child.slug.toLowerCase().includes(q)) {
              results.push({ workspace: ws, page: child });
            }
          }
        }
      }
    }
    return results;
  })() : null;

  // Strip frontmatter from page body
  const renderBody = (body: string) => {
    if (body.startsWith('---')) {
      const parts = body.split('---');
      if (parts.length >= 3) return parts.slice(2).join('---').trim();
    }
    return body;
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
              <SidebarMenu className="mt-1 mb-0.5">
                <SidebarMenuItem>
                  <div className="flex items-center gap-2 px-2 py-1.5 rounded-md text-sm text-sidebar-foreground/70 bg-white/80 border border-sidebar-border/60">
                    <Search size={16} className="shrink-0 text-sidebar-foreground/40" />
                    <input
                      type="text"
                      value={searchQuery}
                      onChange={(e) => setSearchQuery(e.target.value)}
                      placeholder="Search..."
                      className="bg-transparent outline-none w-full text-sm placeholder:text-sidebar-foreground/40"
                    />
                  </div>
                </SidebarMenuItem>
              </SidebarMenu>
            </SidebarHeader>
            <SidebarContent>
              {/* Personal Space */}
              {personalSpaces.map((ws) => (
                <SpaceSection
                  key={ws.id}
                  workspace={ws}
                  label="Personal Space"
                  pages={spacePages[ws.id] || []}
                  expanded={expandedSpaces.has(ws.id)}
                  activePage={activePage}
                  onToggle={() => toggleSpace(ws)}
                  onSelectPage={(slug) => selectPage(ws, slug)}
                  onNewPage={() => { setShowNewPage(ws); setNewName(''); }}
                  onNewFolder={() => { setShowNewFolder(ws); setNewName(''); }}
                />
              ))}

              {/* Team Spaces */}
              <SidebarGroup>
                <SidebarGroupLabel>Team Spaces</SidebarGroupLabel>
                <SidebarMenu>
                  {teamSpaces.map((ws) => (
                    <SpaceTreeItem
                      key={ws.id}
                      workspace={ws}
                      pages={spacePages[ws.id] || []}
                      expanded={expandedSpaces.has(ws.id)}
                      activePage={activePage}
                      onToggle={() => toggleSpace(ws)}
                      onSelectPage={(slug) => selectPage(ws, slug)}
                      onNewPage={() => { setShowNewPage(ws); setNewName(''); }}
                      onNewFolder={() => { setShowNewFolder(ws); setNewName(''); }}
                      onRename={() => { setShowRename(ws); setRenameValue(ws.name); }}
                    />
                  ))}
                  <SidebarMenuItem>
                    <SidebarMenuButton onClick={() => { setShowCreate(true); setNewName(''); setNewSlug(''); }}>
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
                    <SidebarMenuButton onClick={() => { setActivePage(null); setPageContent(null); navigate('/discover'); }} tooltip="Discover public wikis">
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
            {/* Top bar with breadcrumb + actions */}
            {activePage && pageContent && (
              <div className="sticky top-0 z-10 bg-[var(--color-bg)] border-b border-[var(--color-border)] px-6 py-2 flex items-center justify-between">
                <div className="flex items-center gap-1.5 text-sm text-[var(--color-text-secondary)]">
                  <span>{auth?.name}</span>
                  <span className="text-[var(--color-text-tertiary)]">/</span>
                  <span>{activePage.workspace.name}</span>
                  <span className="text-[var(--color-text-tertiary)]">/</span>
                  <span className="text-[var(--color-text)]">{pageContent.title}</span>
                </div>
                <div className="flex items-center gap-1">
                  <button
                    onClick={() => setShowIngest(true)}
                    className="flex items-center gap-1.5 px-2.5 py-1 rounded text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)] transition-colors"
                  >
                    <Upload size={13} /> Add Source
                  </button>
                  <button
                    onClick={handleCompile}
                    disabled={compiling}
                    className="flex items-center gap-1.5 px-2.5 py-1 rounded text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)] transition-colors disabled:opacity-40"
                  >
                    {compiling ? <RefreshCw size={13} className="animate-spin" /> : <Wand2 size={13} />}
                    {compiling ? 'Compiling...' : 'Compile'}
                  </button>
                  <DropdownMenu>
                    <DropdownMenuTrigger asChild>
                      <button className="p-1 rounded text-[var(--color-text-tertiary)] hover:bg-[var(--color-bg-hover)] transition-colors outline-none">
                        <MoreHorizontal size={16} />
                      </button>
                    </DropdownMenuTrigger>
                    <DropdownMenuContent align="end" className="w-40">
                      <DropdownMenuItem onClick={handleSubmit} disabled={submitting}>
                        <ArrowUpRight size={14} className="mr-2" />
                        {submitting ? 'Submitting...' : 'Submit'}
                      </DropdownMenuItem>
                    </DropdownMenuContent>
                  </DropdownMenu>
                </div>
              </div>
            )}

            {/* Message */}
            {message && (
              <div className={`mx-6 mt-2 rounded px-3 py-2 text-xs ${
                message.type === 'success' ? 'bg-green-50 text-green-700' : 'bg-red-50 text-red-700'
              }`}>
                {message.text}
              </div>
            )}

            {/* Ingest panel */}
            {showIngest && activePage && (
              <div className="mx-6 mt-2 rounded-lg border border-[var(--color-border)] p-4 bg-[var(--color-bg-secondary)]">
                <div className="text-xs text-[var(--color-text-tertiary)] mb-2">
                  Add source to {activePage.workspace.name}
                </div>
                <IngestForm branch={userBranch} onDone={handleIngestDone} workspaceSlug={activePage.workspace.slug} />
              </div>
            )}

            {/* Content */}
            <div className="max-w-3xl px-16 py-10">
              {searchResults ? (
                <div>
                  <h1 className="text-2xl font-bold mb-4" style={{ fontFamily: 'var(--font-serif)' }}>
                    Search: "{searchQuery}"
                  </h1>
                  {searchResults.length === 0 ? (
                    <p className="text-[var(--color-text-tertiary)] text-sm">No results found.</p>
                  ) : (
                    <div>
                      {searchResults.map((r) => (
                        <button
                          key={`${r.workspace.id}-${r.page.slug}`}
                          onClick={() => { setSearchQuery(''); selectPage(r.workspace, r.page.slug); }}
                          className="w-full text-left flex items-center gap-2.5 px-2 py-2 -mx-2 rounded-md hover:bg-[var(--color-bg-hover)] transition-colors"
                        >
                          <FileText size={16} className="shrink-0 text-[var(--color-text-tertiary)]" />
                          <div className="min-w-0">
                            <div className="text-sm text-[var(--color-text)]">{r.page.title || r.page.slug}</div>
                            <div className="text-xs text-[var(--color-text-tertiary)]">
                              {r.workspace.name}
                              {r.page.summary && ` · ${r.page.summary}`}
                            </div>
                          </div>
                        </button>
                      ))}
                    </div>
                  )}
                </div>
              ) : pageContent ? (
                <article>
                  <h1 className="text-4xl font-bold mb-2 leading-tight" style={{ fontFamily: 'var(--font-serif)' }}>
                    {pageContent.title}
                  </h1>
                  {pageContent.summary && (
                    <p className="text-[var(--color-text-secondary)] mb-8">{pageContent.summary}</p>
                  )}
                  <div className="prose">
                    <ReactMarkdown remarkPlugins={[remarkGfm]}>{renderBody(pageContent.body)}</ReactMarkdown>
                  </div>
                </article>
              ) : location.pathname === '/discover' ? (
                <DiscoverView />
              ) : (
                <div>
                  <h1 className="text-4xl font-bold mb-6" style={{ fontFamily: 'var(--font-serif)' }}>Home</h1>
                  <p className="text-[var(--color-text-tertiary)] text-sm">
                    Select a page from the sidebar to get started.
                  </p>
                </div>
              )}
            </div>
          </SidebarInset>
        </SidebarProvider>
      </TooltipProvider>

      {/* Create team space modal */}
      <Dialog open={showCreate} onOpenChange={setShowCreate}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader><DialogTitle>Create Team Space</DialogTitle></DialogHeader>
          <form onSubmit={handleCreateWorkspace} className="space-y-4 mt-2">
            <div>
              <label className="text-sm text-[var(--color-text-secondary)] mb-1.5 block">Name</label>
              <Input value={newName} onChange={(e) => handleNameChange(e.target.value)} placeholder="e.g. Engineering Wiki" autoFocus />
            </div>
            <div>
              <label className="text-sm text-[var(--color-text-secondary)] mb-1.5 block">URL slug</label>
              <Input value={newSlug} onChange={(e) => setNewSlug(e.target.value)} placeholder="engineering-wiki" className="font-mono" />
            </div>
            <div className="flex justify-end gap-2 pt-2">
              <Button variant="outline" type="button" onClick={() => setShowCreate(false)}>Cancel</Button>
              <Button type="submit" disabled={creating || !newName.trim() || !newSlug.trim()}>{creating ? 'Creating...' : 'Create'}</Button>
            </div>
          </form>
        </DialogContent>
      </Dialog>

      {/* New page modal */}
      <Dialog open={!!showNewPage} onOpenChange={(open) => !open && setShowNewPage(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader><DialogTitle>New Page</DialogTitle></DialogHeader>
          <form onSubmit={handleCreatePage} className="space-y-4 mt-2">
            <div>
              <label className="text-sm text-[var(--color-text-secondary)] mb-1.5 block">Title</label>
              <Input value={newName} onChange={(e) => setNewName(e.target.value)} placeholder="e.g. Meeting Notes" autoFocus />
            </div>
            <div className="flex justify-end gap-2 pt-2">
              <Button variant="outline" type="button" onClick={() => setShowNewPage(null)}>Cancel</Button>
              <Button type="submit" disabled={!newName.trim()}>Create</Button>
            </div>
          </form>
        </DialogContent>
      </Dialog>

      {/* Rename workspace modal */}
      <Dialog open={!!showRename} onOpenChange={(open) => !open && setShowRename(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader><DialogTitle>Rename Space</DialogTitle></DialogHeader>
          <form onSubmit={handleRename} className="space-y-4 mt-2">
            <div>
              <label className="text-sm text-[var(--color-text-secondary)] mb-1.5 block">Name</label>
              <Input value={renameValue} onChange={(e) => setRenameValue(e.target.value)} autoFocus />
            </div>
            <div className="flex justify-end gap-2 pt-2">
              <Button variant="outline" type="button" onClick={() => setShowRename(null)}>Cancel</Button>
              <Button type="submit" disabled={!renameValue.trim()}>Rename</Button>
            </div>
          </form>
        </DialogContent>
      </Dialog>

      {/* New folder modal */}
      <Dialog open={!!showNewFolder} onOpenChange={(open) => !open && setShowNewFolder(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader><DialogTitle>New Folder</DialogTitle></DialogHeader>
          <form onSubmit={handleCreateFolder} className="space-y-4 mt-2">
            <div>
              <label className="text-sm text-[var(--color-text-secondary)] mb-1.5 block">Name</label>
              <Input value={newName} onChange={(e) => setNewName(e.target.value)} placeholder="e.g. Research, Projects" autoFocus />
            </div>
            <div className="flex justify-end gap-2 pt-2">
              <Button variant="outline" type="button" onClick={() => setShowNewFolder(null)}>Cancel</Button>
              <Button type="submit" disabled={!newName.trim()}>Create</Button>
            </div>
          </form>
        </DialogContent>
      </Dialog>
    </>
  );
}

// ── Sidebar Components ──

function SpaceSection({
  workspace, label, pages, expanded, activePage, onToggle, onSelectPage, onNewPage, onNewFolder,
}: {
  workspace: Workspace; label: string; pages: PageMeta[];
  expanded: boolean; activePage: ActivePage | null;
  onToggle: () => void; onSelectPage: (slug: string) => void;
  onNewPage: () => void; onNewFolder: () => void;
}) {
  return (
    <SidebarGroup>
      <SidebarGroupLabel className="group/label">
        <span className="cursor-pointer" onClick={onToggle}>{label}</span>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button className="ml-auto opacity-0 group-hover/label:opacity-100 text-sidebar-foreground/40 hover:text-sidebar-foreground/70 transition-all outline-none focus:outline-none ring-0 focus:ring-0">
              <Plus size={14} />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="w-40">
            <DropdownMenuItem onClick={onNewPage}><FileText size={14} className="mr-2" /> New Page</DropdownMenuItem>
            <DropdownMenuItem onClick={onNewFolder}><FolderPlus size={14} className="mr-2" /> New Folder</DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </SidebarGroupLabel>
      {expanded && (
        <SidebarMenu>
          {pages.map((p) => (
            <PageItem
              key={p.slug}
              page={p}
              isActive={activePage?.workspace.id === workspace.id && activePage?.slug === p.slug}
              onSelect={() => onSelectPage(p.slug)} onSelectChild={(slug) => onSelectPage(slug)}
            />
          ))}
        </SidebarMenu>
      )}
    </SidebarGroup>
  );
}

function SpaceTreeItem({
  workspace, pages, expanded, activePage, onToggle, onSelectPage, onNewPage, onNewFolder, onRename,
}: {
  workspace: Workspace; pages: PageMeta[];
  expanded: boolean; activePage: ActivePage | null;
  onToggle: () => void; onSelectPage: (slug: string) => void;
  onNewPage: () => void; onNewFolder: () => void; onRename: () => void;
}) {
  return (
    <>
      <SidebarMenuItem className="group/space relative">
        <SidebarMenuButton onClick={onToggle} tooltip={workspace.name}>
          <ChevronRight size={14} className={`transition-transform ${expanded ? 'rotate-90' : ''}`} />
          <Library size={16} />
          <span>{workspace.name}</span>
        </SidebarMenuButton>
        {/* Hover actions: "+" and "..." */}
        <div className="absolute right-1 top-1/2 -translate-y-1/2 flex items-center gap-0.5 opacity-0 group-hover/space:opacity-100 transition-all">
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button className="p-0.5 rounded text-sidebar-foreground/40 hover:text-sidebar-foreground/70 outline-none focus:outline-none ring-0 focus:ring-0">
                <MoreHorizontal size={14} />
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-36">
              <DropdownMenuItem onClick={onRename}><Pencil size={14} className="mr-2" /> Rename</DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button className="p-0.5 rounded text-sidebar-foreground/40 hover:text-sidebar-foreground/70 outline-none focus:outline-none ring-0 focus:ring-0">
                <Plus size={14} />
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-40">
              <DropdownMenuItem onClick={onNewPage}><FileText size={14} className="mr-2" /> New Page</DropdownMenuItem>
              <DropdownMenuItem onClick={onNewFolder}><FolderPlus size={14} className="mr-2" /> New Folder</DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </SidebarMenuItem>
      {expanded && pages.map((p) => (
        <PageItem
          key={p.slug}
          page={p}
          isActive={activePage?.workspace.id === workspace.id && activePage?.slug === p.slug}
          onSelect={() => onSelectPage(p.slug)} onSelectChild={(slug) => onSelectPage(slug)}
          indent
        />
      ))}
    </>
  );
}

function PageItem({ page, isActive, onSelect, onSelectChild, indent }: {
  page: PageMeta; isActive: boolean; onSelect: () => void;
  onSelectChild?: (slug: string) => void; indent?: boolean;
}) {
  const [open, setOpen] = useState(false);

  if (page.kind === 'folder') {
    return (
      <>
        <SidebarMenuItem className={indent ? 'pl-4' : ''}>
          <SidebarMenuButton onClick={() => setOpen(!open)} isActive={isActive}>
            <ChevronRight size={12} className={`transition-transform ${open ? 'rotate-90' : ''}`} />
            <Folder size={16} />
            <span>{page.title || page.slug}</span>
          </SidebarMenuButton>
        </SidebarMenuItem>
        {open && page.children?.map((child) => (
          <SidebarMenuItem key={child.slug} className={indent ? 'pl-8' : 'pl-4'}>
            <SidebarMenuButton onClick={() => onSelectChild?.(child.slug)} isActive={false}>
              <FileText size={16} />
              <span>{child.title || child.slug}</span>
            </SidebarMenuButton>
          </SidebarMenuItem>
        ))}
      </>
    );
  }

  return (
    <SidebarMenuItem className={indent ? 'pl-4' : ''}>
      <SidebarMenuButton onClick={onSelect} isActive={isActive}>
        <FileText size={16} />
        <span>{page.title || page.slug}</span>
      </SidebarMenuButton>
    </SidebarMenuItem>
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
              <button
                onClick={() => handleJoin(ws.slug)}
                disabled={joining === ws.slug}
                className="text-xs text-white bg-[var(--color-text)] px-3 py-1.5 rounded-md hover:opacity-90 disabled:opacity-40 transition-opacity"
              >
                {joining === ws.slug ? 'Joining...' : 'Join'}
              </button>
            </div>
          ))}
        </div>
      )}
    </>
  );
}
