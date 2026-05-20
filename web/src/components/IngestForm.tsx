import { useState } from 'react';
import { Plus, Link2, Type } from 'lucide-react';
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
    <form onSubmit={handleSubmit}>
      {/* Type toggle */}
      <div className="flex gap-1 mb-3">
        <button
          type="button"
          onClick={() => setSourceType('text')}
          className={`flex items-center gap-1.5 px-2.5 py-1 rounded text-xs transition-colors ${
            sourceType === 'text'
              ? 'bg-[var(--color-bg-active)] text-[var(--color-text)]'
              : 'text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]'
          }`}
        >
          <Type size={12} /> Text
        </button>
        <button
          type="button"
          onClick={() => setSourceType('url')}
          className={`flex items-center gap-1.5 px-2.5 py-1 rounded text-xs transition-colors ${
            sourceType === 'url'
              ? 'bg-[var(--color-bg-active)] text-[var(--color-text)]'
              : 'text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]'
          }`}
        >
          <Link2 size={12} /> URL
        </button>
      </div>

      {/* Input */}
      {sourceType === 'url' ? (
        <input
          type="url"
          value={content}
          onChange={(e) => setContent(e.target.value)}
          placeholder="Paste a URL..."
          className="w-full rounded-md border border-[var(--color-border)] bg-[var(--color-bg)] px-3 py-2 text-sm placeholder:text-[var(--color-text-tertiary)] focus:outline-none focus:border-[var(--color-accent)] transition-colors"
        />
      ) : (
        <textarea
          value={content}
          onChange={(e) => setContent(e.target.value)}
          placeholder="Paste or type your content here..."
          rows={5}
          className="w-full rounded-md border border-[var(--color-border)] bg-[var(--color-bg)] px-3 py-2 text-sm placeholder:text-[var(--color-text-tertiary)] focus:outline-none focus:border-[var(--color-accent)] transition-colors resize-none"
        />
      )}

      {/* Submit */}
      <button
        type="submit"
        disabled={loading || !content.trim()}
        className="mt-3 flex items-center gap-1.5 rounded-md bg-[var(--color-text)] text-white px-3 py-1.5 text-sm hover:opacity-90 disabled:opacity-40 transition-opacity"
      >
        <Plus size={14} />
        {loading ? 'Adding...' : 'Add source'}
      </button>
    </form>
  );
}
