import { useEffect, useState } from 'react';
import { listReviews, getReview, reviewAction, type Submission, type ReviewDetail } from '../api';

export function ReviewPage() {
  const [reviews, setReviews] = useState<Submission[]>([]);
  const [selected, setSelected] = useState<ReviewDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [actionLoading, setActionLoading] = useState(false);

  const load = () => {
    setLoading(true);
    listReviews().then(setReviews).finally(() => setLoading(false));
  };

  useEffect(() => { load(); }, []);

  const handleSelect = async (id: string) => {
    const detail = await getReview(id);
    setSelected(detail);
  };

  const handleAction = async (action: string) => {
    if (!selected) return;
    setActionLoading(true);
    try {
      await reviewAction(selected.submission.id, action);
      setSelected(null);
      load();
    } finally {
      setActionLoading(false);
    }
  };

  if (selected) {
    return (
      <div className="space-y-6">
        <button
          onClick={() => setSelected(null)}
          className="text-sm text-stone-400 hover:text-stone-600"
        >
          &larr; Back to reviews
        </button>

        <div className="bg-white rounded-lg border border-stone-200 p-6">
          <h2 className="text-xl font-bold text-stone-800 mb-2">
            Submission #{selected.submission.id.slice(0, 8)}
          </h2>
          <p className="text-stone-500 text-sm mb-4">{selected.submission.summary}</p>
          <p className="text-xs text-stone-400 mb-6">
            Pages: {selected.submission.page_slugs.join(', ')} |
            Branch: {selected.submission.source_branch} |
            {new Date(selected.submission.created_at).toLocaleString()}
          </p>

          {/* Diffs */}
          <div className="space-y-4">
            {selected.diffs.map((diff) => (
              <div key={diff.path} className="border border-stone-200 rounded-lg overflow-hidden">
                <div className="bg-stone-50 px-4 py-2 text-sm font-mono text-stone-600 border-b border-stone-200">
                  {diff.old_content === null ? '+ ' : '~ '}
                  {diff.path}
                </div>
                <pre className="p-4 text-xs overflow-x-auto bg-white">
                  {diff.old_content === null ? (
                    <code className="text-green-700">
                      {diff.new_content?.split('\n').map((l, i) => (
                        <div key={i}>+ {l}</div>
                      ))}
                    </code>
                  ) : (
                    <code>
                      {diff.new_content?.split('\n').map((l, i) => {
                        const oldLines = diff.old_content?.split('\n') || [];
                        const isChanged = i >= oldLines.length || oldLines[i] !== l;
                        return (
                          <div key={i} className={isChanged ? 'text-green-700 bg-green-50' : ''}>
                            {isChanged ? '+ ' : '  '}{l}
                          </div>
                        );
                      })}
                    </code>
                  )}
                </pre>
              </div>
            ))}
          </div>

          {/* Actions */}
          <div className="flex gap-3 mt-6 pt-4 border-t border-stone-200">
            <button
              onClick={() => handleAction('approve')}
              disabled={actionLoading}
              className="rounded-lg bg-green-600 px-5 py-2 text-sm text-white hover:bg-green-700 disabled:opacity-50"
            >
              {actionLoading ? 'Processing...' : 'Approve'}
            </button>
            <button
              onClick={() => handleAction('reject')}
              disabled={actionLoading}
              className="rounded-lg bg-red-500 px-5 py-2 text-sm text-white hover:bg-red-600 disabled:opacity-50"
            >
              Reject
            </button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-bold text-stone-800">Pending Reviews</h1>
      {loading ? (
        <div className="text-center py-12 text-stone-400">Loading...</div>
      ) : reviews.length === 0 ? (
        <div className="text-center py-16 text-stone-400">
          <p className="text-lg">No pending reviews</p>
        </div>
      ) : (
        <div className="space-y-2">
          {reviews.map((r) => (
            <button
              key={r.id}
              onClick={() => handleSelect(r.id)}
              className="w-full text-left rounded-lg border border-stone-200 bg-white p-4 hover:border-stone-300 hover:shadow-sm transition"
            >
              <div className="flex justify-between items-start">
                <div>
                  <h3 className="font-medium text-stone-800">
                    #{r.id.slice(0, 8)} — {r.page_slugs.length} page(s)
                  </h3>
                  <p className="text-sm text-stone-500 mt-1">{r.summary}</p>
                </div>
                <span className="text-xs px-2 py-1 rounded bg-amber-100 text-amber-700">
                  {r.status}
                </span>
              </div>
              <p className="text-xs text-stone-400 mt-2">
                {new Date(r.created_at).toLocaleString()}
              </p>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
