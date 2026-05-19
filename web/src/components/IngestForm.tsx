import { useState } from 'react';
import { ingest } from '../api';

export function IngestForm({ branch, onDone }: { branch: string; onDone: () => void }) {
  const [sourceType, setSourceType] = useState<'text' | 'url'>('text');
  const [content, setContent] = useState('');
  const [loading, setLoading] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!content.trim()) return;
    setLoading(true);
    try {
      await ingest(sourceType, content, branch);
      setContent('');
      onDone();
    } finally {
      setLoading(false);
    }
  };

  return (
    <form onSubmit={handleSubmit} className="space-y-3">
      <div className="flex gap-2">
        <button
          type="button"
          onClick={() => setSourceType('text')}
          className={`px-3 py-1 rounded text-sm ${
            sourceType === 'text'
              ? 'bg-stone-800 text-white'
              : 'bg-stone-100 text-stone-600'
          }`}
        >
          Text
        </button>
        <button
          type="button"
          onClick={() => setSourceType('url')}
          className={`px-3 py-1 rounded text-sm ${
            sourceType === 'url'
              ? 'bg-stone-800 text-white'
              : 'bg-stone-100 text-stone-600'
          }`}
        >
          URL
        </button>
      </div>
      {sourceType === 'url' ? (
        <input
          type="url"
          value={content}
          onChange={(e) => setContent(e.target.value)}
          placeholder="https://..."
          className="w-full rounded-lg border border-stone-200 px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-stone-300"
        />
      ) : (
        <textarea
          value={content}
          onChange={(e) => setContent(e.target.value)}
          placeholder="Paste your content here..."
          rows={6}
          className="w-full rounded-lg border border-stone-200 px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-stone-300 resize-none"
        />
      )}
      <button
        type="submit"
        disabled={loading || !content.trim()}
        className="rounded-lg bg-stone-800 px-4 py-2 text-sm text-white hover:bg-stone-700 disabled:opacity-50"
      >
        {loading ? 'Ingesting...' : 'Ingest'}
      </button>
    </form>
  );
}
