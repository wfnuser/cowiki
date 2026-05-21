import { useEffect, useState } from 'react';
import { useSearchParams, useParams, Link } from 'react-router-dom';
import { FileText } from 'lucide-react';
import { search, type SearchResult } from '../api';

export function SearchPage() {
  const { workspaceSlug } = useParams();
  const [searchParams] = useSearchParams();
  const q = searchParams.get('q') || '';
  const [results, setResults] = useState<SearchResult[]>([]);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (!q) return;
    setLoading(true);
    search(q).then(setResults).finally(() => setLoading(false));
  }, [q]);

  return (
    <div>
      <h1 className="text-4xl font-bold mb-1" style={{ fontFamily: 'var(--font-serif)' }}>
        Search
      </h1>
      <p className="text-[var(--color-text-tertiary)] text-sm mb-6">
        {q ? `Results for "${q}"` : 'Use the search bar in the sidebar.'}
      </p>

      {loading ? (
        <div className="py-8 text-center text-[var(--color-text-tertiary)] text-sm">Searching...</div>
      ) : results.length === 0 && q ? (
        <div className="py-12 text-center text-[var(--color-text-tertiary)] text-sm">
          No results for "{q}"
        </div>
      ) : (
        <div>
          {results.map((r) => (
            <Link
              key={r.slug}
              to={`/w/${workspaceSlug}/page/${r.slug}?branch=main`}
              className="flex items-start gap-2.5 px-2 py-2.5 -mx-2 rounded-md hover:bg-[var(--color-bg-hover)] transition-colors group"
            >
              <FileText size={16} className="mt-0.5 shrink-0 text-[var(--color-text-tertiary)]" strokeWidth={1.5} />
              <div className="min-w-0 flex-1">
                <div className="text-sm text-[var(--color-text)]">{r.title || r.slug}</div>
                {r.summary && (
                  <div className="text-xs text-[var(--color-text-tertiary)] mt-0.5">{r.summary}</div>
                )}
              </div>
              <span className="text-xs text-[var(--color-text-tertiary)] shrink-0 tabular-nums">
                {(r.similarity * 100).toFixed(0)}%
              </span>
            </Link>
          ))}
        </div>
      )}
    </div>
  );
}
