import { useEffect, useState } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import { Plus, FolderOpen, FileText, Clock, LogOut } from 'lucide-react';
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
      setNewName('');
      setNewSlug('');
      setShowCreate(false);
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
                <span>Spaces</span>
                <button
                  onClick={() => setShowCreate(!showCreate)}
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
                        <FolderOpen />
                        <span>{ws.name}</span>
                      </Link>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                ))}
                {!loading && workspaces.length === 0 && (
                  <div className="px-2 py-4 text-xs text-sidebar-foreground/40 text-center">
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
            {/* Create workspace form */}
            {showCreate && (
              <form onSubmit={handleCreate} className="mb-8 rounded-lg border border-[var(--color-border)] p-5 bg-[var(--color-bg-secondary)]">
                <h2 className="text-sm font-medium text-[var(--color-text)] mb-3">Create a new space</h2>
                <div className="space-y-3">
                  <div>
                    <label className="text-xs text-[var(--color-text-tertiary)] mb-1 block">Name</label>
                    <input
                      type="text"
                      value={newName}
                      onChange={(e) => handleNameChange(e.target.value)}
                      placeholder="My Team"
                      className="w-full rounded-md border border-[var(--color-border)] bg-[var(--color-bg)] px-3 py-1.5 text-sm focus:outline-none focus:border-[var(--color-accent)]"
                      autoFocus
                    />
                  </div>
                  <div>
                    <label className="text-xs text-[var(--color-text-tertiary)] mb-1 block">URL slug</label>
                    <input
                      type="text"
                      value={newSlug}
                      onChange={(e) => setNewSlug(e.target.value)}
                      placeholder="my-team"
                      className="w-full rounded-md border border-[var(--color-border)] bg-[var(--color-bg)] px-3 py-1.5 text-sm font-mono focus:outline-none focus:border-[var(--color-accent)]"
                    />
                  </div>
                  <button
                    type="submit"
                    disabled={creating || !newName.trim() || !newSlug.trim()}
                    className="rounded-md bg-[var(--color-text)] text-white px-4 py-1.5 text-sm hover:opacity-90 disabled:opacity-40 transition-opacity"
                  >
                    {creating ? 'Creating...' : 'Create'}
                  </button>
                </div>
              </form>
            )}

            {/* Main content — recent pages placeholder */}
            <h1 className="text-4xl font-bold mb-1" style={{ fontFamily: 'var(--font-serif)' }}>
              Home
            </h1>
            <p className="text-[var(--color-text-tertiary)] text-sm mb-8">
              Welcome back, {auth?.name}. Select a space from the sidebar or create a new one.
            </p>

            {/* Recent activity placeholder */}
            <div className="py-12 text-center">
              <Clock size={24} className="mx-auto text-[var(--color-text-tertiary)] mb-3" />
              <p className="text-[var(--color-text-tertiary)] text-sm">
                Recently opened pages will appear here.
              </p>
            </div>
          </div>
        </SidebarInset>
      </SidebarProvider>
    </TooltipProvider>
  );
}
