import { EditorView } from '@codemirror/view';
import { HighlightStyle, syntaxHighlighting } from '@codemirror/language';
import { tags as t } from '@lezer/highlight';
import { C, fonts } from '@/lib/design';

/**
 * Editor chrome + live-preview line styles, mirroring the `.prose`
 * typography of the read view so edit mode is visually seamless.
 */
export const editorTheme = EditorView.theme({
  '&': {
    fontSize: '14px',
    color: C.ink,
    backgroundColor: 'transparent',
    height: '100%',
  },
  '&.cm-focused': { outline: 'none' },
  '.cm-scroller': {
    fontFamily: fonts.sans,
    lineHeight: '1.72',
    overflow: 'auto',
  },
  '.cm-content': {
    padding: '36px 48px 56px 56px',
    caretColor: C.ink,
    maxWidth: '820px',
  },
  '.cm-line': { padding: '0.08rem 0' },

  // Headings — match .prose h1/h2/h3
  '.cm-lp-h1': {
    fontFamily: fonts.serif, fontSize: '2.25rem', fontWeight: '700',
    lineHeight: '1.25', paddingTop: '1.1rem', paddingBottom: '0.35rem',
  },
  '.cm-lp-h2': {
    fontFamily: fonts.serif, fontSize: '1.5rem', fontWeight: '600',
    lineHeight: '1.3', paddingTop: '1.15rem', paddingBottom: '0.4rem',
  },
  '.cm-lp-h3': {
    fontSize: '1.125rem', fontWeight: '600',
    paddingTop: '0.9rem', paddingBottom: '0.25rem',
  },
  '.cm-lp-h4, .cm-lp-h5, .cm-lp-h6': {
    fontSize: '1rem', fontWeight: '600',
    paddingTop: '0.7rem', paddingBottom: '0.2rem',
  },

  // Blockquote
  '.cm-lp-quote': {
    borderLeft: `3px solid ${C.line}`,
    paddingLeft: '1rem',
    color: C.ink2,
  },

  // Fenced code blocks
  '.cm-lp-code': {
    backgroundColor: C.sidebar,
    fontFamily: fonts.mono,
    fontSize: '0.85em',
    lineHeight: '1.6',
    padding: '0 1rem',
  },
  '.cm-lp-code-first': { borderRadius: '6px 6px 0 0', paddingTop: '0.5rem' },
  '.cm-lp-code-last': { borderRadius: '0 0 6px 6px', paddingBottom: '0.5rem' },

  // Frontmatter
  '.cm-lp-frontmatter': {
    fontFamily: fonts.mono,
    fontSize: '0.8em',
    color: C.muted,
    lineHeight: '1.6',
  },

  // Tables stay source-mode; mono keeps the pipes aligned
  '.cm-lp-table': { fontFamily: fonts.mono, fontSize: '0.85em', lineHeight: '1.6' },

  // Inline widgets
  '.cm-lp-bullet': { color: C.faint, display: 'inline-block', width: '1em' },
  '.cm-lp-hr': {
    display: 'inline-block', width: '100%', verticalAlign: 'middle',
    borderTop: `1px solid ${C.line}`,
  },
  '.cm-lp-checkbox': {
    accentColor: C.accent,
    width: '14px', height: '14px',
    verticalAlign: 'middle',
    margin: '0 0.45em 0.2em 0',
    cursor: 'pointer',
  },
  '.cm-lp-image': {
    maxWidth: '100%', maxHeight: '480px',
    borderRadius: '6px', display: 'block', margin: '0.4rem 0',
  },

  // Links
  '.cm-lp-link': {
    textDecoration: 'underline', textUnderlineOffset: '2px',
    cursor: 'pointer',
  },
  '.cm-lp-wikilink': {
    color: C.accent,
    textDecoration: 'underline', textUnderlineOffset: '2px',
    textDecorationColor: 'rgba(226, 89, 11, 0.4)',
    cursor: 'pointer',
  },

  // Autocomplete popup
  '.cm-tooltip.cm-tooltip-autocomplete': {
    border: `1px solid ${C.line}`,
    borderRadius: '8px',
    backgroundColor: C.panel,
    boxShadow: '0 8px 28px rgba(29, 28, 26, 0.12)',
    overflow: 'hidden',
  },
  '.cm-tooltip-autocomplete ul li': { padding: '3px 10px', fontSize: '13px' },
  '.cm-tooltip-autocomplete ul li[aria-selected]': {
    backgroundColor: C.accentSoft, color: C.ink,
  },
});

/** Inline markdown + code-token colors (GitHub-light-ish, warmed to match). */
export const editorHighlighting = syntaxHighlighting(HighlightStyle.define([
  { tag: t.strong, fontWeight: '700' },
  { tag: t.emphasis, fontStyle: 'italic' },
  { tag: t.strikethrough, textDecoration: 'line-through' },
  {
    tag: t.monospace,
    fontFamily: fonts.mono, fontSize: '0.85em',
    color: C.accent, backgroundColor: C.rail,
    borderRadius: '3px', padding: '1px 4px',
  },
  { tag: t.link, textDecoration: 'underline', textUnderlineOffset: '2px' },
  { tag: t.url, color: C.muted },
  // Markdown punctuation (visible only near the cursor)
  { tag: t.processingInstruction, color: C.faint },
  { tag: t.meta, color: C.muted },
  { tag: t.contentSeparator, color: C.faint },
  { tag: t.quote, color: C.ink2 },
  { tag: t.labelName, color: C.blue },
  // Code tokens inside fenced blocks
  { tag: t.keyword, color: '#cf222e' },
  { tag: [t.string, t.special(t.string)], color: '#0a3069' },
  { tag: t.comment, color: '#6e7781', fontStyle: 'italic' },
  { tag: [t.number, t.bool, t.atom], color: '#0550ae' },
  { tag: [t.typeName, t.className, t.namespace], color: '#953800' },
  { tag: [t.function(t.variableName), t.function(t.propertyName)], color: '#8250df' },
  { tag: t.propertyName, color: '#0550ae' },
  { tag: t.operator, color: '#cf222e' },
  { tag: t.tagName, color: '#116329' },
  { tag: t.attributeName, color: '#0550ae' },
  { tag: t.heading, fontWeight: '600' },
]));
