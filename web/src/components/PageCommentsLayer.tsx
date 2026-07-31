import {
  createContext, createElement, useCallback, useContext, useEffect, useMemo, useRef, useState,
} from 'react';
import ReactMarkdown, { type Components } from 'react-markdown';
import remarkGfm from 'remark-gfm';
import {
  MessageSquare, Reply, Check, RotateCcw, AtSign, ChevronsRight,
  ChevronRight, ChevronDown, CheckCircle2, Trash2, MoreHorizontal,
} from 'lucide-react';
import {
  listPageComments, createPageComment, setPageCommentResolved, deletePageComment,
  listMembers, type PageComment, type MemberInfo,
} from '@/api';
import { buildSnapshotMaps, mapWithSurviving, type MappedAnchor } from '@/lib/comment-anchor';
import { C, fonts } from '@/lib/design';

// ── identity helpers ───────────────────────────────────────────────────────
const AVATAR_COLORS = ['#2f6bb0', '#2f8a5b', '#8b5cf6', '#c2410c', '#3f6c8c', '#9a6f93', '#5d8a6c', '#b5790f'];
function colorFor(id: string): string {
  let h = 0;
  for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) >>> 0;
  return AVATAR_COLORS[h % AVATAR_COLORS.length];
}
function initials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return '?';
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
  return (parts[0][0] + parts[1][0]).toUpperCase();
}
function relTime(iso: string): string {
  const s = Math.max(0, (Date.now() - new Date(iso).getTime()) / 1000);
  if (s < 60) return 'now';
  if (s < 3600) return `${Math.floor(s / 60)}m`;
  if (s < 86400) return `${Math.floor(s / 3600)}h`;
  return `${Math.floor(s / 86400)}d`;
}
function quoteOf(source: string, start: number, end: number): string {
  const text = source
    .split('\n')
    .slice(start - 1, end)
    .map((l) => l.replace(/^\s*(#{1,6}\s+|[-*+]\s+|>\s?|\d+\.\s+)/, '').trim())
    .filter(Boolean)
    .join(' ');
  return text.length > 140 ? text.slice(0, 140) + '…' : text;
}

function Avatar({ id, name, size = 24 }: { id: string; name: string; size?: number }) {
  return (
    <div style={{
      width: size, height: size, flexShrink: 0, borderRadius: '50%', background: colorFor(id),
      color: '#fff', display: 'flex', alignItems: 'center', justifyContent: 'center',
      fontSize: size * 0.42, fontWeight: 600, letterSpacing: '-0.02em',
    }}>
      {initials(name)}
    </div>
  );
}

const ghostBtn: React.CSSProperties = {
  display: 'inline-flex', alignItems: 'center', gap: 5, padding: '5px 8px', borderRadius: 7,
  border: 'none', background: 'transparent', color: C.muted, fontSize: 12.5, fontWeight: 550, cursor: 'pointer',
};

// Render a quoted snippet as *inline* markdown (bold/italic/links/code), so it
// reads like the doc — but flatten all block wrappers, and show an image's alt
// text instead of the image itself.
const QUOTE_COMPONENTS: Components = {
  p: ({ children }) => <>{children}</>,
  a: ({ children }) => <>{children}</>,
  img: ({ alt }) => <>{alt || '🖼'}</>,
  h1: ({ children }) => <>{children}</>, h2: ({ children }) => <>{children}</>,
  h3: ({ children }) => <>{children}</>, h4: ({ children }) => <>{children}</>,
  h5: ({ children }) => <>{children}</>, h6: ({ children }) => <>{children}</>,
  ul: ({ children }) => <>{children}</>, ol: ({ children }) => <>{children}</>,
  li: ({ children }) => <>{children} </>,
  blockquote: ({ children }) => <>{children}</>,
  pre: ({ children }) => <>{children}</>,
  br: () => <> </>,
} as Components;
function Quote({ text }: { text: string }) {
  return <ReactMarkdown remarkPlugins={[remarkGfm]} components={QUOTE_COMPONENTS}>{text}</ReactMarkdown>;
}

interface Thread {
  root: PageComment;
  replies: PageComment[];
  anchor: MappedAnchor;
  quote: string;
}

function selectionLineRange(root: HTMLElement): { start: number; end: number } | null {
  const sel = window.getSelection();
  if (!sel || sel.isCollapsed || sel.rangeCount === 0) return null;
  const range = sel.getRangeAt(0);
  if (!root.contains(range.commonAncestorContainer)) return null;
  const blockOf = (n: Node): HTMLElement | null => {
    let el: Node | null = n.nodeType === Node.TEXT_NODE ? n.parentElement : n;
    while (el && el !== root) {
      if (el instanceof HTMLElement && el.dataset.sourceLine) return el;
      el = el.parentElement;
    }
    return null;
  };
  const a = blockOf(range.startContainer);
  const b = blockOf(range.endContainer);
  if (!a || !b) return null;
  const start = Math.min(Number(a.dataset.sourceLine), Number(b.dataset.sourceLine));
  const end = Math.max(
    Number(a.dataset.endLine ?? a.dataset.sourceLine),
    Number(b.dataset.endLine ?? b.dataset.sourceLine),
  );
  return { start, end };
}

// ── shared controller (doc highlights + panel read the same state) ──────────
interface CommentsCtx {
  /** True only on a page view with a workspace — drives the header toggle. */
  enabled: boolean;
  anchored: Thread[]; outdated: Thread[]; resolved: Thread[]; openCount: number;
  activeId: string | null; setActive: (id: string) => void;
  panelOpen: boolean; setPanelOpen: (b: boolean) => void;
  composing: { start: number; end: number; quote: string } | null;
  cancelCompose: () => void; submitNew: (body: string) => Promise<void>;
  members: MemberInfo[]; nameOf: (id: string) => string;
  currentUserId?: string; myId: string; myName: string;
  onResolve: (id: string, r: boolean) => Promise<void>; onDelete: (id: string) => Promise<void>;
  submitReply: (parentId: string, body: string) => Promise<void>;
  /** Highlight info for a doc source line, or null if not commented. */
  lineHighlight: (line: number) => { active: boolean } | null;
  focusLine: (line: number) => void;
}
const Ctx = createContext<CommentsCtx | null>(null);

export function CommentsProvider({
  workspaceSlug, pageSlug, source, articleRef, currentUserId, children,
}: {
  workspaceSlug: string;
  pageSlug: string;
  source: string;
  articleRef: React.RefObject<HTMLElement | null>;
  currentUserId: string | undefined;
  children: React.ReactNode;
}) {
  const [comments, setComments] = useState<PageComment[]>([]);
  const [snapshots, setSnapshots] = useState<{ content_hash: string; source: string }[]>([]);
  const [members, setMembers] = useState<MemberInfo[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [panelOpen, setPanelOpen] = useState(true);
  const [pending, setPending] = useState<{ start: number; end: number; x: number; y: number } | null>(null);
  const [composing, setComposing] = useState<{ start: number; end: number; quote: string } | null>(null);
  const enabled = !!workspaceSlug && !!pageSlug;

  const reload = useCallback(async () => {
    if (!workspaceSlug || !pageSlug) { setComments([]); setSnapshots([]); return; }
    const res = await listPageComments(workspaceSlug, pageSlug);
    setComments(res.comments);
    setSnapshots(res.snapshots);
  }, [workspaceSlug, pageSlug]);

  useEffect(() => { void reload(); }, [reload]);
  useEffect(() => {
    if (!workspaceSlug) return;
    listMembers(workspaceSlug).then(setMembers).catch(() => setMembers([]));
  }, [workspaceSlug]);
  // Reset the focused thread when navigating to a different page.
  useEffect(() => { setActiveId(null); }, [pageSlug]);

  const nameOf = useCallback(
    (uid: string) => members.find((m) => m.id === uid)?.name ?? uid.slice(0, 8),
    [members],
  );

  const threads = useMemo<Thread[]>(() => {
    const maps = buildSnapshotMaps(snapshots, source);
    const snapById = new Map(snapshots.map((s) => [s.content_hash, s.source]));
    const repliesByParent = new Map<string, PageComment[]>();
    for (const c of comments) {
      if (c.parent_id) {
        const arr = repliesByParent.get(c.parent_id) ?? [];
        arr.push(c);
        repliesByParent.set(c.parent_id, arr);
      }
    }
    return comments.filter((c) => !c.parent_id).map((root) => {
      let anchor: MappedAnchor = { kind: 'outdated' };
      let quote = '';
      if (root.content_hash && root.start_line != null && root.end_line != null) {
        const m = maps.get(root.content_hash);
        anchor = m ? mapWithSurviving(m, root.start_line, root.end_line) : { kind: 'outdated' };
        if (anchor.kind === 'anchored') quote = quoteOf(source, anchor.startLine, anchor.endLine);
        else { const snap = snapById.get(root.content_hash); if (snap) quote = quoteOf(snap, root.start_line, root.end_line); }
      }
      return { root, replies: repliesByParent.get(root.id) ?? [], anchor, quote };
    });
  }, [comments, snapshots, source]);

  const anchored = threads.filter((t) => t.anchor.kind === 'anchored' && !t.root.resolved);
  const outdated = threads.filter((t) => t.anchor.kind === 'outdated' && !t.root.resolved);
  const resolved = threads.filter((t) => t.root.resolved);
  const openCount = anchored.length + outdated.length;

  useEffect(() => {
    if (activeId == null && anchored.length) setActiveId(anchored[0].root.id);
  }, [anchored, activeId]);

  // map every commented source line → its thread (first wins)
  const lineToThread = useMemo(() => {
    const m = new Map<number, string>();
    for (const t of anchored) {
      if (t.anchor.kind !== 'anchored') continue;
      for (let l = t.anchor.startLine; l <= t.anchor.endLine; l++) if (!m.has(l)) m.set(l, t.root.id);
    }
    return m;
  }, [anchored]);

  // selection → floating "Comment" toolbar
  useEffect(() => {
    if (!enabled) return;
    const onUp = () => {
      // A composer is open — don't let the still-present selection re-arm the
      // floating "Comment" bubble (it would resurface after cancel/submit).
      if (composing) return;
      const root = articleRef.current;
      if (!root) return;
      const lr = selectionLineRange(root);
      if (!lr) { setPending(null); return; }
      const rect = window.getSelection()!.getRangeAt(0).getBoundingClientRect();
      setPending({ ...lr, x: rect.left + rect.width / 2, y: rect.top });
    };
    document.addEventListener('mouseup', onUp);
    return () => document.removeEventListener('mouseup', onUp);
  }, [articleRef, composing, enabled]);

  const submitNew = async (body: string) => {
    if (!composing || !body.trim()) return;
    await createPageComment(workspaceSlug, { slug: pageSlug, body: body.trim(), source, startLine: composing.start, endLine: composing.end });
    setComposing(null);
    setPending(null);
    await reload();
  };
  const submitReply = async (parentId: string, body: string) => {
    if (!body.trim()) return;
    await createPageComment(workspaceSlug, { slug: pageSlug, body: body.trim(), parentId });
    await reload();
  };

  const value: CommentsCtx = {
    enabled,
    anchored, outdated, resolved, openCount,
    activeId, setActive: setActiveId,
    panelOpen, setPanelOpen,
    composing, cancelCompose: () => { setComposing(null); setPending(null); }, submitNew,
    members, nameOf, currentUserId, myId: currentUserId ?? 'me', myName: currentUserId ? nameOf(currentUserId) : 'You',
    onResolve: async (id, r) => { await setPageCommentResolved(workspaceSlug, id, r); await reload(); },
    onDelete: async (id) => { await deletePageComment(workspaceSlug, id); await reload(); },
    submitReply,
    lineHighlight: (line) => (lineToThread.has(line) ? { active: lineToThread.get(line) === activeId } : null),
    focusLine: (line) => { const id = lineToThread.get(line); if (id) { setActiveId(id); setPanelOpen(true); } },
  };

  const startComment = () => {
    if (!pending) return;
    setComposing({ start: pending.start, end: pending.end, quote: quoteOf(source, pending.start, pending.end) });
    setPanelOpen(true);
    setPending(null);
  };

  return (
    <Ctx.Provider value={value}>
      {children}
      {enabled && pending && !composing && (
        <div style={{
          position: 'fixed', zIndex: 60, top: pending.y - 46, left: pending.x - 52,
          display: 'flex', alignItems: 'center', background: C.ink, borderRadius: 9, padding: 4,
          boxShadow: '0 10px 28px -8px rgba(0,0,0,0.4)',
        }}>
          <button onMouseDown={(e) => { e.preventDefault(); startComment(); }} style={{
            display: 'flex', alignItems: 'center', gap: 6, height: 28, padding: '0 11px', color: '#fff',
            fontSize: 13, fontWeight: 600, borderRadius: 6, border: 'none', background: C.accent, cursor: 'pointer',
          }}>
            <MessageSquare size={14} /> Comment
          </button>
          <div style={{ position: 'absolute', left: 46, bottom: -5, width: 10, height: 10, background: C.ink, transform: 'rotate(45deg)' }} />
        </div>
      )}
    </Ctx.Provider>
  );
}

/** Top-bar comments toggle (count + open/close), à la Feishu/Notion. Renders
 *  nothing off a page. Place it in the page header. */
export function CommentsHeaderToggle({ style }: { style?: React.CSSProperties }) {
  const ctx = useContext(Ctx);
  if (!ctx || !ctx.enabled) return null;
  return (
    <button
      onClick={() => ctx.setPanelOpen(!ctx.panelOpen)}
      title={ctx.panelOpen ? 'Hide comments' : 'Show comments'}
      style={{
        ...style, fontWeight: 600,
        background: ctx.panelOpen ? C.accentSoft : (style?.background ?? 'transparent'),
        color: ctx.panelOpen ? C.accent : (style?.color ?? C.ink2),
      }}
    >
      <MessageSquare size={14} /> {ctx.openCount}
    </button>
  );
}

// ── markdown block renderers: tag every block with its source line, and wrap
//    commented blocks' inline content in a soft Highlight span (hugs the text) ─
type BlockProps = {
  node?: { position?: { start?: { line?: number }; end?: { line?: number } } };
  children?: React.ReactNode;
} & React.HTMLAttributes<HTMLElement>;

function makeBlock(tag: string) {
  return function Block({ node, children, ...rest }: BlockProps) {
    const ctx = useContext(Ctx);
    const line = node?.position?.start?.line;
    const endLine = node?.position?.end?.line ?? line;
    const hl = ctx && line ? ctx.lineHighlight(line) : null;
    const inner = hl ? (
      <span
        onClick={(e) => { e.stopPropagation(); ctx!.focusLine(line!); }}
        style={{
          cursor: 'pointer', borderRadius: 3, padding: '1px 1px',
          background: hl.active ? 'rgba(226,89,11,0.16)' : 'rgba(226,89,11,0.07)',
          boxShadow: hl.active ? `inset 0 -2px 0 ${C.accent}` : 'inset 0 -1.5px 0 rgba(226,89,11,0.32)',
          transition: 'background .15s, box-shadow .15s',
          WebkitBoxDecorationBreak: 'clone', boxDecorationBreak: 'clone',
        }}
      >
        {children}
      </span>
    ) : children;
    return createElement(tag, { 'data-source-line': line, 'data-end-line': endLine, ...rest }, inner);
  };
}

export const commentMarkdownComponents: Components = {
  p: makeBlock('p'), li: makeBlock('li'), h1: makeBlock('h1'), h2: makeBlock('h2'),
  h3: makeBlock('h3'), h4: makeBlock('h4'), blockquote: makeBlock('blockquote'), pre: makeBlock('pre'),
} as Components;

// ── the right-hand comment panel ───────────────────────────────────────────
export function CommentsPanel() {
  const ctx = useContext(Ctx);
  const [showResolved, setShowResolved] = useState(false);
  const [showOutdated, setShowOutdated] = useState(false);
  if (!ctx) return null;
  const { anchored, outdated, resolved, openCount, activeId, setActive, panelOpen, setPanelOpen,
    composing, cancelCompose, submitNew, members, nameOf, currentUserId, myId, myName,
    onResolve, onDelete, submitReply } = ctx;

  // Collapsed: nothing here — the header toggle reopens the panel.
  if (!panelOpen || (openCount === 0 && resolved.length === 0 && !composing)) return null;

  return (
    <aside style={{
      width: 360, flexShrink: 0, alignSelf: 'stretch', borderLeft: `1px solid ${C.line}`,
      background: C.panel, display: 'flex', flexDirection: 'column', minHeight: 0,
    }}>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '12px 16px 12px 20px', flexShrink: 0 }}>
        <div style={{ display: 'flex', alignItems: 'baseline', gap: 6 }}>
          <span style={{ fontSize: 13.5, fontWeight: 600, color: C.ink }}>Comments</span>
          <span style={{ fontSize: 12.5, color: C.faint }}>({openCount})</span>
        </div>
        <button onClick={() => setPanelOpen(false)} title="Collapse" style={{ ...ghostBtn, padding: 5 }}>
          <ChevronsRight size={16} />
        </button>
      </div>

      <div style={{ flex: 1, overflow: 'auto', padding: '4px 16px 24px' }}>
        {composing && (
          <Composer quote={composing.quote} meId={myId} meName={myName} members={members}
            placeholder="Add a comment…  use @ to mention" autoFocus
            onCancel={cancelCompose} onSubmit={submitNew} primaryLabel="Comment" />
        )}

        <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
          {[...anchored, ...outdated].map((t) => (
            <CommentCard key={t.root.id} t={t} active={t.root.id === activeId} members={members}
              meId={myId} meName={myName} currentUserId={currentUserId} nameOf={nameOf}
              onActivate={() => setActive(t.root.id)}
              onResolve={() => onResolve(t.root.id, true)}
              onReply={(body) => submitReply(t.root.id, body)} onDelete={onDelete} />
          ))}
        </div>

        {outdated.length > 0 && (
          <button onClick={() => setShowOutdated((s) => !s)} style={{ ...ghostBtn, width: '100%', justifyContent: 'flex-start', padding: '8px 4px', fontSize: 13, marginTop: 14 }}>
            <span style={{ width: 8, height: 8, borderRadius: '50%', background: C.amber }} />
            {outdated.length} not in your version
            {showOutdated ? <ChevronDown size={14} color={C.faint} style={{ marginLeft: 'auto' }} /> : <ChevronRight size={14} color={C.faint} style={{ marginLeft: 'auto' }} />}
          </button>
        )}

        {resolved.length > 0 && (
          <div style={{ marginTop: 18 }}>
            <button onClick={() => setShowResolved((s) => !s)} style={{ ...ghostBtn, width: '100%', justifyContent: 'flex-start', padding: '8px 4px', fontSize: 13 }}>
              <CheckCircle2 size={15} color={C.green} />
              {resolved.length} resolved
              {showResolved ? <ChevronDown size={14} color={C.faint} style={{ marginLeft: 'auto' }} /> : <ChevronRight size={14} color={C.faint} style={{ marginLeft: 'auto' }} />}
            </button>
            {showResolved && resolved.map((t) => (
              <div key={t.root.id} style={{ opacity: 0.72, padding: '10px 4px', marginTop: 2, borderTop: `1px solid ${C.lineSoft}` }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 6 }}>
                  <Avatar id={t.root.user_id} name={nameOf(t.root.user_id)} size={22} />
                  <span style={{ fontSize: 13, fontWeight: 600, color: C.ink, flex: 1 }}>{nameOf(t.root.user_id)}</span>
                  <span style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 11.5, fontWeight: 600, color: C.green }}><Check size={13} />Resolved</span>
                </div>
                <div style={{ fontSize: 13, color: C.muted, marginBottom: 8, lineHeight: 1.5 }}>{t.root.body}</div>
                <button onClick={() => onResolve(t.root.id, false)} style={{ ...ghostBtn, padding: '3px 6px', fontSize: 12 }}><RotateCcw size={12} /> Reopen</button>
              </div>
            ))}
          </div>
        )}
      </div>
    </aside>
  );
}

// ── one thread card ─────────────────────────────────────────────────────────
function CommentCard({
  t, active, members, meId, meName, currentUserId, nameOf, onActivate, onResolve, onReply, onDelete,
}: {
  t: Thread; active: boolean; members: MemberInfo[]; meId: string; meName: string;
  currentUserId: string | undefined; nameOf: (id: string) => string;
  onActivate: () => void; onResolve: () => void;
  onReply: (body: string) => void | Promise<void>; onDelete: (id: string) => void;
}) {
  const [replying, setReplying] = useState(false);
  const authorName = nameOf(t.root.user_id);
  const isOutdated = t.anchor.kind === 'outdated';

  if (!active) {
    return (
      <div onClick={onActivate} style={{ background: C.panel, border: `1px solid ${C.line}`, borderRadius: 12, padding: '11px 13px', cursor: 'pointer', boxShadow: '0 1px 2px rgba(29,28,26,0.03)' }}>
        <div style={{ marginBottom: 9, paddingLeft: 8, borderLeft: `2.5px solid ${isOutdated ? C.amberSoft : '#f0d8c5'}` }}>
          <span style={{ fontSize: 12, lineHeight: 1.4, color: C.faint, fontStyle: 'italic', display: '-webkit-box', WebkitLineClamp: 1, WebkitBoxOrient: 'vertical', overflow: 'hidden' }}>
            {isOutdated && <span style={{ color: C.amber, fontStyle: 'normal', fontWeight: 600 }}>outdated · </span>}<Quote text={t.quote} />
          </span>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 6 }}>
          <Avatar id={t.root.user_id} name={authorName} size={22} />
          <span style={{ fontSize: 13, fontWeight: 600, color: C.ink, flex: 1, minWidth: 0, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{authorName}</span>
          <span style={{ fontSize: 11.5, color: C.faint }}>{relTime(t.root.created_at)}</span>
        </div>
        <div style={{ fontSize: 13.5, lineHeight: 1.5, color: C.ink2, display: '-webkit-box', WebkitLineClamp: 2, WebkitBoxOrient: 'vertical', overflow: 'hidden' }}>{t.root.body}</div>
        {t.replies.length > 0 && (
          <div style={{ display: 'flex', alignItems: 'center', gap: 5, marginTop: 7, fontSize: 12, color: C.faint }}>
            <MessageSquare size={13} />{t.replies.length} {t.replies.length === 1 ? 'reply' : 'replies'}
          </div>
        )}
      </div>
    );
  }

  return (
    <div onClick={onActivate} style={{ background: C.panel, border: '1px solid transparent', borderRadius: 12, padding: '13px 14px 12px', cursor: 'pointer', position: 'relative', boxShadow: `0 4px 16px -10px rgba(29,28,26,0.25), 0 0 0 1.5px ${isOutdated ? C.amber : C.accent}` }}>
      <div style={{ display: 'flex', gap: 8, marginBottom: 11, paddingLeft: 9, borderLeft: `2.5px solid ${isOutdated ? C.amber : C.accent}` }}>
        <span style={{ fontSize: 12.5, lineHeight: 1.45, color: C.muted, fontStyle: 'italic', display: '-webkit-box', WebkitLineClamp: 2, WebkitBoxOrient: 'vertical', overflow: 'hidden' }}>
          {isOutdated && <span style={{ color: C.amber, fontStyle: 'normal', fontWeight: 600 }}>outdated · </span>}<Quote text={t.quote} />
        </span>
      </div>
      <div style={{ display: 'flex', alignItems: 'center', gap: 9, marginBottom: 7 }}>
        <Avatar id={t.root.user_id} name={authorName} size={26} />
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ fontSize: 13.5, fontWeight: 600, color: C.ink, lineHeight: 1.2 }}>{authorName}</div>
          <div style={{ fontSize: 11.5, color: C.faint }}>{relTime(t.root.created_at)} ago</div>
        </div>
        {t.root.user_id === currentUserId ? (
          <button onClick={(e) => { e.stopPropagation(); onDelete(t.root.id); }} title="Delete" style={{ ...ghostBtn, padding: 4 }}><Trash2 size={14} /></button>
        ) : (
          <MoreHorizontal size={15} color={C.faint} />
        )}
      </div>
      <div style={{ fontSize: 14, lineHeight: 1.55, color: C.ink2, marginBottom: t.replies.length ? 12 : 10 }}>{t.root.body}</div>

      {t.replies.length > 0 && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 11, marginBottom: 10 }}>
          {t.replies.map((r) => (
            <div key={r.id} style={{ display: 'flex', gap: 8 }}>
              <Avatar id={r.user_id} name={nameOf(r.user_id)} size={22} />
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ display: 'flex', alignItems: 'baseline', gap: 7, marginBottom: 1 }}>
                  <span style={{ fontSize: 12.5, fontWeight: 600, color: C.ink }}>{nameOf(r.user_id)}</span>
                  <span style={{ fontSize: 11, color: C.faint }}>{relTime(r.created_at)}</span>
                </div>
                <div style={{ fontSize: 13.5, lineHeight: 1.5, color: C.ink2 }}>{r.body}</div>
              </div>
            </div>
          ))}
        </div>
      )}

      {!replying ? (
        <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginTop: 2 }}>
          <button onClick={(e) => { e.stopPropagation(); setReplying(true); }} style={ghostBtn}><Reply size={14} />Reply</button>
          <div style={{ flex: 1 }} />
          <button onClick={(e) => { e.stopPropagation(); onResolve(); }} style={{ ...ghostBtn, color: C.green }}><Check size={15} />Resolve</button>
        </div>
      ) : (
        <div onClick={(e) => e.stopPropagation()}>
          <Composer meId={meId} meName={meName} members={members} placeholder="Reply…" autoFocus compact
            onCancel={() => setReplying(false)}
            onSubmit={async (body) => { await onReply(body); setReplying(false); }} primaryLabel="Send" />
        </div>
      )}
    </div>
  );
}

// ── composer with @ mention autocomplete ───────────────────────────────────
function Composer({
  quote, meId, meName, members, placeholder, autoFocus, compact, onCancel, onSubmit, primaryLabel,
}: {
  quote?: string; meId: string; meName: string; members: MemberInfo[];
  placeholder?: string; autoFocus?: boolean; compact?: boolean;
  onCancel: () => void; onSubmit: (body: string) => void | Promise<void>; primaryLabel: string;
}) {
  const [value, setValue] = useState('');
  const ref = useRef<HTMLTextAreaElement>(null);
  const [menu, setMenu] = useState<{ query: string; at: number } | null>(null);

  const sync = (next: string, caret: number) => {
    setValue(next);
    const m = /@([A-Za-z0-9_-]*)$/.exec(next.slice(0, caret));
    setMenu(m ? { query: m[1], at: caret - m[1].length - 1 } : null);
  };
  const pick = (name: string) => {
    if (!menu) return;
    setValue(value.slice(0, menu.at) + '@' + name + ' ' + value.slice(menu.at + 1 + menu.query.length));
    setMenu(null);
    ref.current?.focus();
  };
  const matches = menu ? members.filter((m) => m.name.toLowerCase().startsWith(menu.query.toLowerCase())).slice(0, 5) : [];

  return (
    <div style={{ marginBottom: compact ? 0 : 14, marginTop: compact ? 4 : 0 }}>
      {quote && (
        <div style={{ marginBottom: 9, paddingLeft: 9, borderLeft: `2.5px solid ${C.accent}` }}>
          <span style={{ fontSize: 12.5, lineHeight: 1.45, color: C.muted, fontStyle: 'italic', display: '-webkit-box', WebkitLineClamp: 2, WebkitBoxOrient: 'vertical', overflow: 'hidden' }}><Quote text={quote} /></span>
        </div>
      )}
      <div style={{ display: 'flex', gap: 8, alignItems: 'flex-start' }}>
        <Avatar id={meId} name={meName} size={22} />
        <div style={{ flex: 1, position: 'relative', border: `1.5px solid ${C.accent}`, borderRadius: 9, background: '#fff', overflow: 'hidden' }}>
          <textarea ref={ref} value={value} placeholder={placeholder} rows={2} autoFocus={autoFocus}
            onChange={(e) => sync(e.target.value, e.target.selectionStart)}
            style={{ width: '100%', border: 'none', outline: 'none', resize: 'none', padding: '8px 10px', fontSize: 13.5, lineHeight: 1.5, color: C.ink, fontFamily: fonts.sans, boxSizing: 'border-box', background: 'transparent' }} />
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '5px 8px', borderTop: `1px solid ${C.lineSoft}` }}>
            <AtSign size={15} color={C.faint} />
            <div style={{ flex: 1 }} />
            <button onClick={onCancel} style={{ ...ghostBtn, padding: '5px 9px' }}>Cancel</button>
            <button onClick={() => onSubmit(value)} disabled={!value.trim()}
              style={{ display: 'flex', alignItems: 'center', gap: 5, padding: '5px 12px', borderRadius: 7, border: 'none', background: value.trim() ? C.accent : C.line, color: value.trim() ? '#fff' : C.faint, fontSize: 12.5, fontWeight: 600, cursor: value.trim() ? 'pointer' : 'default' }}>
              {primaryLabel}
            </button>
          </div>
          {matches.length > 0 && (
            <div style={{ position: 'absolute', top: '100%', left: 0, zIndex: 70, marginTop: 2, minWidth: 170, background: '#fff', border: `1px solid ${C.line}`, borderRadius: 8, boxShadow: '0 6px 18px rgba(0,0,0,.12)', overflow: 'hidden' }}>
              {matches.map((m) => (
                <button key={m.id} onMouseDown={(e) => { e.preventDefault(); pick(m.name); }}
                  style={{ display: 'flex', alignItems: 'center', gap: 8, width: '100%', textAlign: 'left', padding: '7px 10px', border: 'none', background: 'transparent', fontSize: 12.5, color: C.ink, cursor: 'pointer' }}
                  onMouseEnter={(e) => { e.currentTarget.style.background = C.tag; }}
                  onMouseLeave={(e) => { e.currentTarget.style.background = 'transparent'; }}>
                  <Avatar id={m.id} name={m.name} size={18} /> {m.name}
                </button>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
