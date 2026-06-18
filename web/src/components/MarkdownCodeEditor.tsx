import { useMemo, useRef } from 'react';
import CodeMirror, { type ReactCodeMirrorRef } from '@uiw/react-codemirror';
import { markdown, markdownLanguage } from '@codemirror/lang-markdown';
import { languages } from '@codemirror/language-data';
import { EditorView, keymap } from '@codemirror/view';
import { EditorSelection } from '@codemirror/state';
import {
  Bold, Italic, Heading, List, ListOrdered, Link as LinkIcon, Code, Quote,
} from 'lucide-react';
import { C, fonts } from '@/lib/design';

/** Wrap each selection in `before … after` (bold/italic/inline-code). */
function surround(view: EditorView, before: string, after = before) {
  const tr = view.state.changeByRange((range) => {
    const text = view.state.sliceDoc(range.from, range.to);
    const anchor = range.from + before.length;
    return {
      changes: { from: range.from, to: range.to, insert: before + text + after },
      range: EditorSelection.range(anchor, anchor + text.length),
    };
  });
  view.dispatch(tr);
  view.focus();
}

/** Prepend `prefix` to every line the selection touches (headings/lists/quote). */
function linePrefix(view: EditorView, prefix: string) {
  const { state } = view;
  const changes: { from: number; insert: string }[] = [];
  const seen = new Set<number>();
  for (const range of state.selection.ranges) {
    const first = state.doc.lineAt(range.from).number;
    const last = state.doc.lineAt(range.to).number;
    for (let n = first; n <= last; n++) {
      if (seen.has(n)) continue;
      seen.add(n);
      changes.push({ from: state.doc.line(n).from, insert: prefix });
    }
  }
  view.dispatch({ changes });
  view.focus();
}

/** Insert a markdown link around the selection, cursor landing on the URL. */
function insertLink(view: EditorView) {
  const tr = view.state.changeByRange((range) => {
    const text = view.state.sliceDoc(range.from, range.to) || 'text';
    const urlFrom = range.from + 1 + text.length + 2;
    return {
      changes: { from: range.from, to: range.to, insert: `[${text}](url)` },
      range: EditorSelection.range(urlFrom, urlFrom + 3),
    };
  });
  view.dispatch(tr);
  view.focus();
}

const TOOLS: { icon: typeof Bold; title: string; run: (view: EditorView) => void }[] = [
  { icon: Bold, title: 'Bold (⌘B)', run: (v) => surround(v, '**') },
  { icon: Italic, title: 'Italic (⌘I)', run: (v) => surround(v, '_') },
  { icon: Code, title: 'Inline code', run: (v) => surround(v, '`') },
  { icon: Heading, title: 'Heading', run: (v) => linePrefix(v, '## ') },
  { icon: List, title: 'Bullet list', run: (v) => linePrefix(v, '- ') },
  { icon: ListOrdered, title: 'Numbered list', run: (v) => linePrefix(v, '1. ') },
  { icon: Quote, title: 'Quote', run: (v) => linePrefix(v, '> ') },
  { icon: LinkIcon, title: 'Link', run: insertLink },
];

/**
 * Markdown source editor built on CodeMirror 6.
 *
 * Edits the literal markdown bytes (no WYSIWYG round-trip), so diffs stay
 * exactly the user's change — important for cowiki's provenance model.
 * Adds syntax highlighting, a formatting toolbar, soft-wrap and ⌘S.
 */
export function MarkdownCodeEditor({
  value,
  onChange,
  onSubmit,
}: {
  value: string;
  onChange: (next: string) => void;
  /** ⌘S inside the editor. */
  onSubmit?: () => void;
}) {
  const ref = useRef<ReactCodeMirrorRef>(null);

  // Keep the latest onSubmit reachable from the keymap without making the
  // extensions array depend on it — otherwise the new closure PageEditor
  // creates each render would rebuild every extension on every keystroke.
  const onSubmitRef = useRef(onSubmit);
  onSubmitRef.current = onSubmit;

  const extensions = useMemo(
    () => [
      markdown({ base: markdownLanguage, codeLanguages: languages }),
      EditorView.lineWrapping,
      keymap.of([
        { key: 'Mod-b', preventDefault: true, run: (v) => { surround(v, '**'); return true; } },
        { key: 'Mod-i', preventDefault: true, run: (v) => { surround(v, '_'); return true; } },
        { key: 'Mod-s', preventDefault: true, run: () => { onSubmitRef.current?.(); return true; } },
      ]),
      EditorView.theme({
        '&': { fontSize: '13.5px', backgroundColor: C.panel, color: C.ink },
        '&.cm-focused': { outline: 'none' },
        '.cm-scroller': { fontFamily: fonts.mono, lineHeight: '1.65', minHeight: '460px' },
        '.cm-content': { fontFamily: fonts.mono, padding: '16px 20px', caretColor: C.accent },
        '.cm-cursor, .cm-dropCursor': { borderLeftColor: C.accent },
        '.cm-selectionBackground, &.cm-focused .cm-selectionBackground, ::selection': { backgroundColor: C.accentSoft },
        '.cm-gutters': { backgroundColor: C.bg, color: C.faint, border: 'none' },
      }),
    ],
    [],
  );

  return (
    <div>
      {/* Formatting toolbar */}
      <div style={{
        display: 'flex', alignItems: 'center', gap: 2, padding: '5px 10px',
        borderBottom: `1px solid ${C.lineSoft}`, background: C.bg,
      }}>
        {TOOLS.map(({ icon: Icon, title, run }) => (
          <button
            key={title}
            type="button"
            title={title}
            onMouseDown={(e) => {
              e.preventDefault();
              const view = ref.current?.view;
              if (view) run(view);
            }}
            style={{
              display: 'flex', alignItems: 'center', justifyContent: 'center',
              width: 28, height: 28, borderRadius: 6, border: 'none',
              background: 'transparent', color: C.ink2, cursor: 'pointer',
            }}
            onMouseEnter={(e) => { e.currentTarget.style.background = C.tag; }}
            onMouseLeave={(e) => { e.currentTarget.style.background = 'transparent'; }}
          >
            <Icon size={15} />
          </button>
        ))}
      </div>

      <CodeMirror
        ref={ref}
        value={value}
        onChange={onChange}
        autoFocus
        basicSetup={{
          lineNumbers: false,
          foldGutter: false,
          highlightActiveLine: false,
          highlightActiveLineGutter: false,
          autocompletion: false,
        }}
        extensions={extensions}
      />
    </div>
  );
}

export default MarkdownCodeEditor;
