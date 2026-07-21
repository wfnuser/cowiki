import { useCallback, useEffect, useMemo, useState } from 'react';
import { FileText, Folder, GitBranch } from 'lucide-react';
import ReactMarkdown from 'react-markdown';
import { Link, useNavigate } from 'react-router-dom';
import remarkGfm from 'remark-gfm';
import type { CloudClient, CloudContent, CloudSpace, CloudTree } from './client';
import { resolveInitialCloudPage } from './cloud-shell-model';
import { CloudNotice } from './CloudHome';
import { cloudSpaceRoute } from './routes';

export function CloudWikiView({
  client,
  space,
  documentPath,
}: {
  client: CloudClient;
  space: CloudSpace;
  documentPath?: string;
}) {
  const navigate = useNavigate();
  const [tree, setTree] = useState<CloudTree | null>(null);
  const [content, setContent] = useState<CloudContent | null>(null);
  const [error, setError] = useState('');
  const pages = useMemo(() => tree?.entries.filter((entry) => entry.kind === 'page') ?? [], [tree]);

  const loadTree = useCallback(async () => {
    setError('');
    try {
      setTree(await client.getTree(space.id));
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Could not load Cloud main.');
    }
  }, [client, space.id]);

  useEffect(() => { void loadTree(); }, [loadTree]);
  useEffect(() => {
    if (!tree || documentPath) return;
    const initial = resolveInitialCloudPage(tree.entries);
    if (initial) navigate(cloudSpaceRoute(space.id, 'wiki', initial), { replace: true });
  }, [documentPath, navigate, space.id, tree]);
  useEffect(() => {
    if (!documentPath) {
      setContent(null);
      return;
    }
    let active = true;
    setContent(null);
    setError('');
    void client.getContent(space.id, documentPath)
      .then((next) => { if (active) setContent(next); })
      .catch((cause) => { if (active) setError(cause instanceof Error ? cause.message : 'Could not load this page.'); });
    return () => { active = false; };
  }, [client, documentPath, space.id]);

  return (
    <div className="grid h-[calc(100vh-56px)] grid-cols-[250px_minmax(0,1fr)]">
      <aside className="overflow-y-auto border-r bg-panel px-3 py-5">
        <div className="px-3 pb-3 text-[11px] font-bold uppercase tracking-[0.12em] text-text-tertiary">Published pages</div>
        {tree?.entries.map((entry) => entry.kind === 'folder' ? (
          <div key={entry.path} className="flex items-center gap-2 px-3 py-1.5 text-xs font-semibold text-text-tertiary" style={{ paddingLeft: 12 + entry.path.split('/').length * 8 }}>
            <Folder size={14} /> {entry.path.split('/').at(-1)}
          </div>
        ) : (
          <Link
            key={entry.path}
            to={cloudSpaceRoute(space.id, 'wiki', entry.path)}
            className={`flex items-center gap-2 rounded-md px-3 py-1.5 text-[13px] no-underline ${documentPath === entry.path ? 'bg-accent-soft font-semibold text-accent' : 'text-text-secondary hover:bg-bg-hover'}`}
            style={{ paddingLeft: 12 + (entry.path.split('/').length - 1) * 8 }}
          >
            <FileText size={14} />
            <span className="truncate">{pageLabel(entry.path)}</span>
          </Link>
        ))}
        {tree && pages.length === 0 && <p className="px-3 text-xs leading-5 text-text-tertiary">Cloud main has no visible Markdown pages.</p>}
      </aside>

      <section className="overflow-y-auto">
        <div className="border-b bg-panel px-8 py-3 text-xs text-text-tertiary">
          <span className="inline-flex items-center gap-1.5"><GitBranch size={13} /> main</span>
          {tree?.oid && <span className="ml-3 font-mono">{tree.oid.slice(0, 8)}</span>}
          <span className="ml-3">Read only</span>
        </div>
        <article className="mx-auto w-full max-w-[860px] px-10 py-14">
          {error && <CloudNotice tone="error">{error}</CloudNotice>}
          {!error && documentPath && !content && <p className="text-sm text-text-tertiary">Loading page…</p>}
          {!error && content && (
            <>
              <div className="mb-7 text-xs text-text-tertiary">{content.path}</div>
              <div className="prose max-w-none"><ReactMarkdown remarkPlugins={[remarkGfm]}>{content.content}</ReactMarkdown></div>
            </>
          )}
          {!error && !documentPath && tree && pages.length === 0 && (
            <div className="py-20 text-center text-sm text-text-tertiary">Publish a Markdown page to Cloud main to see it here.</div>
          )}
        </article>
      </section>
    </div>
  );
}

function pageLabel(path: string): string {
  return path.split('/').at(-1)?.replace(/\.md$/i, '').replace(/[-_]/g, ' ') || path;
}

