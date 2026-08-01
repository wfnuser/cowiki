import { useEffect, useMemo, useState } from 'react';
import { Globe2 } from 'lucide-react';
import { Link, useNavigate, useParams } from 'react-router-dom';
import { PageReader } from '../components/PageReader';
import { splitSystemFrontmatter } from '../lib/page-frontmatter';
import { apiOrigin } from '../runtime';
import {
  createPublicCloudClient,
  type CloudContent,
  type CloudTree,
  type PublicCloudSpace,
} from './client';
import { resolveInitialCloudPage } from './cloud-shell-model';

export function PublicCloudSpacePage() {
  const { slug = '', '*': routePath = '' } = useParams();
  const navigate = useNavigate();
  const client = useMemo(
    () => createPublicCloudClient(apiOrigin() || window.location.origin),
    [],
  );
  const [space, setSpace] = useState<PublicCloudSpace | null>(null);
  const [tree, setTree] = useState<CloudTree | null>(null);
  const [content, setContent] = useState<CloudContent | null>(null);
  const [error, setError] = useState('');
  const documentPath = routePath ? decodeRoutePath(routePath) : '';
  const pages = tree?.entries.filter((entry) => entry.kind === 'page') ?? [];

  useEffect(() => {
    let active = true;
    Promise.all([client.getSpace(slug), client.getTree(slug)])
      .then(([nextSpace, nextTree]) => {
        if (!active) return;
        setSpace(nextSpace);
        setTree(nextTree);
        setError('');
      })
      .catch((cause) => {
        if (active) {
          setError(cause instanceof Error ? cause.message : 'This Space is unavailable.');
        }
      });
    return () => { active = false; };
  }, [client, slug]);

  useEffect(() => {
    if (!tree || documentPath) return;
    const initial = resolveInitialCloudPage(tree.entries);
    if (initial) navigate(publicDocumentRoute(slug, initial), { replace: true });
  }, [documentPath, navigate, slug, tree]);

  useEffect(() => {
    if (!documentPath) {
      setContent(null);
      return;
    }
    let active = true;
    void client.getContent(slug, documentPath)
      .then((next) => {
        if (!active) return;
        setContent(next);
        setError('');
      })
      .catch((cause) => {
        if (active) {
          setError(cause instanceof Error ? cause.message : 'Could not load this page.');
        }
      });
    return () => { active = false; };
  }, [client, documentPath, slug]);

  if (error) {
    return (
      <main className="grid min-h-screen place-items-center bg-bg px-6 text-text">
        <div className="max-w-md rounded-xl border bg-panel p-8 text-center">
          <h1 className="font-serif text-2xl font-semibold">Space unavailable</h1>
          <p className="mt-3 text-sm leading-6 text-text-tertiary">
            It may be private, missing, or temporarily unavailable.
          </p>
          <Link className="mt-5 inline-block text-sm text-accent hover:underline" to="/login">
            Sign in with GitHub
          </Link>
        </div>
      </main>
    );
  }

  if (!space || !tree) {
    return <div className="grid min-h-screen place-items-center bg-bg text-sm text-text-tertiary">Loading Space…</div>;
  }

  return (
    <div className="flex min-h-screen bg-bg text-text">
      <aside className="hidden w-72 shrink-0 border-r bg-panel md:block">
        <div className="border-b p-5">
          <div className="mb-2 flex items-center gap-2 text-xs font-semibold uppercase tracking-wide text-green">
            <Globe2 size={14} /> Public Space
          </div>
          <h1 className="font-serif text-xl font-semibold">{space.name}</h1>
          <p className="mt-1 text-xs text-text-tertiary">Merged pages from Cloud main</p>
        </div>
        <nav aria-label="Published pages" className="space-y-1 p-3">
          {pages.map((page) => (
            <Link
              key={page.path}
              className={`block truncate rounded-md px-3 py-2 text-sm ${
                page.path === documentPath
                  ? 'bg-secondary font-semibold text-text'
                  : 'text-text-secondary hover:bg-secondary'
              }`}
              to={publicDocumentRoute(slug, page.path)}
            >
              {page.path}
            </Link>
          ))}
        </nav>
      </aside>

      <main className="flex min-w-0 flex-1 flex-col">
        <header className="flex h-14 shrink-0 items-center justify-between border-b bg-panel px-5 md:px-8">
          <div className="min-w-0 truncate text-sm font-semibold md:hidden">{space.name}</div>
          <div className="hidden text-xs text-text-tertiary md:block">{documentPath || 'Published knowledge'}</div>
          <Link className="text-xs font-semibold text-accent hover:underline" to="/login">
            Sign in to collaborate
          </Link>
        </header>
        <div className="relative min-h-0 flex-1">
          {documentPath && content?.path === documentPath ? (
            <PageReader body={splitSystemFrontmatter(content.content).body} />
          ) : pages.length === 0 ? (
            <div className="p-10 text-sm text-text-tertiary">No published Markdown pages yet.</div>
          ) : (
            <div className="p-10 text-sm text-text-tertiary">Loading page…</div>
          )}
        </div>
      </main>
    </div>
  );
}

function publicDocumentRoute(slug: string, path: string): string {
  const encodedSlug = encodeURIComponent(slug);
  const encodedPath = path.split('/').map(encodeURIComponent).join('/');
  return `/spaces/${encodedSlug}/${encodedPath}`;
}

function decodeRoutePath(value: string): string {
  try {
    return value.split('/').map(decodeURIComponent).join('/');
  } catch {
    return '';
  }
}
