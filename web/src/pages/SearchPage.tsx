import { useEffect, useState } from 'react';
import { useSearchParams, Link } from 'react-router-dom';
import { search, type SearchResult } from '../api';
import { SearchBar } from '../components/SearchBar';

export function SearchPage() {
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
    <div className="space-y-6">
      <h1 className="text-2xl font-bold text-stone-800">Search</h1>
      <SearchBar />

      {loading ? (
        <div className="text-center py-12 text-stone-400">Searching...</div>
      ) : results.length === 0 && q ? (
        <div className="text-center py-12 text-stone-400">No results for "{q}"</div>
      ) : (
        <div className="space-y-2">
          {results.map((r) => (
            <Link
              key={r.slug}
              to={`/page/${r.slug}?branch=main`}
              className="block rounded-lg border border-stone-200 bg-white p-4 hover:border-stone-300 transition"
            >
              <div className="flex justify-between">
                <h3 className="font-medium text-stone-800">{r.title || r.slug}</h3>
                <span className="text-xs text-stone-400">
                  {(r.similarity * 100).toFixed(0)}% match
                </span>
              </div>
              {r.summary && <p className="text-sm text-stone-500 mt-1">{r.summary}</p>}
            </Link>
          ))}
        </div>
      )}
    </div>
  );
}
