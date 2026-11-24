import { EditorView } from '@codemirror/view';
import { HighlightStyle, syntaxHighlighting } from '@codemirror/language';
import { tags as t } from '@lezer/highlight';
import { C, fonts, shadows } from '@/lib/design';

/**
 * Editor chrome + live-preview line styles. Sizes come from the `--doc-*`
 * tokens in index.css that the reader's `.prose` uses too, so a line sits at
 * the same place whether the page is read or edited. The host lays out the
 * page (padding, width); the editor adds no padding of its own.
 */
const heading = (level: 1 | 2 | 3 | 4) => ({
  fontSize: `var(--doc-h${level}-size)`,
  lineHeight: `var(--doc-h${level}-line)`,
  // A heading's space above, less the blank line or gap that comes before it.
  paddingTop: `calc(var(--doc-h${level}-above) - var(--doc-gap))`,
});

export const editorTheme = EditorView.theme({
  '&': {
    fontSize: 'var(--doc-size)',
    color: C.ink,
    backgroundColor: 'transparent',
    height: '100%',
  },
  '&.cm-focused': { outline: 'none' },
  '.cm-scroller': {
    fontFamily: fonts.sans,
    lineHeight: 'var(--doc-line)',
    overflow: 'auto',
  },
  '.cm-content': {
    padding: '0',
    caretColor: C.ink,
  },
  '.cm-line': { padding: '0' },

  // A blank line between blocks is the reader's gap between them; a block
  // written right under another gets the same gap above it.
  '.cm-lp-blank': { lineHeight: 'var(--doc-gap)', fontSize: '10px' },
  // CodeMirror's caret helpers beside hidden markup must not make a line taller.
  '.cm-widgetBuffer': { verticalAlign: 'middle' },
  // A rule is the reader's: 24px above and below a 1px line (blank lines give 12 each).
  '.cm-lp-rule': { lineHeight: '25px' },
  '.cm-lp-tight': { paddingTop: 'var(--doc-gap)' },

  // Headings — the same sizes as .prose h1…h6
  '.cm-lp-h1': { ...heading(1), fontFamily: 'var(--doc-display-font)', fontWeight: '700', letterSpacing: '-0.01em' },
  '.cm-lp-h2': { ...heading(2), fontFamily: 'var(--doc-display-font)', fontWeight: '600' },
  '.cm-lp-h3': { ...heading(3), fontWeight: '600' },
  '.cm-lp-h4, .cm-lp-h5, .cm-lp-h6': { ...heading(4), fontWeight: '600' },
  '.cm-lp-h1.cm-lp-tight': { paddingTop: 'var(--doc-h1-above)' },
  '.cm-lp-h2.cm-lp-tight': { paddingTop: 'var(--doc-h2-above)' },
  '.cm-lp-h3.cm-lp-tight': { paddingTop: 'var(--doc-h3-above)' },
  '.cm-lp-h4.cm-lp-tight, .cm-lp-h5.cm-lp-tight, .cm-lp-h6.cm-lp-tight': { paddingTop: 'var(--doc-h4-above)' },
  '.cm-line.cm-lp-first': { paddingTop: '0' },
  // `## ` shows in the margin while the caret is on the heading, so the
  // heading's text does not move.
  '.cm-lp-hmark': {
    position: 'absolute',
    transform: 'translateX(-100%)',
    paddingRight: '0.25em',
    color: C.faint,
    fontWeight: '400',
  },

  // Blockquote
  '.cm-lp-quote': {
    borderLeft: `3px solid ${C.line}`,
    paddingLeft: '1rem',
    color: C.ink2,
  },
  // Callouts: the reader's rule colour and title (index.css, "Callouts").
  '.cm-lp-quote:is(.cm-lp-callout-note, .cm-lp-callout-tip, .cm-lp-callout-warning, .cm-lp-callout-danger)': {
    borderLeftColor: 'color-mix(in srgb, var(--callout) 70%, transparent)',
  },
  '.cm-lp-callout-title': { display: 'inline' },
  '.cm-lp-callout-mark': { color: C.faint, fontWeight: '400' },

  // Fenced code blocks: the fence lines are the block's top and bottom
  // padding; their text shows only while the caret is inside.
  '.cm-lp-code': {
    backgroundColor: C.sidebar,
    fontFamily: fonts.mono,
    fontSize: 'var(--doc-code-size)',
    lineHeight: 'var(--doc-code-line)',
    paddingLeft: '16px',
    paddingRight: '16px',
  },
  // Keep a code block's gap above outside its background.
  '.cm-lp-code.cm-lp-tight': {
    paddingTop: '0',
    borderTop: 'var(--doc-gap) solid transparent',
    backgroundClip: 'padding-box',
  },
  '.cm-lp-fence': { lineHeight: 'var(--doc-code-pad)', color: 'transparent' },
  '.cm-lp-fence.cm-lp-fence-shown': { color: C.faint },
  '.cm-lp-fence:not(.cm-lp-fence-shown) span': { color: 'transparent' },
  '.cm-lp-fence-open': { borderRadius: '6px 6px 0 0' },
  '.cm-lp-fence-close': { borderRadius: '0 0 6px 6px' },

  // Frontmatter
  '.cm-lp-frontmatter': {
    fontFamily: fonts.mono,
    fontSize: '0.8em',
    color: C.muted,
    lineHeight: '1.6',
  },

  // Tables stay source-mode; mono keeps the pipes aligned
  '.cm-lp-table': { fontFamily: fonts.mono, fontSize: '0.85em', lineHeight: '1.6' },

  // List markers fill one --doc-indent, like the reader's (see .prose li).
  '.cm-lp-bullet': {
    display: 'inline-block',
    position: 'relative',
    width: 'var(--doc-indent)',
    height: 'calc(var(--doc-line) * 1em)',
    verticalAlign: 'top',
    textIndent: '0',
  },
  '.cm-lp-bullet::before': {
    content: '""',
    position: 'absolute',
    left: '9px',
    top: 'calc(var(--doc-line) * 0.5em - 2.5px)',
    width: '5px',
    height: '5px',
    borderRadius: '50%',
    background: 'var(--color-text-tertiary)',
  },
  '.cm-lp-ol-mark': {
    display: 'inline-block',
    width: 'var(--doc-indent)',
    paddingRight: '0.3em',
    boxSizing: 'border-box',
    textAlign: 'right',
    textIndent: '0',
    color: C.ink2,
    fontVariantNumeric: 'tabular-nums',
    whiteSpace: 'nowrap',
  },
  '.cm-lp-ol-mark span': { color: 'inherit' },
  '.cm-lp-hr': {
    display: 'inline-block', width: '100%', verticalAlign: 'middle',
    borderTop: `1px solid ${C.line}`,
  },
  '.cm-lp-checkbox': {
    width: '15px', height: '15px',
    verticalAlign: 'top',
    margin: 'calc((var(--doc-line) * 1em - 15px) / 2) 7px 0 2px',
    cursor: 'pointer',
  },
  '.cm-lp-image': {
    maxWidth: '100%', maxHeight: '480px',
    borderRadius: '6px', display: 'block', margin: '0.4rem 0',
  },

  // Links
  // Same as the reader's links (index.css, ".prose a").
  '.cm-lp-link, .cm-lp-wikilink': {
    textDecoration: 'underline 1px', textUnderlineOffset: '3px',
    textDecorationColor: 'color-mix(in srgb, currentColor 30%, transparent)',
    cursor: 'pointer',
  },
  '.cm-lp-wikilink': { color: C.accent },

  // Autocomplete popup
  '.cm-tooltip.cm-tooltip-autocomplete': {
    border: `1px solid ${C.line}`,
    borderRadius: '8px',
    backgroundColor: C.panel,
    boxShadow: shadows.lifted,
    overflow: 'hidden',
  },
  '.cm-tooltip-autocomplete ul li': { padding: '3px 10px', fontSize: '13px' },
  '.cm-tooltip-autocomplete ul li[aria-selected]': {
    backgroundColor: C.accentSoft, color: C.ink,
  },
});

/** Code colours from index.css (--color-syntax-*), with dark values; not the UI palette. */
const syntax = {
  keyword: 'var(--color-syntax-keyword)',
  string: 'var(--color-syntax-string)',
  comment: 'var(--color-syntax-comment)',
  literal: 'var(--color-syntax-literal)',
  type: 'var(--color-syntax-type)',
  fn: 'var(--color-syntax-fn)',
  tag: 'var(--color-syntax-tag)',
} as const;

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
  { tag: t.link, textDecoration: 'underline 1px', textUnderlineOffset: '3px' },
  { tag: t.url, color: C.muted },
  // Markdown punctuation (visible only near the cursor)
  { tag: t.processingInstruction, color: C.faint },
  { tag: t.meta, color: C.muted },
  { tag: t.contentSeparator, color: C.faint },
  { tag: t.quote, color: C.ink2 },
  { tag: t.labelName, color: C.blue },
  { tag: t.keyword, color: syntax.keyword },
  { tag: [t.string, t.special(t.string)], color: syntax.string },
  { tag: t.comment, color: syntax.comment, fontStyle: 'italic' },
  { tag: [t.number, t.bool, t.atom], color: syntax.literal },
  { tag: [t.typeName, t.className, t.namespace], color: syntax.type },
  { tag: [t.function(t.variableName), t.function(t.propertyName)], color: syntax.fn },
  { tag: t.propertyName, color: syntax.literal },
  { tag: t.operator, color: syntax.keyword },
  { tag: t.tagName, color: syntax.tag },
  { tag: t.attributeName, color: syntax.literal },
  // Weight comes from the heading's line (.cm-lp-h1…), as in the reader.
  { tag: t.heading, fontWeight: 'inherit' },
]));
