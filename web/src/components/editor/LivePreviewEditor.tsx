import { forwardRef, useEffect, useImperativeHandle, useRef } from 'react';
import { EditorView, keymap, drawSelection } from '@codemirror/view';
import { EditorState, Prec } from '@codemirror/state';
import { history, defaultKeymap, historyKeymap, indentWithTab } from '@codemirror/commands';
import { indentUnit } from '@codemirror/language';
import { markdown, markdownLanguage, markdownKeymap } from '@codemirror/lang-markdown';
import { languages } from '@codemirror/language-data';
import { autocompletion } from '@codemirror/autocomplete';
import type { CompletionContext, CompletionResult } from '@codemirror/autocomplete';
import { livePreview, livePreviewConfig } from './livePreview';
import { WikiLink, Frontmatter } from './markdownExtensions';
import { editorTheme, editorHighlighting } from './theme';

export interface LivePreviewEditorProps {
  initialDoc: string;
  /** Fired on every document change with the full new text. */
  onDocChanged: (doc: string, source: 'human' | 'external') => void;
  /** Cmd/Ctrl+S — the host flushes its pending save. */
  onRequestSave?: () => void;
  onWikilink?: (target: string) => void;
  /** Candidate slugs for `[[` autocompletion. */
  getPageSlugs?: () => string[];
  className?: string;
  style?: React.CSSProperties;
}

export interface LivePreviewEditorHandle {
  /** Replace the open document synchronously after host-side revision checks. */
  replaceDocument: (doc: string) => void;
}

/**
 * Obsidian-style live-preview markdown editor: markdown source with
 * formatting rendered in place; syntax marks reveal themselves around
 * the cursor.
 */
export const LivePreviewEditor = forwardRef<LivePreviewEditorHandle, LivePreviewEditorProps>(function LivePreviewEditor({
  initialDoc,
  onDocChanged,
  onRequestSave,
  onWikilink,
  getPageSlugs,
  className,
  style,
}, ref) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const viewRef = useRef<EditorView | null>(null);
  const applyingExternalDoc = useRef(false);

  // Keep callbacks fresh without recreating the editor.
  const callbacks = useRef({ onDocChanged, onRequestSave, onWikilink, getPageSlugs });
  useEffect(() => {
    callbacks.current = { onDocChanged, onRequestSave, onWikilink, getPageSlugs };
  });

  useEffect(() => {
    if (!hostRef.current) return;

    const wikilinkCompletion = (ctx: CompletionContext): CompletionResult | null => {
      const match = ctx.matchBefore(/\[\[([^\][|]*)$/);
      if (!match) return null;
      const slugs = callbacks.current.getPageSlugs?.() ?? [];
      if (slugs.length === 0) return null;
      return {
        from: match.from + 2,
        options: slugs.map((slug) => ({
          label: slug,
          type: 'text',
          apply: (view, _completion, from, to) => {
            const after = view.state.sliceDoc(to, to + 2);
            const insert = slug + (after === ']]' ? '' : ']]');
            view.dispatch({
              changes: { from, to, insert },
              selection: { anchor: from + insert.length + (after === ']]' ? 2 : 0) },
            });
          },
        })),
        validFor: /^[^\][|]*$/,
      };
    };

    const view = new EditorView({
      parent: hostRef.current,
      state: EditorState.create({
        doc: initialDoc,
        extensions: [
          history(),
          drawSelection(),
          EditorView.lineWrapping,
          indentUnit.of('  '),
          markdown({
            base: markdownLanguage,
            codeLanguages: languages,
            extensions: [WikiLink, Frontmatter],
          }),
          editorTheme,
          editorHighlighting,
          livePreview,
          livePreviewConfig.of({
            onWikilink: (target) => callbacks.current.onWikilink?.(target),
          }),
          autocompletion({ override: [wikilinkCompletion], icons: false }),
          Prec.high(keymap.of([
            {
              key: 'Mod-s',
              run: () => { callbacks.current.onRequestSave?.(); return true; },
            },
          ])),
          keymap.of([...markdownKeymap, ...defaultKeymap, ...historyKeymap, indentWithTab]),
          EditorView.updateListener.of((update) => {
            if (update.docChanged) {
              callbacks.current.onDocChanged(
                update.state.doc.toString(),
                applyingExternalDoc.current ? 'external' : 'human',
              );
            }
          }),
        ],
      }),
    });
    viewRef.current = view;
    view.focus();

    return () => {
      view.destroy();
      viewRef.current = null;
    };
    // The editor is created once per mount; remount (via key) to reset the doc.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useImperativeHandle(ref, () => ({
    // This dispatch is synchronous. A human key event cannot land between the
    // host's revision check and CodeMirror receiving the accepted agent edit.
    replaceDocument: (doc: string) => {
      const view = viewRef.current;
      if (!view || doc === view.state.doc.toString()) return;

      applyingExternalDoc.current = true;
      try {
        view.dispatch({
          changes: { from: 0, to: view.state.doc.length, insert: doc },
          selection: { anchor: Math.min(view.state.selection.main.head, doc.length) },
        });
      } finally {
        applyingExternalDoc.current = false;
      }
    },
  }), []);

  return <div ref={hostRef} className={className} style={{ height: '100%', ...style }} />;
});

export default LivePreviewEditor;
