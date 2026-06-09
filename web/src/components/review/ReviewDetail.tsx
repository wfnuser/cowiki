import { useEffect, useState, useCallback } from 'react';
import { ArrowLeft, MessageSquare, XCircle, GitMerge, Check, AlertCircle, Plus } from 'lucide-react';
import {
  getReview, reviewAction, createComment, resolveComment, unresolveComment,
  type ReviewDetail as ReviewDetailData, type ReviewComment,
} from '../../api';
import { DiffView } from './DiffView';
import { C, fonts } from '@/lib/design';

function timeAgo(iso: string): string {
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return '';
  const secs = Math.max(0, Math.floor((Date.now() - then) / 1000));
  if (secs < 60) return `${secs}s ago`;
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs}h ago`;
  return `${Math.floor(hrs / 24)}d ago`;
}

const statusBadge: Record<string, { bg: string; fg: string; label: string }> = {
  pending: { bg: C.amberSoft, fg: C.amber, label: 'Review needed' },
  approved: { bg: C.greenSoft, fg: C.green, label: 'Approved' },
  rejected: { bg: C.accentSoft, fg: C.accent, label: 'Changes requested' },
  merged: { bg: C.blueSoft, fg: C.blue, label: 'Merged' },
};

export function ReviewDetail({
  workspaceSlug,
  submissionId,
  onBack,
  onActioned,
}: {
  workspaceSlug: string;
  submissionId: string;
  onBack: () => void;
  onActioned: () => void;
}) {
  const [data, setData] = useState<ReviewDetailData | null>(null);
  const [comments, setComments] = useState<ReviewComment[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<null | 'approve' | 'reject' | 'merge'>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [reviewBody, setReviewBody] = useState('');

  useEffect(() => {
    let cancelled = false;
    setData(null);
    setError(null);
    getReview(workspaceSlug, submissionId)
      .then((d) => {
        if (cancelled) return;
        setData(d);
        setComments(d.comments ?? []);
      })
      .catch((e) => !cancelled && setError(e.message || 'Failed to load review'));
    return () => { cancelled = true; };
  }, [workspaceSlug, submissionId]);

  const act = async (action: 'approve' | 'reject' | 'merge') => {
    setBusy(action);
    setActionError(null);
    try {
      await reviewAction(workspaceSlug, submissionId, action);
      onActioned();
      if (action === 'merge') {
        onBack();
      } else {
        // Reload to see updated status
        const d = await getReview(workspaceSlug, submissionId);
        setData(d);
        setComments(d.comments ?? []);
      }
    } catch (e) {
      setActionError((e as Error).message || `Failed to ${action}`);
    } finally {
      setBusy(null);
    }
  };

  const handleAddComment = useCallback(async (filePath: string, lineNumber: number, body: string) => {
    try {
      const c = await createComment(workspaceSlug, submissionId, body, filePath, lineNumber);
      setComments((prev) => [...prev, c]);
    } catch {
      // silently fail for now
    }
  }, [workspaceSlug, submissionId]);

  const handleResolve = useCallback(async (commentId: string) => {
    try {
      await resolveComment(workspaceSlug, submissionId, commentId);
      setComments((prev) => prev.map((c) => c.id === commentId ? { ...c, resolved: true } : c));
    } catch { /* */ }
  }, [workspaceSlug, submissionId]);

  const handleUnresolve = useCallback(async (commentId: string) => {
    try {
      await unresolveComment(workspaceSlug, submissionId, commentId);
      setComments((prev) => prev.map((c) => c.id === commentId ? { ...c, resolved: false } : c));
    } catch { /* */ }
  }, [workspaceSlug, submissionId]);

  const handleReply = useCallback(async (body: string, parentId: string) => {
    const parent = comments.find((c) => c.id === parentId);
    if (!parent) return;
    try {
      const c = await createComment(workspaceSlug, submissionId, body, parent.file_path, parent.line_number ?? undefined, parentId);
      setComments((prev) => [...prev, c]);
    } catch { /* */ }
  }, [workspaceSlug, submissionId, comments]);

  const handleSubmitReview = async () => {
    if (!reviewBody.trim()) return;
    try {
      await createComment(workspaceSlug, submissionId, reviewBody.trim(), '', undefined);
      setComments((prev) => [...prev, {
        id: crypto.randomUUID(),
        submission_id: submissionId,
        user_id: 'you',
        file_path: '',
        line_number: null,
        body: reviewBody.trim(),
        parent_id: null,
        resolved: false,
        created_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
      }]);
      setReviewBody('');
    } catch { /* */ }
  };

  if (error) {
    return (
      <div style={{ padding: 24 }}>
        <button onClick={onBack} style={backBtnStyle}><ArrowLeft size={14} /> Reviews</button>
        <p style={{ color: C.red, fontSize: 14, marginTop: 12 }}>Failed to load review: {error}</p>
      </div>
    );
  }

  if (data == null) {
    return (
      <div style={{ padding: 24 }}>
        <button onClick={onBack} style={backBtnStyle}><ArrowLeft size={14} /> Reviews</button>
        <p style={{ color: C.muted, fontSize: 14, marginTop: 12 }}>Loading review...</p>
      </div>
    );
  }

  const { submission, diffs } = data;
  const status = statusBadge[submission.status] ?? statusBadge.pending;
  const totalAdd = diffs.reduce((n, f) => n + f.additions, 0);
  const totalDel = diffs.reduce((n, f) => n + f.deletions, 0);
  const decided = submission.status !== 'pending';
  const unresolvedCount = comments.filter((c) => !c.resolved && c.line_number != null).length;

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      {/* Header */}
      <div style={{ flexShrink: 0 }}>
        <button onClick={onBack} style={backBtnStyle}>
          <ArrowLeft size={14} /> Reviews
          <span style={{ color: C.faint }}> / #{submissionId.slice(0, 6)}</span>
        </button>

        <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginTop: 8 }}>
          <h1 style={{ fontFamily: fonts.serif, fontSize: 26, fontWeight: 700, color: C.ink, margin: 0 }}>
            {submission.summary || 'Submission'}
          </h1>
          <span style={{ fontSize: 16, color: C.faint, fontWeight: 400 }}>#{submissionId.slice(0, 6)}</span>
          <span style={{
            fontSize: 12.5, fontWeight: 600, padding: '6px 12px', borderRadius: 999,
            background: status.bg, color: status.fg,
          }}>
            {status.label}
          </span>
        </div>

        <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginTop: 6, fontSize: 13, color: C.muted }}>
          <div style={{
            width: 22, height: 22, borderRadius: '50%', background: C.rail,
            display: 'flex', alignItems: 'center', justifyContent: 'center',
            fontSize: 10, fontWeight: 600, color: C.ink2,
          }}>
            {(submission.user_id ?? 'U').slice(0, 1).toUpperCase()}
          </div>
          <span style={{ fontWeight: 550, color: C.ink2 }}>{(submission.user_id ?? 'unknown').slice(0, 8)}</span>
          <span>wants to merge</span>
          <code style={{
            fontSize: 12, padding: '2px 7px', borderRadius: 6,
            background: C.rail, fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
          }}>{submission.source_branch}</code>
          <span style={{ color: C.faint }}>&rarr;</span>
          <code style={{
            fontSize: 12, padding: '2px 7px', borderRadius: 6,
            background: C.rail, fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
          }}>wiki/main</code>
          <span style={{ color: C.green }}>+{totalAdd}</span>
          <span style={{ color: C.red }}>&minus;{totalDel}</span>
          <span style={{ color: C.faint }}>&middot;</span>
          <span>{diffs.length} file{diffs.length === 1 ? '' : 's'}</span>
          <span style={{ color: C.faint }}>&middot;</span>
          <span>{timeAgo(submission.created_at)}</span>
        </div>

        {actionError && (
          <div style={{ marginTop: 8, padding: '6px 12px', background: '#fbeadd', borderRadius: 6, fontSize: 13, color: C.accent }}>
            {actionError}
          </div>
        )}
      </div>

      {/* Main content: diff + sidebar in a flex row */}
      <div style={{ display: 'flex', flex: 1, minHeight: 0, marginTop: 20, overflow: 'auto' }}>
        {/* Diff area (left) */}
        <div style={{ flex: 1, minWidth: 0, padding: '20px 28px 40px' }}>
          <DiffView
            diffs={diffs}
            comments={comments}
            onAddComment={handleAddComment}
            onResolve={handleResolve}
            onUnresolve={handleUnresolve}
            onReply={handleReply}
          />

          {/* Finish your review box */}
          <div style={{
            marginTop: 24, padding: 16, border: `1px solid ${C.line}`, borderRadius: 12, background: C.bg,
          }}>
            <div style={{ fontSize: 14.5, fontWeight: 650, color: C.ink, marginBottom: 10 }}>Finish your review</div>
            <textarea
              value={reviewBody}
              onChange={(e) => setReviewBody(e.target.value)}
              placeholder="Leave a comment on this submission..."
              rows={3}
              style={{
                width: '100%', fontSize: 13, padding: '10px 12px', border: `1px solid ${C.line}`,
                borderRadius: 8, resize: 'vertical', outline: 'none', fontFamily: 'inherit',
                background: '#fff', boxSizing: 'border-box',
              }}
            />
            <div style={{ display: 'flex', flexWrap: 'wrap', gap: 10, marginTop: 10, alignItems: 'center' }}>
              {!decided && (
                <>
                  <button
                    onClick={() => { handleSubmitReview(); act('approve'); }}
                    disabled={busy != null}
                    style={{
                      ...actionBtnStyle, background: C.green, color: '#fff',
                      opacity: busy != null ? 0.5 : 1,
                    }}
                  >
                    <Check size={14} />
                    {busy === 'approve' ? 'Approving...' : 'Approve'}
                  </button>
                  <button
                    onClick={() => { handleSubmitReview(); act('reject'); }}
                    disabled={busy != null}
                    style={{
                      ...actionBtnStyle, background: '#fbeadd', color: C.accent,
                      opacity: busy != null ? 0.5 : 1,
                    }}
                  >
                    <XCircle size={14} />
                    {busy === 'reject' ? 'Requesting...' : 'Request changes'}
                  </button>
                </>
              )}
              {reviewBody.trim() && (
                <button onClick={handleSubmitReview} style={{ ...actionBtnStyle, background: '#fff', color: C.ink2, border: `1px solid ${C.line}` }}>
                  <MessageSquare size={14} /> Comment
                </button>
              )}
              <div style={{ marginLeft: 'auto' }}>
                {submission.status === 'approved' ? (
                  <button
                    onClick={() => act('merge')}
                    disabled={busy != null}
                    style={{
                      ...actionBtnStyle, background: C.ink, color: '#fff',
                      opacity: busy != null ? 0.5 : 1,
                    }}
                  >
                    <GitMerge size={14} />
                    {busy === 'merge' ? 'Merging...' : 'Merge to wiki'}
                  </button>
                ) : (
                  <button
                    disabled
                    style={{
                      ...actionBtnStyle, background: C.rail, color: C.faint,
                      cursor: 'default',
                    }}
                  >
                    <GitMerge size={14} /> Merge to wiki
                  </button>
                )}
              </div>
            </div>
          </div>
        </div>

        {/* Right sidebar */}
        <div style={{ width: 252, flexShrink: 0, borderLeft: `1px solid ${C.line}`, padding: 20, overflowY: 'auto' }}>
          {/* Reviewers */}
          <div style={{ marginBottom: 20 }}>
            <div style={sidebarSectionTitle}>
              Reviewers
            </div>
            {submission.reviewed_by ? (
              <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                <div style={{
                  width: 26, height: 26, borderRadius: '50%', background: C.rail,
                  display: 'flex', alignItems: 'center', justifyContent: 'center',
                  fontSize: 11, fontWeight: 600, color: C.ink2,
                }}>
                  {submission.reviewed_by.slice(0, 1).toUpperCase()}
                </div>
                <span style={{ fontSize: 13.5, color: C.ink2, flex: 1 }}>{submission.reviewed_by.slice(0, 8)}</span>
                {/* Status pip */}
                <div style={{
                  width: 18, height: 18, borderRadius: '50%',
                  background: submission.status === 'approved' ? C.green
                    : submission.status === 'rejected' ? C.accent : C.faint,
                  display: 'flex', alignItems: 'center', justifyContent: 'center',
                }}>
                  {submission.status === 'approved' && <Check size={11} color="#fff" />}
                  {submission.status === 'rejected' && <XCircle size={11} color="#fff" />}
                </div>
              </div>
            ) : (
              <span style={{ fontSize: 12.5, color: C.faint }}>No reviewers yet</span>
            )}
            <button style={{
              background: 'none', border: 'none', cursor: 'pointer', padding: 0,
              display: 'flex', alignItems: 'center', gap: 4,
              fontSize: 12.5, color: C.faint, marginTop: 8,
            }}>
              <Plus size={12} /> Request a reviewer
            </button>
          </div>

          {/* Checks */}
          <div style={{ marginBottom: 20 }}>
            <div style={sidebarSectionTitle}>
              Checks
            </div>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                <div style={{
                  width: 16, height: 16, borderRadius: '50%', background: C.green,
                  display: 'flex', alignItems: 'center', justifyContent: 'center',
                }}>
                  <Check size={10} color="#fff" />
                </div>
                <span style={{ fontSize: 13, color: C.ink2 }}>Links valid</span>
              </div>
              {unresolvedCount > 0 && (
                <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                  <div style={{
                    width: 16, height: 16, borderRadius: '50%', background: C.amber,
                    display: 'flex', alignItems: 'center', justifyContent: 'center',
                  }}>
                    <AlertCircle size={10} color="#fff" />
                  </div>
                  <span style={{ fontSize: 13, color: C.ink2 }}>{unresolvedCount} unresolved</span>
                </div>
              )}
            </div>
          </div>

          {/* Labels */}
          <div>
            <div style={sidebarSectionTitle}>
              Labels
            </div>
            <div style={{ display: 'flex', flexWrap: 'wrap', gap: 4 }}>
              {submission.page_slugs.map((slug) => (
                <span key={slug} style={{
                  fontSize: 11.5, padding: '2px 8px', borderRadius: 999,
                  background: C.rail, color: C.ink2,
                }}>
                  {slug}
                </span>
              ))}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

const backBtnStyle: React.CSSProperties = {
  background: 'none', border: 'none', cursor: 'pointer',
  display: 'flex', alignItems: 'center', gap: 6,
  fontSize: 12.5, color: C.muted, padding: 0,
};

const actionBtnStyle: React.CSSProperties = {
  display: 'flex', alignItems: 'center', gap: 6,
  padding: '9px 15px', borderRadius: 8, fontSize: 13.5, fontWeight: 600,
  border: 'none', cursor: 'pointer',
};

const sidebarSectionTitle: React.CSSProperties = {
  fontSize: 11.5, fontWeight: 600, color: C.faint, textTransform: 'uppercase',
  letterSpacing: '0.06em', marginBottom: 8,
};

export default ReviewDetail;
