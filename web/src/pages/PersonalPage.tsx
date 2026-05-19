import { useState, useCallback } from 'react';
import { PageList } from '../components/PageList';
import { IngestForm } from '../components/IngestForm';
import { compile, submit, listPages } from '../api';

const BRANCH = 'user/default';

export function PersonalPage() {
  const [refreshKey, setRefreshKey] = useState(0);
  const [compiling, setCompiling] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [message, setMessage] = useState('');

  const refresh = useCallback(() => setRefreshKey((k) => k + 1), []);

  const handleCompile = async () => {
    setCompiling(true);
    setMessage('');
    try {
      const res = await compile(BRANCH);
      setMessage(`Compiled ${res.pages?.length || 0} pages`);
      refresh();
    } catch (e) {
      setMessage('Compile failed');
    } finally {
      setCompiling(false);
    }
  };

  const handleSubmit = async () => {
    setSubmitting(true);
    setMessage('');
    try {
      const pages = await listPages(BRANCH);
      if (pages.length === 0) {
        setMessage('No pages to submit');
        return;
      }
      const slugs = pages.map((p) => p.slug);
      const res = await submit(BRANCH, slugs);
      const dupCount = res.duplicates?.length || 0;
      setMessage(
        `Submitted! ${dupCount > 0 ? `${dupCount} possible duplicate(s) found.` : ''} Awaiting review.`
      );
    } catch (e) {
      setMessage('Submit failed');
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold text-stone-800">My Space</h1>
        <div className="flex gap-2">
          <button
            onClick={handleCompile}
            disabled={compiling}
            className="rounded-lg bg-stone-800 px-4 py-2 text-sm text-white hover:bg-stone-700 disabled:opacity-50"
          >
            {compiling ? 'Compiling...' : 'Compile'}
          </button>
          <button
            onClick={handleSubmit}
            disabled={submitting}
            className="rounded-lg border border-stone-800 px-4 py-2 text-sm text-stone-800 hover:bg-stone-100 disabled:opacity-50"
          >
            {submitting ? 'Submitting...' : 'Submit to Shared'}
          </button>
        </div>
      </div>

      {message && (
        <div className="rounded-lg bg-stone-100 px-4 py-3 text-sm text-stone-600">
          {message}
        </div>
      )}

      <div className="rounded-lg border border-stone-200 bg-white p-5">
        <h2 className="text-sm font-medium text-stone-500 mb-3">Ingest Source</h2>
        <IngestForm branch={BRANCH} onDone={refresh} />
      </div>

      <div>
        <h2 className="text-sm font-medium text-stone-500 mb-3">My Pages</h2>
        <PageList key={refreshKey} branch={BRANCH} />
      </div>
    </div>
  );
}
