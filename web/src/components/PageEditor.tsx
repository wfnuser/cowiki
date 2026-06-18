import { useEffect, useState } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { Check, X } from 'lucide-react';
import { C, fonts } from '@/lib/design';
import { MarkdownCodeEditor } from './MarkdownCodeEditor';

/**
 * In-page markdown editor with GitHub-style Write / Preview tabs.
 * Edits the raw page body (including the frontmatter block that defines
 * title/summary); preview renders the body without frontmatter.
 */
export function PageEditor({
  initialBody,
  stripFrontmatter,
  onSave,
  onCancel,
}: {
  initialBody: string;
  /** Same body→rendered-markdown transform the page view uses. */
  stripFrontmatter: (body: string) => string;
  onSave: (body: string) => Promise<void>;
  onCancel: () => void;
}) {
  const [body, setBody] = useState(initialBody);
  const [tab, setTab] = useState<'write' | 'preview'>('write');
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const dirty = body !== initialBody;

  // Cmd/Ctrl+S saves, Esc cancels.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 's') {
        // When focus is inside the editor, CodeMirror's own Mod-s keymap
        // already triggers the save — skip here to avoid a double save.
        if ((e.target as HTMLElement | null)?.closest('.cm-editor')) return;
        e.preventDefault();
        void handleSave();
      } else if (e.key === 'Escape') {
        onCancel();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [body, saving]);

  const handleSave = async () => {
    if (saving) return;
    setSaving(true);
    setError(null);
    try {
      await onSave(body);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to save');
      setSaving(false);
      return;
    }
    setSaving(false);
  };

  const tabBtn = (key: 'write' | 'preview', label: string) => (
    <button
      onClick={() => setTab(key)}
      style={{
        padding: '7px 16px', fontSize: 13, fontWeight: tab === key ? 600 : 450,
        color: tab === key ? C.ink : C.muted, background: tab === key ? C.panel : 'transparent',
        border: 'none', borderBottom: tab === key ? `2px solid ${C.accent}` : '2px solid transparent',
        cursor: 'pointer', marginBottom: -1,
      }}
    >
      {label}
    </button>
  );

  return (
    <div style={{ border: `1px solid ${C.line}`, borderRadius: 12, overflow: 'hidden', background: C.panel }}>
      {/* Tab bar */}
      <div style={{
        display: 'flex', alignItems: 'center', background: C.bg,
        borderBottom: `1px solid ${C.line}`, padding: '0 8px',
      }}>
        {tabBtn('write', 'Write')}
        {tabBtn('preview', 'Preview')}
        <div style={{ marginLeft: 'auto', display: 'flex', alignItems: 'center', gap: 8, padding: '6px 8px' }}>
          {error && <span style={{ fontSize: 12.5, color: C.red, maxWidth: 380, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{error}</span>}
          <button
            onClick={onCancel}
            disabled={saving}
            style={{
              display: 'flex', alignItems: 'center', gap: 5, padding: '6px 12px', borderRadius: 7,
              fontSize: 12.5, fontWeight: 550, color: C.ink2, background: '#fff',
              border: `1px solid ${C.line}`, cursor: 'pointer',
            }}
          >
            <X size={13} /> Cancel
          </button>
          <button
            onClick={handleSave}
            disabled={saving || !dirty}
            style={{
              display: 'flex', alignItems: 'center', gap: 5, padding: '6px 14px', borderRadius: 7,
              fontSize: 12.5, fontWeight: 600, color: '#fff',
              background: dirty ? C.accent : C.faint,
              border: 'none', cursor: dirty ? 'pointer' : 'default',
              opacity: saving ? 0.6 : 1,
            }}
          >
            <Check size={13} /> {saving ? 'Saving…' : 'Save'}
          </button>
        </div>
      </div>

      {/* Body */}
      {tab === 'write' ? (
        <MarkdownCodeEditor
          value={body}
          onChange={setBody}
          onSubmit={() => { void handleSave(); }}
        />
      ) : (
        <div className="prose" style={{ padding: '16px 24px', minHeight: 460 }}>
          <ReactMarkdown remarkPlugins={[remarkGfm]}>{stripFrontmatter(body)}</ReactMarkdown>
        </div>
      )}

      {/* Footer hint */}
      <div style={{
        padding: '7px 14px', borderTop: `1px solid ${C.lineSoft}`, background: C.bg,
        fontSize: 11.5, color: C.faint, display: 'flex', gap: 14,
      }}>
        <span>Markdown supported</span>
        <span>The top <code style={{ fontFamily: fonts.mono }}>---</code> frontmatter block sets the page title & summary</span>
        <span style={{ marginLeft: 'auto' }}>⌘S to save · Esc to cancel</span>
      </div>
    </div>
  );
}

export default PageEditor;
