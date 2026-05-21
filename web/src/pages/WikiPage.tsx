import { useEffect, useState, useCallback } from 'react';
import { Link } from 'react-router-dom';
import { FileText, Folder, RefreshCw, Wand2, ArrowUpRight, Eye } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { SearchBar } from '../components/SearchBar';
import { IngestForm } from '../components/IngestForm';
import { listPages, compile, submit, type PageMeta } from '../api';
import { getStoredAuth } from '../auth';

type ViewMode = 'drafts' | 'main';

interface TreeNode {
  name: string;
  path: string;
  pages: PageMeta[];
  children: TreeNode[];
  isDraft?: boolean;
}

function buildTree(pages: PageMeta[], draftSlugs: Set<string>): TreeNode {
  const root: TreeNode = { name: '', path: '', pages: [], children: [] };

  for (const page of pages) {
    const slug = page.slug;
    const parts = slug.split('/');

    if (parts.length === 1) {
      root.pages.push({ ...page, slug });
    } else {
      let node = root;
      for (let i = 0; i < parts.length - 1; i++) {
        const dirName = parts[i];
        let child = node.children.find((c) => c.name === dirName);
        if (!child) {
          child = { name: dirName, path: parts.slice(0, i + 1).join('/'), pages: [], children: [] };
          node.children.push(child);
        }
        node = child;
      }
      node.pages.push({ ...page, slug });
    }
  }

  return root;
}

function mergeTrees(mainPages: PageMeta[], draftPages: PageMeta[]): { pages: (PageMeta & { isDraft?: boolean; isModified?: boolean })[]; } {
  const mainMap = new Map(mainPages.map((p) => [p.slug, p]));
  const merged: (PageMeta & { isDraft?: boolean; isModified?: boolean })[] = [];

  // Add all main pages
  for (const p of mainPages) {
    merged.push(p);
  }

  // Add draft-only pages, mark modified ones
  for (const p of draftPages) {
    const existing = mainMap.get(p.slug);
    if (!existing) {
      merged.push({ ...p, isDraft: true });
    } else if (existing.content_hash !== p.content_hash) {
      // Replace with draft version, mark as modified
      const idx = merged.findIndex((m) => m.slug === p.slug);
      if (idx >= 0) {
        merged[idx] = { ...p, isModified: true };
      }
    }
  }

  return { pages: merged };
}

export function WikiPage() {
  const [viewMode, setViewMode] = useState<ViewMode>('drafts');
  const [mainPages, setMainPages] = useState<PageMeta[]>([]);
  const [draftPages, setDraftPages] = useState<PageMeta[]>([]);
  const [loading, setLoading] = useState(true);
  const [showIngest, setShowIngest] = useState(false);
  const [compiling, setCompiling] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [message, setMessage] = useState<{ text: string; type: 'success' | 'error' | 'info' } | null>(null);

  const auth = getStoredAuth();
  const userBranch = auth ? `user/${auth.id}` : 'user/default';

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [main, drafts] = await Promise.all([
        listPages('main'),
        listPages(userBranch),
      ]);
      setMainPages(main);
      setDraftPages(drafts);
    } finally {
      setLoading(false);
    }
  }, [userBranch]);

  useEffect(() => { load(); }, [load]);

  const handleCompile = async () => {
    setCompiling(true);
    setMessage(null);
    try {
      const res = await compile(userBranch);
      const count = res.pages?.length || 0;
      const skipped = res.skipped || 0;
      setMessage({
        text: `Compiled ${count} page(s)${skipped > 0 ? `, ${skipped} skipped (unchanged)` : ''}`,
        type: count > 0 ? 'success' : 'info',
      });
      load();
    } catch {
      setMessage({ text: 'Compilation failed', type: 'error' });
    } finally {
      setCompiling(false);
    }
  };

  const handleSubmit = async () => {
    setSubmitting(true);
    setMessage(null);
    try {
      const pages = await listPages(userBranch);
      if (pages.length === 0) {
        setMessage({ text: 'No pages to submit', type: 'info' });
        return;
      }
      const slugs = pages.map((p) => p.slug);
      const res = await submit(userBranch, slugs);
      const dupCount = res.duplicates?.length || 0;
      setMessage({
        text: `Submitted for review.${dupCount > 0 ? ` ${dupCount} possible duplicate(s).` : ''}`,
        type: 'success',
      });
    } catch {
      setMessage({ text: 'Submit failed', type: 'error' });
    } finally {
      setSubmitting(false);
    }
  };

  // Determine which pages to show
  const displayPages = viewMode === 'main'
    ? mainPages
    : mergeTrees(mainPages, draftPages).pages;

  const draftCount = draftPages.filter(
    (d) => !mainPages.find((m) => m.slug === d.slug && m.content_hash === d.content_hash)
  ).length;

  return (
    <div>
      <div className="flex items-center justify-between mb-1">
        <h1 className="text-4xl font-bold" style={{ fontFamily: 'var(--font-serif)' }}>
          Wiki
        </h1>
        {/* View mode toggle */}
        <div className="flex items-center gap-1 rounded-md border border-[var(--color-border)] p-0.5">
          <button
            onClick={() => setViewMode('drafts')}
            className={`flex items-center gap-1.5 px-2.5 py-1 rounded text-xs transition-colors ${
              viewMode === 'drafts'
                ? 'bg-[var(--color-bg-active)] text-[var(--color-text)]'
                : 'text-[var(--color-text-tertiary)] hover:text-[var(--color-text-secondary)]'
            }`}
          >
            <Eye size={12} />
            My Drafts
            {draftCount > 0 && (
              <span className="bg-[var(--color-amber)] text-white text-[10px] px-1 rounded-full">
                {draftCount}
              </span>
            )}
          </button>
          <button
            onClick={() => setViewMode('main')}
            className={`px-2.5 py-1 rounded text-xs transition-colors ${
              viewMode === 'main'
                ? 'bg-[var(--color-bg-active)] text-[var(--color-text)]'
                : 'text-[var(--color-text-tertiary)] hover:text-[var(--color-text-secondary)]'
            }`}
          >
            Main
          </button>
        </div>
      </div>

      <p className="text-[var(--color-text-tertiary)] text-sm mb-4">
        {viewMode === 'drafts'
          ? 'Shared wiki with your uncommitted drafts.'
          : 'Shared wiki — approved content only.'}
      </p>

      {/* Action bar */}
      <div className="flex items-center gap-2 mb-4">
        <button
          onClick={() => setShowIngest(!showIngest)}
          className="flex items-center gap-1.5 rounded-md border border-[var(--color-border)] px-3 py-1.5 text-sm text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)] transition-colors"
        >
          + Add source
        </button>
        <button
          onClick={handleCompile}
          disabled={compiling}
          className="flex items-center gap-1.5 rounded-md bg-[var(--color-text)] text-white px-3 py-1.5 text-sm hover:opacity-90 disabled:opacity-40 transition-opacity"
        >
          {compiling ? <RefreshCw className="h-3.5 w-3.5 animate-spin" /> : <Wand2 className="h-3.5 w-3.5" />}
          {compiling ? 'Compiling...' : 'Compile'}
        </button>
        <button
          onClick={handleSubmit}
          disabled={submitting}
          className="flex items-center gap-1.5 rounded-md border border-[var(--color-text)] px-3 py-1.5 text-sm text-[var(--color-text)] hover:bg-[var(--color-bg-hover)] disabled:opacity-40 transition-colors"
        >
          <ArrowUpRight className="h-3.5 w-3.5" />
          {submitting ? 'Submitting...' : 'Submit to shared'}
        </button>
      </div>

      {/* Message */}
      {message && (
        <div className={`mb-4 rounded px-3 py-2 text-[13px] ${
          message.type === 'success' ? 'bg-green-50 text-green-700' :
          message.type === 'error' ? 'bg-red-50 text-red-700' :
          'bg-[var(--color-bg-hover)] text-[var(--color-text-secondary)]'
        }`}>
          {message.text}
        </div>
      )}

      {/* Ingest form */}
      {showIngest && (
        <div className="mb-6 rounded-md border border-[var(--color-border)] p-4 bg-[var(--color-bg-secondary)]">
          <div className="text-xs text-[var(--color-text-tertiary)] mb-3">Add source</div>
          <IngestForm branch={userBranch} onDone={() => { load(); setShowIngest(false); }} />
        </div>
      )}

      {/* Search */}
      <div className="mb-6">
        <SearchBar />
      </div>

      {/* Page list */}
      {loading ? (
        <div className="py-8 text-center text-[var(--color-text-tertiary)] text-sm">Loading...</div>
      ) : displayPages.length === 0 ? (
        <div className="py-16 text-center">
          <p className="text-[var(--color-text-tertiary)] text-sm">
            No pages yet. Add a source and compile to get started.
          </p>
        </div>
      ) : (
        <div>
          {displayPages.map((p: any) => (
            <Link
              key={p.slug}
              to={`/page/${p.slug}?branch=${p.isDraft || p.isModified ? userBranch : 'main'}`}
              className="flex items-start gap-2.5 px-2 py-2 -mx-2 rounded-md hover:bg-[var(--color-bg-hover)] transition-colors group"
            >
              <FileText
                size={16}
                className="mt-0.5 shrink-0 text-[var(--color-text-tertiary)] group-hover:text-[var(--color-text-secondary)]"
                strokeWidth={1.5}
              />
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <span className="text-sm text-[var(--color-text)]">{p.title || p.slug}</span>
                  {p.isDraft && (
                    <Badge variant="outline" className="text-[10px] px-1.5 py-0 text-[var(--color-amber)] border-[var(--color-amber)]">
                      Draft
                    </Badge>
                  )}
                  {p.isModified && (
                    <Badge variant="outline" className="text-[10px] px-1.5 py-0 text-[var(--color-accent)] border-[var(--color-accent)]">
                      Modified
                    </Badge>
                  )}
                </div>
                {p.summary && (
                  <div className="text-xs text-[var(--color-text-tertiary)] mt-0.5 truncate">{p.summary}</div>
                )}
              </div>
              <span className="ml-auto text-xs text-[var(--color-text-tertiary)] shrink-0 opacity-0 group-hover:opacity-100 transition-opacity">
                {new Date(p.updated_at).toLocaleDateString()}
              </span>
            </Link>
          ))}
        </div>
      )}
    </div>
  );
}
