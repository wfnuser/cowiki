import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
} from 'react';
import { Check, CircleDashed, CloudUpload, X } from 'lucide-react';
import { C, fonts } from '@/lib/design';
import {
  LivePreviewEditor,
  type LivePreviewEditorHandle,
} from './editor/LivePreviewEditor';
import {
  VersionedDocument,
  type DocumentReplacement,
  type DocumentSnapshot,
} from '@/lib/versioned-document';

type SaveStatus = 'saved' | 'dirty' | 'saving' | 'error';

/**
 * Obsidian-style live-preview editor for a page body (frontmatter included,
 * shown as a dimmed block). Human and agent edits share this document; every
 * accepted edit advances its revision so an agent can never apply stale text
 * over newer typing. Esc or Done exits after flushing any pending save.
 */
export interface PageEditorHandle {
  /** Current text and revision read by an in-app agent before proposing an edit. */
  readDocument: () => DocumentSnapshot;
  /** Applies only when expectedRevision still matches; stale edits throw internally. */
  applyAgentEdit: (edit: Omit<DocumentReplacement, 'writer'>) => DocumentSnapshot;
}

export const PageEditor = forwardRef<PageEditorHandle, {
  initialBody: string;
  /** Persist the body; called on the autosave debounce, not on every keystroke. */
  onSave: (body: string) => Promise<void>;
  /** Exit edit mode (pending changes are flushed first). */
  onDone: () => void;
  onWikilink?: (target: string) => void;
  getPageSlugs?: () => string[];
}>(function PageEditor({
  initialBody,
  onSave,
  onDone,
  onWikilink,
  getPageSlugs,
}, ref) {
  const [status, setStatus] = useState<SaveStatus>('saved');
  const [error, setError] = useState<string | null>(null);

  const latestBody = useRef(initialBody);
  const savedBody = useRef(initialBody);
  const document = useRef(new VersionedDocument(initialBody));
  const liveEditor = useRef<LivePreviewEditorHandle>(null);
  const saving = useRef(false);
  const timer = useRef<number | null>(null);

  const flush = useCallback(async () => {
    if (timer.current !== null) { window.clearTimeout(timer.current); timer.current = null; }
    if (saving.current) return;
    saving.current = true;
    try {
      while (latestBody.current !== savedBody.current) {
        const body = latestBody.current;
        setStatus('saving');
        await onSave(body);
        savedBody.current = body;
      }
      setStatus('saved');
      setError(null);
    } catch (e) {
      setStatus('error');
      setError(e instanceof Error ? e.message : 'Failed to save');
    } finally {
      saving.current = false;
    }
  }, [onSave]);

  const scheduleSave = useCallback((doc: string) => {
    latestBody.current = doc;
    if (timer.current !== null) window.clearTimeout(timer.current);
    if (doc === savedBody.current) {
      setStatus('saved');
      return;
    }
    setStatus('dirty');
    timer.current = window.setTimeout(() => { void flush(); }, 900);
  }, [flush]);

  const handleDocChanged = useCallback((doc: string, source: 'human' | 'external') => {
    if (source === 'human') {
      const current = document.current.snapshot();
      document.current.replace({
        content: doc,
        expectedRevision: current.revision,
        writer: 'human',
      });
    }
    scheduleSave(doc);
  }, [scheduleSave]);

  useImperativeHandle(ref, () => ({
    readDocument: () => document.current.snapshot(),
    applyAgentEdit: (edit) => {
      const editor = liveEditor.current;
      if (!editor) throw new Error('shared editor is not ready');
      const accepted = document.current.replace({ ...edit, writer: 'agent' });
      // Synchronous hand-off closes the check→apply race: user input cannot
      // arrive after validation but before the accepted text reaches CodeMirror.
      editor.replaceDocument(accepted.content);
      return accepted;
    },
  }), []);

  const handleDone = useCallback(async () => {
    await flush();
    if (latestBody.current !== savedBody.current) return; // save failed — stay
    onDone();
  }, [flush, onDone]);

  const handleWikilink = useCallback(async (target: string) => {
    await flush();
    onWikilink?.(target);
  }, [flush, onWikilink]);

  // Esc exits (unless CodeMirror already consumed it, e.g. closing autocomplete).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && !e.defaultPrevented) void handleDone();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [handleDone]);

  // Warn before closing the tab with unsaved edits.
  useEffect(() => {
    const onBeforeUnload = (e: BeforeUnloadEvent) => {
      if (latestBody.current !== savedBody.current) e.preventDefault();
    };
    window.addEventListener('beforeunload', onBeforeUnload);
    return () => window.removeEventListener('beforeunload', onBeforeUnload);
  }, []);

  // Flush on unmount so navigating away never drops edits.
  useEffect(() => () => { void flush(); }, [flush]);

  const statusUi = {
    saved: { icon: <Check size={12} />, text: 'Saved', color: C.muted },
    dirty: { icon: <CircleDashed size={12} />, text: 'Editing…', color: C.faint },
    saving: { icon: <CloudUpload size={12} />, text: 'Saving…', color: C.muted },
    error: { icon: <X size={12} />, text: error || 'Save failed', color: C.red },
  }[status];

  return (
    <div style={{ position: 'absolute', inset: 0 }}>
      <LivePreviewEditor
        ref={liveEditor}
        initialDoc={initialBody}
        onDocChanged={handleDocChanged}
        onRequestSave={() => { void flush(); }}
        onWikilink={handleWikilink}
        getPageSlugs={getPageSlugs}
      />

      {/* Floating status + Done */}
      <div style={{
        position: 'absolute', top: 10, right: 16, zIndex: 10,
        display: 'flex', alignItems: 'center', gap: 10,
        padding: '5px 6px 5px 12px', borderRadius: 999,
        background: C.panel, border: `1px solid ${C.line}`,
        boxShadow: '0 1px 2px rgba(29, 28, 26, 0.06)',
      }}>
        <span
          title="Autosaves as you type · ⌘S to save now · Esc to finish"
          style={{
            display: 'flex', alignItems: 'center', gap: 5,
            fontSize: 12, color: statusUi.color, fontFamily: fonts.sans,
            maxWidth: 320, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
          }}
        >
          {statusUi.icon} {statusUi.text}
        </span>
        <button
          onClick={() => { void handleDone(); }}
          style={{
            padding: '4px 14px', borderRadius: 999, fontSize: 12.5, fontWeight: 600,
            color: '#fff', background: C.accent, border: 'none', cursor: 'pointer',
          }}
        >
          Done
        </button>
      </div>
    </div>
  );
});

export default PageEditor;
