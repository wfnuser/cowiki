import { useEffect, useState } from 'react';
import { Check, X, ChevronLeft, GitPullRequest } from 'lucide-react';
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
      <div>
        <button
          onClick={() => setSelected(null)}
          className="inline-flex items-center gap-1 text-sm text-[var(--color-text-tertiary)] hover:text-[var(--color-text-secondary)] mb-6 transition-colors"
        >
          <ChevronLeft size={14} /> All reviews
        </button>

        <h1
          className="text-2xl font-bold mb-1"
          style={{ fontFamily: 'var(--font-serif)' }}
        >
          Review #{selected.submission.id.slice(0, 8)}
        </h1>
        <p className="text-sm text-[var(--color-text-secondary)] mb-2">
          {selected.submission.summary}
        </p>
        <p className="text-xs text-[var(--color-text-tertiary)] mb-6">
          {selected.submission.page_slugs.length} file(s) &middot;{' '}
          {new Date(selected.submission.created_at).toLocaleString()}
        </p>

        {/* Diffs */}
        <div className="space-y-4 mb-6">
          {selected.diffs.map((diff) => (
            <div
              key={diff.path}
              className="border border-[var(--color-border)] rounded-lg overflow-hidden"
            >
              <div className="bg-[var(--color-bg-secondary)] px-4 py-2 text-xs font-mono text-[var(--color-text-secondary)] border-b border-[var(--color-border)] flex items-center gap-2">
                <span
                  className={`inline-block w-4 text-center font-bold ${
                    diff.old_content === null ? 'text-[var(--color-green)]' : 'text-[var(--color-amber)]'
                  }`}
                >
                  {diff.old_content === null ? '+' : '~'}
                </span>
                {diff.path}
              </div>
              <div className="overflow-x-auto">
                <pre className="p-4 text-xs leading-relaxed font-mono">
                  {diff.new_content?.split('\n').map((line, i) => {
                    const oldLines = diff.old_content?.split('\n') || [];
                    const isNew = diff.old_content === null;
                    const isChanged = !isNew && (i >= oldLines.length || oldLines[i] !== line);
                    return (
                      <div
                        key={i}
                        className={
                          isNew
                            ? 'text-[var(--color-green)] bg-green-50/50'
                            : isChanged
                              ? 'text-[var(--color-green)] bg-green-50/50'
                              : 'text-[var(--color-text-secondary)]'
                        }
                      >
                        <span className="inline-block w-8 text-right pr-3 text-[var(--color-text-tertiary)] select-none">
                          {i + 1}
                        </span>
                        {isNew || isChanged ? '+ ' : '  '}
                        {line}
                      </div>
                    );
                  })}
                </pre>
              </div>
            </div>
          ))}
        </div>

        {/* Actions */}
        <div className="flex gap-2 pt-4 border-t border-[var(--color-border)]">
          <button
            onClick={() => handleAction('approve')}
            disabled={actionLoading}
            className="flex items-center gap-1.5 rounded-md bg-[var(--color-green)] text-white px-4 py-2 text-sm hover:opacity-90 disabled:opacity-40 transition-opacity"
          >
            <Check size={14} />
            {actionLoading ? 'Processing...' : 'Approve'}
          </button>
          <button
            onClick={() => handleAction('reject')}
            disabled={actionLoading}
            className="flex items-center gap-1.5 rounded-md border border-[var(--color-red)] text-[var(--color-red)] px-4 py-2 text-sm hover:bg-red-50 disabled:opacity-40 transition-colors"
          >
            <X size={14} />
            Reject
          </button>
        </div>
      </div>
    );
  }

  return (
    <div>
      <h1
        className="text-4xl font-bold text-[var(--color-text)] mb-1"
        style={{ fontFamily: 'var(--font-serif)' }}
      >
        Reviews
      </h1>
      <p className="text-[var(--color-text-secondary)] text-sm mb-6">
        Submissions waiting for review before entering the shared wiki.
      </p>

      {loading ? (
        <div className="py-8 text-center text-[var(--color-text-tertiary)] text-sm">Loading...</div>
      ) : reviews.length === 0 ? (
        <div className="py-16 text-center">
          <GitPullRequest size={24} className="mx-auto text-[var(--color-text-tertiary)] mb-3" />
          <p className="text-[var(--color-text-tertiary)] text-sm">No pending reviews</p>
        </div>
      ) : (
        <div>
          {reviews.map((r) => (
            <button
              key={r.id}
              onClick={() => handleSelect(r.id)}
              className="w-full text-left flex items-start gap-3 px-2 py-3 -mx-2 rounded-md hover:bg-[var(--color-bg-hover)] transition-colors"
            >
              <GitPullRequest
                size={16}
                className="mt-0.5 shrink-0 text-[var(--color-amber)]"
              />
              <div className="min-w-0 flex-1">
                <div className="text-sm text-[var(--color-text)]">
                  {r.page_slugs.length} page(s) from {r.source_branch}
                </div>
                <div className="text-xs text-[var(--color-text-tertiary)] mt-0.5">
                  {r.summary}
                </div>
              </div>
              <span className="text-xs text-[var(--color-text-tertiary)] shrink-0">
                {new Date(r.created_at).toLocaleDateString()}
              </span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
