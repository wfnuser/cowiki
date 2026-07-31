import { useCallback, useEffect, useMemo, useState } from 'react';
import { ArrowLeft, Check, GitBranch, GitMerge } from 'lucide-react';
import { useNavigate } from 'react-router-dom';

import { DiffView } from '../components/review/DiffView';
import { ReviewInbox, ReviewInboxRow } from '../components/review/ReviewInbox';
import { AvatarBadge } from '../components/ui/avatar-badge';
import { C, fonts } from '../lib/design';
import { CloudApiError } from './client';
import type {
  CloudClient,
  CloudPullRequest,
  CloudPullRequestDiff,
  CloudSpace,
} from './client';
import { cloudDiffToFileDiffs } from './cloud-review-model';
import { mergeActionVisible } from './cloud-shell-model';
import { CloudNotice } from './CloudHome';
import { cloudSpaceRoute } from './routes';
import { canMerge } from './session';

export function CloudReviewsView({
  client,
  space,
  pullRequestId,
}: {
  client: CloudClient;
  space: CloudSpace;
  pullRequestId?: string;
}) {
  const navigate = useNavigate();
  const [pullRequests, setPullRequests] = useState<CloudPullRequest[]>([]);
  const [loading, setLoading] = useState(true);
  const [pending, setPending] = useState('');
  const [diff, setDiff] = useState<CloudPullRequestDiff | null>(null);
  const [diffError, setDiffError] = useState<{ pullRequestId: string; message: string } | null>(null);
  const [notice, setNotice] = useState<{ tone: 'error' | 'success'; message: string } | null>(null);
  const selected = useMemo(
    () => pullRequests.find((pullRequest) => pullRequest.id === pullRequestId) ?? null,
    [pullRequestId, pullRequests],
  );

  const loadPullRequests = useCallback(async () => {
    setLoading(true);
    try {
      setPullRequests(await client.listPullRequests(space.id));
    } catch (cause) {
      setNotice({
        tone: 'error',
        message: cause instanceof Error ? cause.message : 'Could not load pull requests.',
      });
    } finally {
      setLoading(false);
    }
  }, [client, space.id]);

  useEffect(() => {
    let active = true;
    void client.listPullRequests(space.id)
      .then((value) => {
        if (active) setPullRequests(value);
      })
      .catch((cause) => {
        if (active) {
          setNotice({
            tone: 'error',
            message: cause instanceof Error ? cause.message : 'Could not load pull requests.',
          });
        }
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => { active = false; };
  }, [client, space.id]);

  const selectedId = selected?.id;
  const selectedHeadOid = selected?.headOid;
  useEffect(() => {
    if (!selectedId || !selectedHeadOid) return;
    let active = true;
    void client.getPullRequestDiff(space.id, selectedId)
      .then((value) => {
        if (active) setDiff(value);
      })
      .catch((cause) => {
        if (active) {
          setDiffError({
            pullRequestId: selectedId,
            message: cause instanceof Error ? cause.message : 'Could not load this diff.',
          });
        }
      });
    return () => { active = false; };
  }, [client, selectedHeadOid, selectedId, space.id]);

  const currentDiff = selected && diff?.headOid === selected.headOid ? diff : null;
  const currentDiffError = selected && diffError?.pullRequestId === selected.id
    ? diffError.message
    : '';

  const approve = async (pullRequest: CloudPullRequest) => {
    setPending('approve');
    setNotice(null);
    try {
      await client.approvePullRequest(space.id, pullRequest.id);
      setNotice({ tone: 'success', message: 'Approval recorded for the current head.' });
    } catch (cause) {
      setNotice({
        tone: 'error',
        message: cause instanceof Error ? cause.message : 'Approval failed.',
      });
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
      setNotice({
        tone: 'success',
        message: `Pull request #${pullRequest.number} merged into Cloud main.`,
      });
    } catch (cause) {
      const message = cause instanceof CloudApiError && cause.status === 409
        ? 'This pull request changed. Review the latest head before merging.'
        : cause instanceof Error ? cause.message : 'Merge failed.';
      setNotice({ tone: 'error', message });
    } finally {
      await loadPullRequests();
      setPending('');
    }
  };

  return (
    <div className="h-full overflow-y-auto">
      <div style={{ padding: '36px 56px 56px' }}>
        {notice && <div style={{ marginBottom: 18 }}><CloudNotice tone={notice.tone}>{notice.message}</CloudNotice></div>}
        {pullRequestId ? (
          <CloudReviewDetail
            diff={currentDiff}
            diffError={currentDiffError}
            loading={loading}
            pending={pending}
            pullRequest={selected}
            canApprove={canMerge(space.role)}
            canMergePullRequest={mergeActionVisible(space.role)}
            onApprove={approve}
            onBack={() => navigate(cloudSpaceRoute(space.id, 'reviews'))}
            onMerge={merge}
          />
        ) : (
          <ReviewInbox
            description="Review submitted changes before they become Cloud main."
            empty="No pull requests yet."
            loading={loading}
          >
            {!loading && pullRequests.length
              ? pullRequests.map((pullRequest, index) => (
                <ReviewInboxRow
                  key={pullRequest.id}
                  first={index === 0}
                  icon={<GitBranch size={17} color={C.faint} />}
                  title={`#${pullRequest.number} ${pullRequest.title}`}
                  subtitle={`${pullRequest.authorName} · ${pullRequest.headRef} → ${pullRequest.baseRef}`}
                  status={statusLabel(pullRequest.status)}
                  onOpen={() => navigate(cloudSpaceRoute(space.id, 'reviews', pullRequest.id))}
                  trailing={<AvatarBadge name={pullRequest.authorName} size={26} />}
                />
              ))
              : undefined}
          </ReviewInbox>
        )}
      </div>
    </div>
  );
}

function CloudReviewDetail({
  canApprove,
  canMergePullRequest,
  diff,
  diffError,
  loading,
  onApprove,
  onBack,
  onMerge,
  pending,
  pullRequest,
}: {
  canApprove: boolean;
  canMergePullRequest: boolean;
  diff: CloudPullRequestDiff | null;
  diffError: string;
  loading: boolean;
  onApprove: (pullRequest: CloudPullRequest) => Promise<void>;
  onBack: () => void;
  onMerge: (pullRequest: CloudPullRequest) => Promise<void>;
  pending: string;
  pullRequest: CloudPullRequest | null;
}) {
  if (loading && !pullRequest) {
    return <p style={{ color: C.muted, fontSize: 14 }}>Loading Review…</p>;
  }
  if (!pullRequest) {
    return <CloudNotice tone="error">This pull request could not be found.</CloudNotice>;
  }

  const diffs = diff ? cloudDiffToFileDiffs(diff) : null;
  return (
    <div>
      <button type="button" onClick={onBack} style={backButtonStyle}>
        <ArrowLeft size={14} /> Reviews
      </button>

      <div style={{
        display: 'flex',
        alignItems: 'flex-start',
        justifyContent: 'space-between',
        gap: 16,
        marginTop: 10,
      }}>
        <div style={{ minWidth: 0 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 9 }}>
            <GitBranch size={19} color={C.faint} />
            <h1 style={{ margin: 0, color: C.ink, fontFamily: fonts.serif, fontSize: 28 }}>
              {pullRequest.title}
            </h1>
          </div>
          <p style={{ margin: '6px 0 0 28px', color: C.muted, fontSize: 13 }}>
            {pullRequest.authorName} · {pullRequest.headRef} → {pullRequest.baseRef}
          </p>
          <span style={statusPillStyle}>{statusLabel(pullRequest.status)}</span>
        </div>

        {pullRequest.status === 'open' && (
          <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap', justifyContent: 'flex-end' }}>
            {canApprove && (
              <ActionButton
                subtle
                disabled={Boolean(pending) || !diff}
                onClick={() => void onApprove(pullRequest)}
              >
                <Check size={14} /> {pending === 'approve' ? 'Approving…' : 'Approve'}
              </ActionButton>
            )}
            {canMergePullRequest && (
              <ActionButton
                disabled={Boolean(pending) || !diff}
                onClick={() => void onMerge(pullRequest)}
              >
                <GitMerge size={14} /> {pending === 'merge' ? 'Merging…' : 'Merge into main'}
              </ActionButton>
            )}
          </div>
        )}
      </div>

      {pullRequest.body && (
        <p style={{ margin: '18px 0 0 28px', color: C.ink2, fontSize: 13.5, lineHeight: 1.6 }}>
          {pullRequest.body}
        </p>
      )}
      {diffError && <p style={{ color: C.red, fontSize: 13, marginTop: 18 }}>{diffError}</p>}
      {!diffs && !diffError ? (
        <p style={{ color: C.muted, fontSize: 14, marginTop: 24 }}>Loading changes…</p>
      ) : diffs ? (
        <div style={{ marginTop: 24 }}>
          <DiffView diffs={diffs} />
        </div>
      ) : null}
    </div>
  );
}

function ActionButton({
  children,
  disabled,
  onClick,
  subtle = false,
}: {
  children: React.ReactNode;
  disabled?: boolean;
  onClick: () => void;
  subtle?: boolean;
}) {
  return (
    <button
      type="button"
      disabled={disabled}
      onClick={onClick}
      style={{
        display: 'inline-flex',
        alignItems: 'center',
        gap: 6,
        border: subtle ? `1px solid ${C.line}` : 'none',
        borderRadius: 7,
        padding: '8px 11px',
        background: subtle ? C.panel : C.ink,
        color: subtle ? C.muted : '#fff',
        cursor: disabled ? 'default' : 'pointer',
        fontSize: 12.5,
        fontWeight: 650,
        opacity: disabled ? 0.55 : 1,
        whiteSpace: 'nowrap',
      }}
    >
      {children}
    </button>
  );
}

const backButtonStyle: React.CSSProperties = {
  display: 'inline-flex',
  alignItems: 'center',
  gap: 6,
  padding: 0,
  border: 0,
  background: 'transparent',
  color: C.muted,
  cursor: 'pointer',
  fontSize: 13,
};

const statusPillStyle: React.CSSProperties = {
  display: 'inline-block',
  margin: '8px 0 0 28px',
  padding: '3px 7px',
  borderRadius: 999,
  background: C.panel,
  color: C.muted,
  fontSize: 11.5,
  fontWeight: 650,
};

function statusLabel(status: CloudPullRequest['status']): string {
  if (status === 'merged') return 'Merged';
  if (status === 'closed') return 'Closed';
  return 'Open';
}
