import { useCallback, useEffect, useMemo, useState } from 'react';
import { Check, GitMerge, GitPullRequest, RefreshCw } from 'lucide-react';
import { Link } from 'react-router-dom';
import { Button } from '../components/ui/button';
import { canMerge } from './session';
import type {
  CloudClient,
  CloudPullRequest,
  CloudPullRequestDiff,
  CloudSpace,
} from './client';
import { CloudApiError } from './client';
import { mergeActionVisible } from './cloud-shell-model';
import { CloudNotice } from './CloudHome';
import { cloudSpaceRoute } from './routes';

export function CloudReviewsView({ client, space, pullRequestId }: { client: CloudClient; space: CloudSpace; pullRequestId?: string }) {
  const [pullRequests, setPullRequests] = useState<CloudPullRequest[]>([]);
  const [loading, setLoading] = useState(true);
  const [pending, setPending] = useState('');
  const [diff, setDiff] = useState<CloudPullRequestDiff | null>(null);
  const [diffLoading, setDiffLoading] = useState(false);
  const [diffError, setDiffError] = useState('');
  const [notice, setNotice] = useState<{ tone: 'error' | 'success'; message: string } | null>(null);
  const selected = useMemo(() => pullRequests.find((pullRequest) => pullRequest.id === pullRequestId) ?? null, [pullRequestId, pullRequests]);

  const loadPullRequests = useCallback(async () => {
    setLoading(true);
    try {
      setPullRequests(await client.listPullRequests(space.id));
    } catch (cause) {
      setNotice({ tone: 'error', message: cause instanceof Error ? cause.message : 'Could not load pull requests.' });
    } finally {
      setLoading(false);
    }
  }, [client, space.id]);
  useEffect(() => { void loadPullRequests(); }, [loadPullRequests]);
  useEffect(() => {
    if (!selected) {
      setDiff(null);
      setDiffError('');
      return;
    }
    let active = true;
    setDiff(null);
    setDiffError('');
    setDiffLoading(true);
    void client.getPullRequestDiff(space.id, selected.id)
      .then((value) => { if (active) setDiff(value); })
      .catch((cause) => {
        if (active) setDiffError(cause instanceof Error ? cause.message : 'Could not load this diff.');
      })
      .finally(() => { if (active) setDiffLoading(false); });
    return () => { active = false; };
  }, [client, selected?.headOid, selected?.id, space.id]);

  const approve = async (pullRequest: CloudPullRequest) => {
    setPending('approve');
    setNotice(null);
    try {
      await client.approvePullRequest(space.id, pullRequest.id);
      setNotice({ tone: 'success', message: 'Approval recorded for the current head.' });
    } catch (cause) {
      setNotice({ tone: 'error', message: cause instanceof Error ? cause.message : 'Approval failed.' });
    } finally {
      await loadPullRequests();
      setPending('');
    }
  };

  const merge = async (pullRequest: CloudPullRequest) => {
    setPending('merge');
    setNotice(null);
    const expectedHeadOid = pullRequest.headOid;
    try {
      await client.mergePullRequest(space.id, pullRequest.id, expectedHeadOid);
      setNotice({ tone: 'success', message: `Pull request #${pullRequest.number} merged into Cloud main.` });
    } catch (cause) {
      const message = cause instanceof CloudApiError && cause.status === 409
        ? 'This pull request changed. The latest head has been loaded; review it before merging.'
        : cause instanceof Error ? cause.message : 'Merge failed.';
      setNotice({ tone: 'error', message });
    } finally {
      await loadPullRequests();
      setPending('');
    }
  };

  return (
    <div className="mx-auto w-full max-w-5xl px-10 py-12">
      <div className="mb-8 flex items-end justify-between">
        <div>
          <h1 className="font-serif text-4xl font-bold">Reviews</h1>
          <p className="mt-2 text-sm text-text-tertiary">User branches remain live until a maintainer merges their current head.</p>
        </div>
        <Button variant="outline" onClick={() => void loadPullRequests()} disabled={loading}><RefreshCw className={loading ? 'animate-spin' : ''} /> Refresh</Button>
      </div>
      {notice && <CloudNotice tone={notice.tone}>{notice.message}</CloudNotice>}
      <div className="grid grid-cols-[minmax(260px,0.8fr)_minmax(0,1.4fr)] gap-5">
        <div className="overflow-hidden rounded-xl border bg-panel">
          {pullRequests.length === 0 && !loading && <p className="p-6 text-sm text-text-tertiary">No pull requests yet.</p>}
          {pullRequests.map((pullRequest) => (
            <Link key={pullRequest.id} to={cloudSpaceRoute(space.id, 'reviews', pullRequest.id)} className={`block border-b px-5 py-4 text-inherit no-underline last:border-b-0 ${selected?.id === pullRequest.id ? 'bg-accent-soft' : 'hover:bg-secondary'}`}>
              <div className="flex items-center gap-2 text-sm font-semibold"><GitPullRequest size={15} className="text-accent" />#{pullRequest.number} {pullRequest.title}</div>
              <div className="mt-1 text-xs text-text-secondary">{pullRequest.authorName} <span className="text-text-tertiary">@{pullRequest.authorHandle}</span></div>
              <div className="mt-2 flex items-center justify-between text-[11px] text-text-tertiary"><span className="font-mono">{pullRequest.headRef}</span><span className="capitalize">{pullRequest.status}</span></div>
            </Link>
          ))}
        </div>

        <section className="rounded-xl border bg-panel p-7">
          {!selected ? (
            <div className="grid min-h-56 place-items-center text-sm text-text-tertiary">Choose a pull request to review its current head.</div>
          ) : (
            <>
              <div className="mb-6 flex items-start justify-between gap-5">
                <div><div className="text-xs font-semibold text-accent">PULL REQUEST #{selected.number}</div><h2 className="mt-2 font-serif text-3xl font-bold">{selected.title}</h2></div>
                <span className="rounded-full bg-secondary px-3 py-1 text-xs font-semibold capitalize">{selected.status}</span>
              </div>
              {selected.body && <p className="mb-6 whitespace-pre-wrap text-sm leading-6 text-text-secondary">{selected.body}</p>}
              <dl className="grid grid-cols-[90px_1fr] gap-x-4 gap-y-3 border-y py-5 text-xs">
                <dt className="text-text-tertiary">Author</dt><dd>{selected.authorName} <span className="text-text-tertiary">@{selected.authorHandle}</span></dd>
                <dt className="text-text-tertiary">Branch</dt><dd className="font-mono">{selected.headRef} → {selected.baseRef}</dd>
                <dt className="text-text-tertiary">Head</dt><dd className="font-mono">{selected.headOid}</dd>
                <dt className="text-text-tertiary">Approvals</dt><dd>{selected.approvalCount}</dd>
              </dl>
              <div className="mt-6">
                <h3 className="text-sm font-semibold">Changed files</h3>
                {diffLoading && <p className="mt-3 text-xs text-text-tertiary">Loading current diff…</p>}
                {diffError && <div className="mt-3"><CloudNotice tone="error">{diffError}</CloudNotice></div>}
                {diff && (
                  <>
                    <div className="mt-3 overflow-hidden rounded-lg border">
                      {diff.files.length === 0 && <p className="p-4 text-xs text-text-tertiary">No visible Markdown changes.</p>}
                      {diff.files.map((file) => (
                        <div key={`${file.status}:${file.path}`} className="flex items-center gap-3 border-b px-4 py-2.5 text-xs last:border-b-0">
                          <span className="w-16 capitalize text-text-tertiary">{file.status}</span>
                          <span className="min-w-0 flex-1 truncate font-mono">{file.path}</span>
                          <span className="text-success">+{file.additions}</span>
                          <span className="text-destructive">−{file.deletions}</span>
                        </div>
                      ))}
                    </div>
                    {diff.patch && (
                      <pre className="mt-4 max-h-[520px] overflow-auto rounded-lg border bg-bg-secondary p-0 text-[11px] leading-5">
                        {diff.patch.split('\n').map((line, index) => (
                          <div
                            key={`${index}:${line}`}
                            className={`min-w-max px-4 ${
                              line.startsWith('+') && !line.startsWith('+++')
                                ? 'bg-green-50 text-green-800'
                                : line.startsWith('-') && !line.startsWith('---')
                                  ? 'bg-red-50 text-red-800'
                                  : line.startsWith('@@')
                                    ? 'bg-blue-50 text-blue-800'
                                    : 'text-text-secondary'
                            }`}
                          >
                            {line || ' '}
                          </div>
                        ))}
                      </pre>
                    )}
                  </>
                )}
              </div>
              {selected.status === 'open' && (
                <div className="mt-6 flex flex-wrap gap-2">
                  {canMerge(space.role) && <Button variant="outline" disabled={!!pending || !diff || diff.headOid !== selected.headOid} onClick={() => void approve(selected)}><Check /> Approve current head</Button>}
                  {mergeActionVisible(space.role) && <Button disabled={!!pending || !diff || diff.headOid !== selected.headOid} onClick={() => void merge(selected)}><GitMerge /> Merge into main</Button>}
                </div>
              )}
            </>
          )}
        </section>
      </div>
    </div>
  );
}
