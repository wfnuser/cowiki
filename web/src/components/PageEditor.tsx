import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
} from 'react';
import type { CSSProperties } from 'react';
import { Check, X } from 'lucide-react';
import { C, fonts, shadows } from '@/lib/design';
import {
  LivePreviewEditor,
  type LivePreviewEditorHandle,
} from './editor/LivePreviewEditor';
import {
  VersionedDocument,
  type DocumentReplacement,
  type DocumentSnapshot,
} from '@/lib/versioned-document';
import { restoreSystemFrontmatter, splitSystemFrontmatter } from '@/lib/page-frontmatter';

type SaveFeedback = { type: 'saved' | 'error'; text: string } | null;

/**
 * Obsidian-style live-preview editor for a page body. CoWiki-owned frontmatter
 * is hidden and restored on save. Human and agent edits share this document; every
 * accepted edit advances its revision so an agent can never apply stale text
 * over newer typing. Autosave stays quiet; Esc exits after flushing any pending save.
 */
export interface PageEditorHandle {
  /** Current text and revision read by an in-app agent before proposing an edit. */
  readDocument: () => DocumentSnapshot;
  /** Applies only when expectedRevision still matches; stale edits throw internally. */
  applyAgentEdit: (edit: Omit<DocumentReplacement, 'writer'>) => DocumentSnapshot;
  /** Reload an on-disk edit when clean; preserve both versions when human text is dirty. */
  applyExternalFile: (fullBody: string) => 'applied' | 'conflict';
  /** Handle an Agent deleting or renaming the open file without losing human text. */
  applyExternalDeletion: () => 'applied' | 'conflict';
}

type ExternalConflict =
  | { kind: 'changed'; fullBody: string }
  | { kind: 'deleted' };

export const PageEditor = forwardRef<PageEditorHandle, {
  initialBody: string;
  /** Persist the body; called on the autosave debounce, not on every keystroke. */
  onSave: (body: string, expectedExternalBody?: string | null) => Promise<void>;
  onExternalReload?: (fullBody: string) => void;
  onExternalDeleted?: () => void;
  /** Exit edit mode (pending changes are flushed first). */
  onDone: () => void;
  onWikilink?: (target: string) => void;
  getPageSlugs?: () => string[];
}>(function PageEditor({
  initialBody,
  onSave,
  onExternalReload,
  onExternalDeleted,
  onDone,
  onWikilink,
  getPageSlugs,
}, ref) {
  const [feedback, setFeedback] = useState<SaveFeedback>(null);
  const [externalConflict, setExternalConflict] = useState<ExternalConflict | null>(null);
  const initialPage = useRef(splitSystemFrontmatter(initialBody));

  const latestBody = useRef(initialPage.current.body);
  const savedBody = useRef(initialPage.current.body);
  const document = useRef(new VersionedDocument(initialPage.current.body));
  const liveEditor = useRef<LivePreviewEditorHandle>(null);
  const saving = useRef(false);
  const timer = useRef<number | null>(null);
  const feedbackTimer = useRef<number | null>(null);
  const manualSaveRequested = useRef(false);

  const flush = useCallback(async (showConfirmation = false) => {
    if (showConfirmation) manualSaveRequested.current = true;
    if (timer.current !== null) { window.clearTimeout(timer.current); timer.current = null; }
    if (saving.current) return;
    saving.current = true;
    try {
      while (latestBody.current !== savedBody.current) {
        const body = latestBody.current;
        await onSave(restoreSystemFrontmatter(initialPage.current.systemFrontmatter, body));
        savedBody.current = body;
      }
      if (feedbackTimer.current !== null) window.clearTimeout(feedbackTimer.current);
      if (manualSaveRequested.current) {
        setFeedback({ type: 'saved', text: 'Saved locally' });
        feedbackTimer.current = window.setTimeout(() => setFeedback(null), 1600);
      } else {
        setFeedback(null);
      }
    } catch (e) {
      setFeedback({ type: 'error', text: e instanceof Error ? e.message : 'Failed to save' });
    } finally {
      manualSaveRequested.current = false;
      saving.current = false;
    }
  }, [onSave]);

  const scheduleSave = useCallback((doc: string) => {
    latestBody.current = doc;
    if (timer.current !== null) window.clearTimeout(timer.current);
    if (doc === savedBody.current) {
      return;
    }
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
    applyExternalFile: (fullBody) => {
      const next = splitSystemFrontmatter(fullBody);
      if (latestBody.current !== savedBody.current) {
        setExternalConflict({ kind: 'changed', fullBody });
        return 'conflict';
      }
      const editor = liveEditor.current;
      if (!editor) return 'conflict';
      initialPage.current = next;
      latestBody.current = next.body;
      savedBody.current = next.body;
      const current = document.current.snapshot();
      document.current.replace({
        content: next.body,
        expectedRevision: current.revision,
        writer: 'agent',
      });
      editor.replaceDocument(next.body);
      setExternalConflict(null);
      return 'applied';
    },
    applyExternalDeletion: () => {
      if (latestBody.current !== savedBody.current) {
        setExternalConflict({ kind: 'deleted' });
        return 'conflict';
      }
      onExternalDeleted?.();
      return 'applied';
    },
  }), [onExternalDeleted]);

  const handleDone = useCallback(async () => {
    await flush();
    if (latestBody.current !== savedBody.current) return; // save failed — stay
    onDone();
  }, [flush, onDone]);

  const handleWikilink = useCallback(async (target: string) => {
    await flush();
    onWikilink?.(target);
  }, [flush, onWikilink]);

  const reloadExternal = useCallback(() => {
    if (!externalConflict) return;
    if (externalConflict.kind === 'deleted') {
      setExternalConflict(null);
      setFeedback(null);
      onExternalDeleted?.();
      return;
    }
    const next = splitSystemFrontmatter(externalConflict.fullBody);
    initialPage.current = next;
    latestBody.current = next.body;
    savedBody.current = next.body;
    const current = document.current.snapshot();
    document.current.replace({ content: next.body, expectedRevision: current.revision, writer: 'agent' });
    liveEditor.current?.replaceDocument(next.body);
    setExternalConflict(null);
    setFeedback(null);
    onExternalReload?.(externalConflict.fullBody);
  }, [externalConflict, onExternalDeleted, onExternalReload]);

  const keepHumanVersion = useCallback(async () => {
    if (!externalConflict) return;
    const external = externalConflict.kind === 'changed'
      ? splitSystemFrontmatter(externalConflict.fullBody)
      : initialPage.current;
    const merged = restoreSystemFrontmatter(external.systemFrontmatter, latestBody.current);
    try {
      await onSave(
        merged,
        externalConflict.kind === 'changed' ? externalConflict.fullBody : null,
      );
      initialPage.current = external;
      savedBody.current = latestBody.current;
      setExternalConflict(null);
      setFeedback({ type: 'saved', text: 'Kept your version' });
    } catch (cause) {
      setFeedback({ type: 'error', text: cause instanceof Error ? cause.message : 'The file changed again' });
    }
  }, [externalConflict, onSave]);

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
  useEffect(() => () => {
    if (feedbackTimer.current !== null) window.clearTimeout(feedbackTimer.current);
    void flush();
  }, [flush]);

  return (
    <div style={{ position: 'absolute', inset: 0 }}>
      <LivePreviewEditor
        ref={liveEditor}
        initialDoc={initialPage.current.body}
        onDocChanged={handleDocChanged}
        onRequestSave={() => { void flush(true); }}
        onWikilink={handleWikilink}
        getPageSlugs={getPageSlugs}
      />

      {feedback && <div style={{
        position: 'absolute', top: 12, right: 18, zIndex: 10,
        padding: '7px 11px', borderRadius: 8,
        background: C.panel, border: `1px solid ${feedback.type === 'error' ? C.redSoft : C.line}`,
        boxShadow: shadows.toast,
      }}>
        <span
          style={{
            display: 'flex', alignItems: 'center', gap: 5,
            fontSize: 12, color: feedback.type === 'error' ? C.red : C.muted, fontFamily: fonts.sans,
            maxWidth: 320, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
          }}
        >
          {feedback.type === 'error' ? <X size={12} /> : <Check size={12} />}
          {feedback.text}
        </span>
      </div>}

      {externalConflict && <div style={{
        position: 'absolute', top: 12, left: '50%', transform: 'translateX(-50%)', zIndex: 12,
        display: 'flex', alignItems: 'center', gap: 10, padding: '9px 12px', borderRadius: 9,
        background: C.panel, border: `1px solid ${C.redSoft}`, boxShadow: shadows.overlay,
        font: `500 12px ${fonts.sans}`, color: C.ink2,
      }}>
        <span>{externalConflict.kind === 'deleted'
          ? 'The file was deleted or renamed in the Agent terminal.'
          : 'The file changed in the Agent terminal.'}</span>
        <button type="button" onClick={reloadExternal} style={conflictButtonStyle}>
          {externalConflict.kind === 'deleted' ? 'Accept deletion' : 'Reload Agent version'}
        </button>
        <button type="button" onClick={() => { void keepHumanVersion(); }} style={{ ...conflictButtonStyle, background: C.ink, color: C.onAccent }}>Keep my version</button>
      </div>}
    </div>
  );
});

const conflictButtonStyle: CSSProperties = {
  border: `1px solid ${C.line}`, borderRadius: 6, background: C.panel, color: C.ink2,
  padding: '4px 8px', font: `600 11px ${fonts.sans}`, cursor: 'pointer', whiteSpace: 'nowrap',
};

export default PageEditor;
