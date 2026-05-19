import { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
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
    return <div className="text-center py-12 text-stone-400">Loading...</div>;
  }

  if (pages.length === 0) {
    return (
      <div className="text-center py-16 text-stone-400">
        <p className="text-lg">No pages yet</p>
        <p className="text-sm mt-2">
          {branch === 'main'
            ? 'Submit pages from your personal space to see them here.'
            : 'Ingest a source or write a page to get started.'}
        </p>
      </div>
    );
  }

  return (
    <div className="space-y-2">
      {pages.map((p) => (
        <Link
          key={p.slug}
          to={`/page/${p.slug}?branch=${branch}`}
          className="block rounded-lg border border-stone-200 bg-white p-4 hover:border-stone-300 hover:shadow-sm transition"
        >
          <h3 className="font-medium text-stone-800">{p.title || p.slug}</h3>
          {p.summary && (
            <p className="text-sm text-stone-500 mt-1 line-clamp-2">{p.summary}</p>
          )}
          <p className="text-xs text-stone-400 mt-2">
            {new Date(p.updated_at).toLocaleDateString()}
          </p>
        </Link>
      ))}
    </div>
  );
}
