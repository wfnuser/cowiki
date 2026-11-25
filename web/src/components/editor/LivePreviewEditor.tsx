import { forwardRef, useEffect, useImperativeHandle, useRef } from 'react';
import { EditorView, keymap, drawSelection } from '@codemirror/view';
import { EditorState, Prec } from '@codemirror/state';
import { history, defaultKeymap, historyKeymap, indentWithTab, simplifySelection } from '@codemirror/commands';
import { indentUnit } from '@codemirror/language';
import { markdown, markdownLanguage, markdownKeymap } from '@codemirror/lang-markdown';
import { languages } from '@codemirror/language-data';
import { autocompletion, completionStatus } from '@codemirror/autocomplete';
import type { CompletionContext, CompletionResult } from '@codemirror/autocomplete';
import { caretAfterMarkup, livePreview, livePreviewConfig, livePreviewTables } from './livePreview';
import { WikiLink, Frontmatter } from './markdownExtensions';
import { editorTheme, editorHighlighting } from './theme';

export interface LivePreviewEditorProps {
  initialDoc: string;
  /** Fired on every document change with the full new text. */
  onDocChanged: (doc: string, source: 'human' | 'external') => void;
  /** Cmd/Ctrl+S — the host flushes its pending save. */
  onRequestSave?: () => void;
  onWikilink?: (target: string) => void;
  /** Resolve a local attachment to a displayable URL. */
  loadImage?: (src: string) => Promise<string>;
  /** Candidate slugs for `[[` autocompletion. */
  getPageSlugs?: () => string[];
  /** Caret position when the editor opens; it starts at the top otherwise. */
  initialCursor?: number;
  /** Keep the caret's line this far below the top of the editor, as it was when clicked. */
  initialCursorTop?: number;
  /** Focus left the editor for another part of the page (not another window). */
  onBlur?: () => void;
  /** Esc with nothing to close or deselect: stop editing. */
  onEscape?: () => void;
  className?: string;
  style?: React.CSSProperties;
}

export interface LivePreviewEditorHandle {
  /** Replace the open document synchronously after host-side revision checks. */
  replaceDocument: (doc: string) => void;
  /** Put the caret at the end of the document (a click below the last line). */
  focusEnd: () => void;
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
  loadImage,
  getPageSlugs,
  initialCursor,
  initialCursorTop,
  onBlur,
  onEscape,
  className,
  style,
}, ref) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const viewRef = useRef<EditorView | null>(null);
  const applyingExternalDoc = useRef(false);

  // Keep callbacks fresh without recreating the editor.
  const callbacks = useRef({ onDocChanged, onRequestSave, onWikilink, loadImage, getPageSlugs, onBlur, onEscape });
  useEffect(() => {
    callbacks.current = { onDocChanged, onRequestSave, onWikilink, loadImage, getPageSlugs, onBlur, onEscape };
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

    const anchor = initialCursor === undefined
      ? undefined
      : Math.max(0, Math.min(initialCursor, initialDoc.length));
    const view = new EditorView({
      parent: hostRef.current,
      state: EditorState.create({
        doc: initialDoc,
        selection: anchor === undefined ? undefined : { anchor },
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
          livePreviewTables,
          livePreview,
          caretAfterMarkup,
          livePreviewConfig.of({
            onWikilink: (target) => callbacks.current.onWikilink?.(target),
            loadImage: (src) => (callbacks.current.loadImage
              ? callbacks.current.loadImage(src)
              : Promise.reject(new Error('Attachments are not available here'))),
          }),
          // Esc: an open completion menu closes first, then a selection
          // collapses, then editing stops. A completion that is only pending
          // (nothing shown yet) must not swallow the key.
          Prec.highest(keymap.of([{
            key: 'Escape',
            run: (target) => {
              if (completionStatus(target.state) === 'active') return false;
              if (simplifySelection(target)) return true;
              if (!callbacks.current.onEscape) return false;
              callbacks.current.onEscape();
              return true;
            },
          }])),
          autocompletion({ override: [wikilinkCompletion], icons: false }),
          Prec.high(keymap.of([
            {
              key: 'Mod-s',
              run: () => { callbacks.current.onRequestSave?.(); return true; },
            },
          ])),
          keymap.of([...markdownKeymap, ...defaultKeymap, ...historyKeymap, indentWithTab]),
          EditorView.domEventHandlers({
            blur: () => {
              // Switching to another window is not leaving the editor.
              // A destroyed editor (an unmount) also blurs; only the live one counts.
              window.setTimeout(() => {
                if (viewRef.current === view && !view.hasFocus && document.hasFocus()) callbacks.current.onBlur?.();
              }, 0);
            },
          }),
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
    if (anchor !== undefined) {
      view.dispatch({
        effects: EditorView.scrollIntoView(anchor, initialCursorTop === undefined
          ? { y: 'center' }
          : { y: 'start', yMargin: Math.max(0, initialCursorTop) }),
      });
    }

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
    focusEnd: () => {
      const view = viewRef.current;
      if (!view) return;
      view.focus();
      view.dispatch({ selection: { anchor: view.state.doc.length } });
    },
  }), []);

  return <div ref={hostRef} className={className} style={{ height: '100%', ...style }} />;
});

export default LivePreviewEditor;
