import { useState, useCallback } from 'react';
import { Wand2, ArrowUpRight, RefreshCw } from 'lucide-react';
import { PageList } from '../components/PageList';
import { IngestForm } from '../components/IngestForm';
import { compile, submit, listPages } from '../api';

const BRANCH = 'user/default';

export function PersonalPage() {
  const [refreshKey, setRefreshKey] = useState(0);
  const [compiling, setCompiling] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [message, setMessage] = useState<{ text: string; type: 'info' | 'success' | 'error' } | null>(null);
  const [showIngest, setShowIngest] = useState(false);

  const refresh = useCallback(() => setRefreshKey((k) => k + 1), []);

  const handleCompile = async () => {
    setCompiling(true);
    setMessage(null);
    try {
      const res = await compile(BRANCH);
      setMessage({ text: `Compiled ${res.pages?.length || 0} page(s)`, type: 'success' });
      refresh();
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
      const pages = await listPages(BRANCH);
      if (pages.length === 0) {
        setMessage({ text: 'No pages to submit', type: 'info' });
        return;
      }
      const slugs = pages.map((p) => p.slug);
      const res = await submit(BRANCH, slugs);
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

  return (
    <div>
      <h1 className="text-4xl font-bold text-[var(--color-text)] mb-1" style={{ fontFamily: 'var(--font-serif)' }}>
        My Space
      </h1>
      <p className="text-[var(--color-text-secondary)] text-sm mb-6">
        Draft pages and sources. Only you can see this.
      </p>

      {/* Actions */}
      <div className="flex items-center gap-2 mb-6">
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
          {compiling ? <RefreshCw size={14} className="animate-spin" /> : <Wand2 size={14} />}
          {compiling ? 'Compiling...' : 'Compile'}
        </button>
        <button
          onClick={handleSubmit}
          disabled={submitting}
          className="flex items-center gap-1.5 rounded-md border border-[var(--color-text)] px-3 py-1.5 text-sm text-[var(--color-text)] hover:bg-[var(--color-bg-hover)] disabled:opacity-40 transition-colors"
        >
          <ArrowUpRight size={14} />
          {submitting ? 'Submitting...' : 'Submit to shared'}
        </button>
      </div>

      {/* Message */}
      {message && (
        <div
          className={`mb-4 rounded-md px-3 py-2 text-sm ${
            message.type === 'success'
              ? 'bg-green-50 text-[var(--color-green)]'
              : message.type === 'error'
                ? 'bg-red-50 text-[var(--color-red)]'
                : 'bg-[var(--color-bg-hover)] text-[var(--color-text-secondary)]'
          }`}
        >
          {message.text}
        </div>
      )}

      {/* Ingest form */}
      {showIngest && (
        <div className="mb-6 rounded-md border border-[var(--color-border)] p-4 bg-[var(--color-bg-secondary)]">
          <div className="text-xs font-medium text-[var(--color-text-tertiary)] uppercase tracking-wider mb-3">
            Add source
          </div>
          <IngestForm branch={BRANCH} onDone={() => { refresh(); setShowIngest(false); }} />
        </div>
      )}

      {/* Pages */}
      <div className="border-t border-[var(--color-border)] pt-4">
        <div className="text-xs font-medium text-[var(--color-text-tertiary)] uppercase tracking-wider mb-3">
          Draft pages
        </div>
        <PageList key={refreshKey} branch={BRANCH} />
      </div>
    </div>
  );
}
