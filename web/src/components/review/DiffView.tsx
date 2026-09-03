import { useState, useCallback } from 'react';
import { MessageSquare, Check, ChevronDown, ChevronRight, Send, X, FileText, Pencil } from 'lucide-react';
import type { FileDiff, DiffHunk, DiffLine, ReviewComment } from '../../api';
import { C, fonts } from '@/lib/design';
import { timeAgo } from '../../lib/time';

const mono = fonts.mono;

function fileStatus(f: FileDiff): 'added' | 'deleted' | 'modified' {
  const hasOldContent = f.old_content != null || f.old_binary_hash != null;
  const hasNewContent = f.new_content != null || f.new_binary_hash != null;
  if (!hasOldContent) return 'added';
  if (!hasNewContent) return 'deleted';
  return 'modified';
}

const statusColors: Record<string, { bg: string; fg: string }> = {
  added: { bg: C.greenBg, fg: C.green },
  deleted: { bg: C.redBg, fg: C.red },
  modified: { bg: C.amberSoft, fg: C.amber },
};

/* ── Inline comment thread ── */
function CommentThread({
  comments,
  onResolve,
  onUnresolve,
  onReply,
}: {
  comments: ReviewComment[];
  onResolve?: (id: string) => void;
  onUnresolve?: (id: string) => void;
  onReply?: (body: string, parentId: string) => void;
}) {
  const [replyTo, setReplyTo] = useState<string | null>(null);
  const [replyBody, setReplyBody] = useState('');

  const handleReply = (parentId: string) => {
    if (!replyBody.trim() || !onReply) return;
    onReply(replyBody.trim(), parentId);
    setReplyBody('');
    setReplyTo(null);
  };

  return (
    <div style={{ background: C.diffMute, borderTop: `1px solid ${C.line}`, borderBottom: `1px solid ${C.line}`, padding: '8px 16px 8px 80px' }}>
      {comments.map((c) => (
        <div key={c.id} style={{ marginBottom: 8, padding: '8px 12px', background: C.panel, borderRadius: 6, border: `1px solid ${C.line}` }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 4 }}>
            <div style={{
              width: 20, height: 20, borderRadius: '50%', background: C.rail,
              display: 'flex', alignItems: 'center', justifyContent: 'center',
              fontSize: 10, fontWeight: 600, color: C.ink2,
            }}>
              {c.user_id.slice(0, 1).toUpperCase()}
            </div>
            <span style={{ fontSize: 12, fontWeight: 600, color: C.ink }}>{c.user_id.slice(0, 8)}</span>
            <span style={{ fontSize: 11, color: C.muted }}>{timeAgo(c.created_at)}</span>
            {c.resolved && <span style={{ fontSize: 10, color: C.green, fontWeight: 500 }}>Resolved</span>}
          </div>
          <div style={{ fontSize: 13, color: C.ink2, lineHeight: 1.5, whiteSpace: 'pre-wrap' }}>{c.body}</div>
          <div style={{ display: 'flex', gap: 8, marginTop: 6 }}>
            {!c.resolved && onResolve && (
              <button onClick={() => onResolve(c.id)} style={smallBtn}>
                <Check size={11} /> Resolve
              </button>
            )}
            {c.resolved && onUnresolve && (
              <button onClick={() => onUnresolve(c.id)} style={smallBtn}>Unresolve</button>
            )}
            {onReply && (
              <button onClick={() => setReplyTo(replyTo === c.id ? null : c.id)} style={smallBtn}>Reply</button>
            )}
          </div>
          {replyTo === c.id && (
            <div style={{ marginTop: 8, display: 'flex', gap: 6 }}>
              <input
                value={replyBody}
                onChange={(e) => setReplyBody(e.target.value)}
                placeholder="Reply..."
                onKeyDown={(e) => e.key === 'Enter' && handleReply(c.id)}
                style={{ flex: 1, fontSize: 12, padding: '4px 8px', border: `1px solid ${C.line}`, borderRadius: 4, outline: 'none', background: C.panel }}
              />
              <button onClick={() => handleReply(c.id)} style={{ ...smallBtn, color: C.accent }}>
                <Send size={11} />
              </button>
            </div>
          )}
        </div>
      ))}
    </div>
  );
}

const smallBtn: React.CSSProperties = {
  background: 'none', border: 'none', cursor: 'pointer',
  fontSize: 11, color: C.muted, display: 'flex', alignItems: 'center', gap: 3,
  padding: '2px 4px', borderRadius: 3,
};

/* ── New comment composer (appears when clicking line gutter) ── */
function CommentComposer({
  onSubmit,
  onCancel,
}: {
  onSubmit: (body: string) => void;
  onCancel: () => void;
}) {
  const [body, setBody] = useState('');

  return (
    <tr>
      <td colSpan={4} style={{ padding: '8px 16px 8px 80px', background: C.diffMute, borderBottom: `1px solid ${C.line}` }}>
        <div style={{ background: C.panel, border: `1px solid ${C.line}`, borderRadius: 6, padding: 10 }}>
          <textarea
            value={body}
            onChange={(e) => setBody(e.target.value)}
            placeholder="Write a comment..."
            autoFocus
            rows={3}
            style={{
              width: '100%', fontSize: 13, fontFamily: 'inherit', padding: 8,
              border: `1px solid ${C.line}`, borderRadius: 4, resize: 'vertical',
              outline: 'none', background: C.panel,
            }}
          />
          <div style={{ display: 'flex', gap: 6, justifyContent: 'flex-end', marginTop: 6 }}>
            <button onClick={onCancel} style={{ ...smallBtn, fontSize: 12, padding: '4px 10px', border: `1px solid ${C.line}`, borderRadius: 4 }}>
              <X size={12} /> Cancel
            </button>
            <button
              onClick={() => body.trim() && onSubmit(body.trim())}
              disabled={!body.trim()}
              style={{
                fontSize: 12, padding: '4px 12px', borderRadius: 4, border: 'none',
                background: C.accent, color: C.onAccent, cursor: body.trim() ? 'pointer' : 'default',
                opacity: body.trim() ? 1 : 0.5, display: 'flex', alignItems: 'center', gap: 4,
              }}
            >
              <MessageSquare size={12} /> Comment
            </button>
          </div>
        </div>
      </td>
    </tr>
  );
}

/* ── Hunk rendering ── */
function HunkView({
  hunk,
  filePath,
  comments,
  composingLine,
  onStartComment,
  onCancelComment,
  onSubmitComment,
  onResolve,
  onUnresolve,
  onReply,
}: {
  hunk: DiffHunk;
  filePath: string;
  comments: ReviewComment[];
  composingLine: number | null;
  onStartComment: (line: number) => void;
  onCancelComment: () => void;
  onSubmitComment: (body: string, line: number) => void;
  onResolve?: (id: string) => void;
  onUnresolve?: (id: string) => void;
  onReply?: (body: string, parentId: string) => void;
}) {
  const lineComments = (line: DiffLine): ReviewComment[] => {
    const ln = line.kind === 'del' ? line.old_line : line.new_line;
    if (ln == null) return [];
    return comments.filter((c) => c.file_path === filePath && c.line_number === ln);
  };

  return (
    <>
      {/* Hunk header */}
      <tr>
        <td colSpan={4} style={{
          background: C.blueSoft, color: C.muted, fontSize: 12, fontFamily: mono,
          padding: '4px 12px', borderBottom: `1px solid ${C.line}`,
        }}>
          {hunk.header}
        </td>
      </tr>
      {hunk.lines.map((line, i) => {
        const lc = lineComments(line);
        const ln = line.kind === 'del' ? line.old_line : line.new_line;
        const bgColor = line.kind === 'add' ? C.greenBgSoft : line.kind === 'del' ? C.redBgSoft : 'transparent';
        const gutterBg = line.kind === 'add' ? C.greenBg : line.kind === 'del' ? C.redBg : 'transparent';
        const prefix = line.kind === 'add' ? '+' : line.kind === 'del' ? '-' : ' ';

        return (
          <TableFragment key={`${hunk.header}-${i}`}>
            <tr style={{ background: bgColor }}>
              {/* Old line number */}
              <td
                onClick={() => ln != null && onStartComment(ln)}
                style={{
                  ...gutterStyle, background: gutterBg, cursor: 'pointer',
                  borderRight: `1px solid ${C.line}`,
                }}
                title="Click to comment"
              >
                {line.old_line ?? ''}
              </td>
              {/* New line number */}
              <td
                onClick={() => ln != null && onStartComment(ln)}
                style={{
                  ...gutterStyle, background: gutterBg, cursor: 'pointer',
                  borderRight: `1px solid ${C.line}`,
                }}
                title="Click to comment"
              >
                {line.new_line ?? ''}
              </td>
              {/* Prefix (+/-/space) */}
              <td style={{
                width: 18, textAlign: 'center', fontFamily: mono, fontSize: 13,
                color: line.kind === 'add' ? C.green : line.kind === 'del' ? C.red : C.muted,
                padding: 0, userSelect: 'none', lineHeight: '22px',
              }}>
                {prefix}
              </td>
              {/* Code content */}
              <td style={{
                fontFamily: mono, fontSize: 13, lineHeight: '22px',
                padding: '0 12px', whiteSpace: 'pre-wrap', wordBreak: 'break-all',
              }}>
                {renderLineText(line.text)}
              </td>
            </tr>
            {/* Inline comments */}
            {lc.length > 0 && (
              <tr>
                <td colSpan={4}>
                  <CommentThread comments={lc} onResolve={onResolve} onUnresolve={onUnresolve} onReply={onReply} />
                </td>
              </tr>
            )}
            {/* Comment composer */}
            {composingLine === ln && ln != null && (
              <CommentComposer
                onSubmit={(body) => onSubmitComment(body, ln)}
                onCancel={onCancelComment}
              />
            )}
          </TableFragment>
        );
      })}
    </>
  );
}

/** Fragment wrapper for table rows */
function TableFragment({ children }: { children: React.ReactNode }) {
  return <>{children}</>;
}

const gutterStyle: React.CSSProperties = {
  width: 46, minWidth: 46, textAlign: 'right', padding: '0 8px',
  fontFamily: mono, fontSize: 12, color: C.faint, userSelect: 'none',
  lineHeight: '22px', verticalAlign: 'top',
};

/** Render bold in markdown text */
function renderLineText(text: string): React.ReactNode {
  const parts = text.split(/(\*\*[^*]+\*\*)/g);
  return parts.map((part, i) => {
    if (part.startsWith('**') && part.endsWith('**')) {
      return <strong key={i}>{part.slice(2, -2)}</strong>;
    }
    return part;
  });
}

/* ── FileDiffCard ── */
function FileDiffCard({
  file,
  comments,
  onAddComment,
  onResolve,
  onUnresolve,
  onReply,
  onEditSave,
}: {
  file: FileDiff;
  comments: ReviewComment[];
  onAddComment?: (filePath: string, lineNumber: number, body: string) => void;
  onResolve?: (id: string) => void;
  onUnresolve?: (id: string) => void;
  onReply?: (body: string, parentId: string) => void;
  onEditSave?: (path: string, content: string) => Promise<void>;
}) {
  const [collapsed, setCollapsed] = useState(false);
  const [composingLine, setComposingLine] = useState<number | null>(null);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState('');
  const [saving, setSaving] = useState(false);
  const [editError, setEditError] = useState<string | null>(null);
  const status = fileStatus(file);
  const sc = statusColors[status];
  const fileComments = comments.filter((c) => c.file_path === file.path);

  const handleSubmitComment = useCallback((body: string, lineNumber: number) => {
    onAddComment?.(file.path, lineNumber, body);
    setComposingLine(null);
  }, [file.path, onAddComment]);

  const startEdit = () => {
    setDraft(file.new_content ?? '');
    setEditError(null);
    setEditing(true);
    setCollapsed(false);
  };

  const saveEdit = async () => {
    if (!onEditSave || saving) return;
    setSaving(true);
    setEditError(null);
    try {
      await onEditSave(file.path, draft);
      setEditing(false);
    } catch (e) {
      setEditError(e instanceof Error ? e.message : 'Failed to save');
    } finally {
      setSaving(false);
    }
  };

  return (
    <div style={{ border: `1px solid ${C.line}`, borderRadius: 12, overflow: 'hidden', background: C.panel }}>
      {/* File header */}
      <div
        onClick={() => setCollapsed(!collapsed)}
        style={{
          display: 'flex', alignItems: 'center', gap: 8,
          padding: '10px 14px', background: C.bg,
          borderBottom: collapsed ? 'none' : `1px solid ${C.line}`,
          cursor: 'pointer', userSelect: 'none',
        }}
      >
        {collapsed ? <ChevronRight size={14} color={C.muted} /> : <ChevronDown size={14} color={C.muted} />}
        <FileText size={14} color={C.muted} />
        <span style={{ fontFamily: mono, fontSize: 13, fontWeight: 550, color: C.ink }}>{file.path}</span>
        <span style={{
          fontSize: 11, padding: '1px 6px', borderRadius: 4, fontWeight: 600, letterSpacing: '0.03em',
          textTransform: 'uppercase', background: sc.bg, color: sc.fg,
        }}>
          {status}
        </span>
        <span style={{ marginLeft: 'auto', fontSize: 12, color: C.muted }}>
          <span style={{ color: C.green }}>+{file.additions}</span>
          {' '}
          <span style={{ color: C.red }}>-{file.deletions}</span>
        </span>
        {onEditSave && file.new_content != null && !editing && (
          <button
            onClick={(e) => { e.stopPropagation(); startEdit(); }}
            title="Edit this file in the review (amends the submission snapshot)"
            style={{
              display: 'flex', alignItems: 'center', gap: 4, padding: '3px 9px', borderRadius: 6,
              border: `1px solid ${C.line}`, background: C.panel, color: C.ink2,
              fontSize: 12, fontWeight: 550, cursor: 'pointer',
            }}
          >
            <Pencil size={12} /> Edit
          </button>
        )}
      </div>

      {/* Review-fix editor (edits the pr snapshot; S stays frozen, R is amended) */}
      {editing && (
        <div style={{ borderBottom: `1px solid ${C.line}` }}>
          <textarea
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            spellCheck={false}
            autoFocus
            style={{
              display: 'block', width: '100%', minHeight: 280, padding: '12px 16px',
              border: 'none', outline: 'none', resize: 'vertical', boxSizing: 'border-box',
              fontFamily: mono, fontSize: 13, lineHeight: 1.6, color: C.ink, background: C.panel,
            }}
          />
          <div style={{
            display: 'flex', alignItems: 'center', gap: 8, padding: '8px 12px',
            background: C.bg, borderTop: `1px solid ${C.lineSoft}`,
          }}>
            <span style={{ fontSize: 12, color: C.faint }}>
              Saves as a review fix on this submission — the diff below updates to include it.
            </span>
            {editError && <span style={{ fontSize: 12, color: C.red }}>{editError}</span>}
            <button
              onClick={() => setEditing(false)}
              disabled={saving}
              style={{
                marginLeft: 'auto', padding: '5px 12px', borderRadius: 6, fontSize: 12.5, fontWeight: 550,
                border: `1px solid ${C.line}`, background: C.panel, color: C.ink2, cursor: 'pointer',
              }}
            >
              Cancel
            </button>
            <button
              onClick={saveEdit}
              disabled={saving || draft === (file.new_content ?? '')}
              style={{
                padding: '5px 14px', borderRadius: 6, fontSize: 12.5, fontWeight: 600,
                border: 'none', background: draft !== (file.new_content ?? '') ? C.accent : C.faint,
                color: C.onAccent, cursor: 'pointer', opacity: saving ? 0.6 : 1,
              }}
            >
              {saving ? 'Saving…' : 'Save review fix'}
            </button>
          </div>
        </div>
      )}
      {/* Diff body */}
      {!collapsed && (
        <div style={{ overflowX: 'auto' }}>
          <table style={{ width: '100%', borderCollapse: 'collapse', tableLayout: 'fixed' }}>
            <colgroup>
              <col style={{ width: 46 }} />
              <col style={{ width: 46 }} />
              <col style={{ width: 18 }} />
              <col />
            </colgroup>
            <tbody>
              {file.hunks.length === 0 ? (
                <tr>
                  <td colSpan={4} style={{ padding: 16, textAlign: 'center', color: C.muted, fontSize: 13 }}>
                    No changes in this file
                  </td>
                </tr>
              ) : (
                file.hunks.map((hunk, hi) => (
                  <HunkView
                    key={hi}
                    hunk={hunk}
                    filePath={file.path}
                    comments={fileComments}
                    composingLine={composingLine}
                    onStartComment={(ln) => setComposingLine(composingLine === ln ? null : ln)}
                    onCancelComment={() => setComposingLine(null)}
                    onSubmitComment={handleSubmitComment}
                    onResolve={onResolve}
                    onUnresolve={onUnresolve}
                    onReply={onReply}
                  />
                ))
              )}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

/* ── Exports ── */
export function DiffView({
  diffs,
  comments = [],
  onAddComment,
  onResolve,
  onUnresolve,
  onReply,
  onEditSave,
}: {
  diffs: FileDiff[];
  comments?: ReviewComment[];
  onAddComment?: (filePath: string, lineNumber: number, body: string) => void;
  onResolve?: (id: string) => void;
  onUnresolve?: (id: string) => void;
  onReply?: (body: string, parentId: string) => void;
  onEditSave?: (path: string, content: string) => Promise<void>;
}) {
  if (!diffs.length) {
    return <p style={{ color: C.muted, fontSize: 14 }}>No file changes.</p>;
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      {diffs.map((f) => (
        <FileDiffCard
          key={f.path}
          file={f}
          comments={comments}
          onAddComment={onAddComment}
          onResolve={onResolve}
          onUnresolve={onUnresolve}
          onReply={onReply}
          onEditSave={onEditSave}
        />
      ))}
    </div>
  );
}

export default DiffView;
