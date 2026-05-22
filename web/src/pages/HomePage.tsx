import { useEffect, useState } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import { Plus, FolderOpen, Clock, LogOut, BookOpen } from 'lucide-react';
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

export function HomePage() {
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [loading, setLoading] = useState(true);
  const [showCreate, setShowCreate] = useState(false);
  const [newName, setNewName] = useState('');
  const [newSlug, setNewSlug] = useState('');
  const [creating, setCreating] = useState(false);
  const navigate = useNavigate();
  const auth = getStoredAuth();

  const load = () => {
    setLoading(true);
    listWorkspaces().then(setWorkspaces).finally(() => setLoading(false));
  };

  useEffect(() => { load(); }, []);

  const handleNameChange = (name: string) => {
    setNewName(name);
    setNewSlug(
      name.toLowerCase().replace(/[^a-z0-9\s-]/g, '').replace(/\s+/g, '-').replace(/-+/g, '-').trim()
    );
  };

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newName.trim() || !newSlug.trim()) return;
    setCreating(true);
    try {
      const ws = await createWorkspace(newName.trim(), newSlug.trim());
      setShowCreate(false);
      setNewName('');
      setNewSlug('');
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
            <SidebarGroup>
              <SidebarGroupLabel>
                <span>Knowledge Spaces</span>
                <button
                  onClick={() => setShowCreate(true)}
                  className="ml-auto text-sidebar-foreground/40 hover:text-sidebar-foreground/70 transition-colors"
                >
                  <Plus size={14} />
                </button>
              </SidebarGroupLabel>
              <SidebarMenu>
                {workspaces.map((ws) => (
                  <SidebarMenuItem key={ws.id}>
                    <SidebarMenuButton asChild tooltip={ws.name}>
                      <Link to={`/w/${ws.slug}`}>
                        <BookOpen />
                        <span>{ws.name}</span>
                      </Link>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                ))}
                {!loading && workspaces.length === 0 && (
                  <div className="px-2 py-6 text-xs text-sidebar-foreground/40 text-center">
                    No spaces yet
                  </div>
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
              Home
            </h1>
            <p className="text-[var(--color-text-tertiary)] text-sm mb-8">
              Welcome back, {auth?.name}.
            </p>

            {/* Spaces grid */}
            {!loading && workspaces.length > 0 && (
              <section className="mb-10">
                <h2 className="text-xs font-medium text-[var(--color-text-tertiary)] uppercase tracking-wider mb-3">
                  Knowledge Spaces
                </h2>
                <div className="grid grid-cols-2 gap-3">
                  {workspaces.map((ws) => (
                    <Link
                      key={ws.id}
                      to={`/w/${ws.slug}`}
                      className="rounded-lg border border-[var(--color-border)] bg-white p-5 hover:border-[var(--color-border-hover)] hover:shadow-sm transition-all group"
                    >
                      <div className="flex items-center gap-3">
                        <div className="w-10 h-10 rounded-lg bg-[var(--color-bg-hover)] flex items-center justify-center text-lg font-medium text-[var(--color-text-secondary)]">
                          {ws.name[0]?.toUpperCase()}
                        </div>
                        <div>
                          <div className="text-sm font-medium text-[var(--color-text)]">{ws.name}</div>
                          <div className="text-xs text-[var(--color-text-tertiary)]">{ws.role}</div>
                        </div>
                      </div>
                    </Link>
                  ))}
                  {/* Create new card */}
                  <button
                    onClick={() => setShowCreate(true)}
                    className="rounded-lg border border-dashed border-[var(--color-border)] p-5 hover:border-[var(--color-border-hover)] hover:bg-[var(--color-bg-secondary)] transition-all flex items-center justify-center gap-2 text-sm text-[var(--color-text-tertiary)]"
                  >
                    <Plus size={16} />
                    New Knowledge Space
                  </button>
                </div>
              </section>
            )}

            {/* Empty state */}
            {!loading && workspaces.length === 0 && (
              <div className="py-16 text-center">
                <FolderOpen size={32} className="mx-auto text-[var(--color-text-tertiary)] mb-3" />
                <p className="text-[var(--color-text-secondary)] text-sm mb-1">No knowledge spaces yet</p>
                <p className="text-[var(--color-text-tertiary)] text-xs mb-4">Create one to start building knowledge with your team.</p>
                <button
                  onClick={() => setShowCreate(true)}
                  className="rounded-md bg-[var(--color-text)] text-white px-4 py-1.5 text-sm hover:opacity-90 transition-opacity"
                >
                  Create your first space
                </button>
              </div>
            )}

            {/* Recent activity */}
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

      {/* Create modal */}
      <Dialog open={showCreate} onOpenChange={setShowCreate}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Create Knowledge Space</DialogTitle>
          </DialogHeader>
          <form onSubmit={handleCreate} className="space-y-4 mt-2">
            <div>
              <label className="text-sm text-[var(--color-text-secondary)] mb-1.5 block">
                Space name
              </label>
              <Input
                value={newName}
                onChange={(e) => handleNameChange(e.target.value)}
                placeholder="e.g. Engineering Wiki, Product Knowledge Base"
                autoFocus
              />
            </div>
            <div>
              <label className="text-sm text-[var(--color-text-secondary)] mb-1.5 block">
                URL slug
              </label>
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
    </TooltipProvider>
  );
}
