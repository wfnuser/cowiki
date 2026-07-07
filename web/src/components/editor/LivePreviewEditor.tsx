import { useEffect, useRef } from 'react';
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
  onDocChanged: (doc: string) => void;
  /** Cmd/Ctrl+S — the host flushes its pending save. */
  onRequestSave?: () => void;
  onWikilink?: (target: string) => void;
  /** Candidate slugs for `[[` autocompletion. */
  getPageSlugs?: () => string[];
  className?: string;
  style?: React.CSSProperties;
}

/**
 * Obsidian-style live-preview markdown editor: markdown source with
 * formatting rendered in place; syntax marks reveal themselves around
 * the cursor.
 */
export function LivePreviewEditor({
  initialDoc,
  onDocChanged,
  onRequestSave,
  onWikilink,
  getPageSlugs,
  className,
  style,
}: LivePreviewEditorProps) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const viewRef = useRef<EditorView | null>(null);

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
            if (update.docChanged) callbacks.current.onDocChanged(update.state.doc.toString());
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

  return <div ref={hostRef} className={className} style={{ height: '100%', ...style }} />;
}

export default LivePreviewEditor;
