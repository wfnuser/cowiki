import { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import { FileText } from 'lucide-react';
import { listPages, type PageMeta } from '../api';

export function PageList({ branch = 'main' }: { branch?: string }) {
  const [pages, setPages] = useState<PageMeta[]>([]);
  const [loading, setLoading] = useState(true);

  const load = () => {
    setLoading(true);
    listPages(branch).then(setPages).finally(() => setLoading(false));
  };

  useEffect(() => { load(); }, [branch]);

  if (loading) {
    return (
      <div className="py-8 text-center text-[var(--color-text-tertiary)] text-sm">
        Loading...
      </div>
    );
  }

  if (pages.length === 0) {
    return (
      <div className="py-16 text-center">
        <p className="text-[var(--color-text-tertiary)] text-sm">
          {branch === 'main'
            ? 'No pages in the shared wiki yet.'
            : 'Your space is empty. Ingest a source to get started.'}
        </p>
      </div>
    );
  }

  return (
    <div>
      {pages.map((p) => (
        <Link
          key={p.slug}
          to={`/page/${p.slug}?branch=${branch}`}
          className="flex items-start gap-2.5 px-2 py-2 -mx-2 rounded-md hover:bg-[var(--color-bg-hover)] transition-colors group"
        >
          <FileText
            size={16}
            className="mt-0.5 shrink-0 text-[var(--color-text-tertiary)] group-hover:text-[var(--color-text-secondary)]"
            strokeWidth={1.5}
          />
          <div className="min-w-0">
            <div className="text-sm text-[var(--color-text)] group-hover:text-[var(--color-text)]">
              {p.title || p.slug}
            </div>
            {p.summary && (
              <div className="text-xs text-[var(--color-text-tertiary)] mt-0.5 truncate">
                {p.summary}
              </div>
            )}
          </div>
        </Link>
      ))}
    </div>
  );
}
