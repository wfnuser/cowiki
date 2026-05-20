import { useState, useCallback } from 'react';
import { Wand2, ArrowUpRight, RefreshCw } from 'lucide-react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
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
      <h1 className="text-4xl font-bold mb-1" style={{ fontFamily: 'var(--font-serif)' }}>
        My Space
      </h1>
      <p className="text-[var(--color-text-tertiary)] text-sm mb-4">
        Draft pages and sources. Only you can see this.
      </p>

      <div className="flex items-center gap-1.5 mb-8">
        <button
          onClick={() => setShowIngest(!showIngest)}
          className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded text-[13px] text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)] transition-colors"
        >
          + Add source
        </button>
        <button
          onClick={handleCompile}
          disabled={compiling}
          className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded text-[13px] text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)] transition-colors disabled:opacity-40"
        >
          {compiling ? <RefreshCw className="h-3.5 w-3.5 animate-spin" /> : <Wand2 className="h-3.5 w-3.5" />}
          {compiling ? 'Compiling...' : 'Compile'}
        </button>
        <button
          onClick={handleSubmit}
          disabled={submitting}
          className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded text-[13px] text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)] transition-colors disabled:opacity-40"
        >
          <ArrowUpRight className="h-3.5 w-3.5" />
          {submitting ? 'Submitting...' : 'Submit to shared'}
        </button>
      </div>

      {message && (
        <div className={`mb-4 rounded px-3 py-2 text-[13px] ${
          message.type === 'success' ? 'bg-green-50 text-green-700' :
          message.type === 'error' ? 'bg-red-50 text-red-700' :
          'bg-[var(--color-bg-hover)] text-[var(--color-text-secondary)]'
        }`}>
          {message.text}
        </div>
      )}

      {showIngest && (
        <Card className="mb-6">
          <CardHeader className="pb-3">
            <CardTitle className="text-sm font-medium text-muted-foreground">Add source</CardTitle>
          </CardHeader>
          <CardContent>
            <IngestForm branch={BRANCH} onDone={() => { refresh(); setShowIngest(false); }} />
          </CardContent>
        </Card>
      )}

      <div className="pt-2">
        <p className="text-xs text-[var(--color-text-tertiary)] mb-2">
          Draft pages
        </p>
        <PageList key={refreshKey} branch={BRANCH} />
      </div>
    </div>
  );
}
