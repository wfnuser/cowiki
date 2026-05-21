import { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import { Plus, FolderOpen, FileText, Clock } from 'lucide-react';
import { listWorkspaces, createWorkspace, type Workspace } from '../api';
import { getStoredAuth } from '../auth';

export function HomePage() {
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [loading, setLoading] = useState(true);
  const [showCreate, setShowCreate] = useState(false);
  const [newName, setNewName] = useState('');
  const [newSlug, setNewSlug] = useState('');
  const [creating, setCreating] = useState(false);

  const auth = getStoredAuth();

  const load = () => {
    setLoading(true);
    listWorkspaces().then(setWorkspaces).finally(() => setLoading(false));
  };

  useEffect(() => { load(); }, []);

  // Auto-generate slug from name
  const handleNameChange = (name: string) => {
    setNewName(name);
    setNewSlug(
      name
        .toLowerCase()
        .replace(/[^a-z0-9\s-]/g, '')
        .replace(/\s+/g, '-')
        .replace(/-+/g, '-')
        .trim()
    );
  };

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newName.trim() || !newSlug.trim()) return;
    setCreating(true);
    try {
      await createWorkspace(newName.trim(), newSlug.trim());
      setNewName('');
      setNewSlug('');
      setShowCreate(false);
      load();
    } finally {
      setCreating(false);
    }
  };

  // Recent pages placeholder — in the future, track actual recently opened pages
  const recentPages: { title: string; workspace: string; slug: string; time: string }[] = [];

  return (
    <div className="min-h-screen bg-[var(--color-bg)]">
      <header className="border-b border-[var(--color-border)]">
        <div className="max-w-5xl mx-auto px-6 py-4 flex items-center justify-between">
          <div className="flex items-center gap-2">
            <div className="w-6 h-6 rounded bg-[var(--color-text)] flex items-center justify-center">
              <span className="text-white text-xs font-bold">c</span>
            </div>
            <span className="font-semibold text-sm">CoWiki</span>
          </div>
          <div className="flex items-center gap-2">
            <div className="flex h-6 w-6 items-center justify-center rounded-full bg-[var(--color-bg-active)] text-xs font-medium">
              {auth?.name?.[0]?.toUpperCase() || 'U'}
            </div>
            <span className="text-xs text-[var(--color-text-secondary)]">{auth?.name}</span>
          </div>
        </div>
      </header>

      <div className="max-w-5xl mx-auto px-6 py-10">
        {/* Recent pages */}
        {recentPages.length > 0 && (
          <section className="mb-10">
            <h2 className="text-xs font-medium text-[var(--color-text-tertiary)] uppercase tracking-wider mb-3">
              Recently opened
            </h2>
            <div className="grid grid-cols-3 gap-3">
              {recentPages.map((p, i) => (
                <Link
                  key={i}
                  to={`/w/${p.workspace}/page/${p.slug}`}
                  className="rounded-lg border border-[var(--color-border)] bg-white p-4 hover:border-[var(--color-border-hover)] transition-colors"
                >
                  <div className="flex items-center gap-2 text-sm text-[var(--color-text)]">
                    <FileText size={14} className="text-[var(--color-text-tertiary)]" />
                    {p.title}
                  </div>
                  <div className="flex items-center gap-1 mt-2 text-xs text-[var(--color-text-tertiary)]">
                    <Clock size={10} /> {p.time}
                  </div>
                </Link>
              ))}
            </div>
          </section>
        )}

        {/* Workspaces */}
        <section>
          <div className="flex items-center justify-between mb-4">
            <h2 className="text-xs font-medium text-[var(--color-text-tertiary)] uppercase tracking-wider">
              Shared Spaces
            </h2>
            <button
              onClick={() => setShowCreate(!showCreate)}
              className="flex items-center gap-1.5 text-xs text-[var(--color-text-secondary)] hover:text-[var(--color-text)] transition-colors"
            >
              <Plus size={14} /> New space
            </button>
          </div>

          {/* Create form */}
          {showCreate && (
            <form onSubmit={handleCreate} className="mb-6 rounded-lg border border-[var(--color-border)] p-4 bg-[var(--color-bg-secondary)]">
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
                  {creating ? 'Creating...' : 'Create space'}
                </button>
              </div>
            </form>
          )}

          {loading ? (
            <div className="py-8 text-center text-[var(--color-text-tertiary)] text-sm">Loading...</div>
          ) : workspaces.length === 0 ? (
            <div className="py-16 text-center">
              <FolderOpen size={32} className="mx-auto text-[var(--color-text-tertiary)] mb-3" />
              <p className="text-[var(--color-text-tertiary)] text-sm mb-1">No shared spaces yet</p>
              <p className="text-[var(--color-text-tertiary)] text-xs">Create one to start building knowledge with your team.</p>
            </div>
          ) : (
            <div className="grid grid-cols-2 gap-3">
              {workspaces.map((ws) => (
                <Link
                  key={ws.id}
                  to={`/w/${ws.slug}`}
                  className="rounded-lg border border-[var(--color-border)] bg-white p-5 hover:border-[var(--color-border-hover)] hover:shadow-sm transition-all group"
                >
                  <div className="flex items-center gap-3">
                    <div className="w-10 h-10 rounded-lg bg-[var(--color-bg-hover)] flex items-center justify-center text-lg">
                      {ws.name[0]?.toUpperCase()}
                    </div>
                    <div>
                      <div className="text-sm font-medium text-[var(--color-text)] group-hover:text-[var(--color-text)]">
                        {ws.name}
                      </div>
                      <div className="text-xs text-[var(--color-text-tertiary)]">
                        {ws.role} · /{ws.slug}
                      </div>
                    </div>
                  </div>
                </Link>
              ))}
            </div>
          )}
        </section>
      </div>
    </div>
  );
}
