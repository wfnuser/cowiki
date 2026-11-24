import { Decoration, EditorView, ViewPlugin, WidgetType } from '@codemirror/view';
import type { DecorationSet, ViewUpdate } from '@codemirror/view';
import { EditorSelection, EditorState, Facet } from '@codemirror/state';
import type { Range } from '@codemirror/state';
import { syntaxTree } from '@codemirror/language';
import type { SyntaxNode } from '@lezer/common';

import { CALLOUT_LINE, calloutOf } from '@/lib/callouts';

/** Editor-level callbacks the host app provides (wikilink navigation). */
export interface LivePreviewConfig {
  onWikilink?: (target: string) => void;
  /** Resolve a local attachment (relative path or `![[name]]`) to a displayable URL. */
  loadImage?: (src: string) => Promise<string>;
}

const REMOTE_IMAGE = /^(https?:|data:|blob:)/i;
const IMAGE_EXTENSION = /\.(png|jpe?g|gif|webp|svg|bmp|avif|ico|tiff?)$/i;

export const livePreviewConfig = Facet.define<LivePreviewConfig, LivePreviewConfig>({
  combine: (values) => values[0] ?? {},
});

// ── Widgets ──────────────────────────────────────────────────────────

class BulletWidget extends WidgetType {
  toDOM() {
    const span = document.createElement('span');
    span.className = 'cm-lp-bullet';
    span.setAttribute('aria-hidden', 'true');
    return span;
  }
  eq() { return true; }
}

class CheckboxWidget extends WidgetType {
  checked: boolean;
  constructor(checked: boolean) { super(); this.checked = checked; }
  toDOM() {
    const box = document.createElement('input');
    box.type = 'checkbox';
    box.checked = this.checked;
    box.className = 'cm-lp-checkbox';
    return box;
  }
  eq(other: CheckboxWidget) { return other.checked === this.checked; }
  ignoreEvent() { return false; }
}

/** A callout's icon, the same mask the reader draws (index.css, ".callout-icon"). */
class CalloutIconWidget extends WidgetType {
  icon: string;
  constructor(icon: string) { super(); this.icon = icon; }
  toDOM() {
    const span = document.createElement('span');
    span.className = 'callout-icon cm-lp-callout-icon';
    span.dataset.icon = this.icon;
    span.setAttribute('aria-hidden', 'true');
    return span;
  }
  eq(other: CalloutIconWidget) { return other.icon === this.icon; }
}

/** A callout written without a title shows its type ("Tip"), as the reader does. */
class CalloutTitleWidget extends WidgetType {
  title: string;
  constructor(title: string) { super(); this.title = title; }
  toDOM() {
    const span = document.createElement('span');
    span.className = 'callout-title cm-lp-callout-title';
    span.textContent = this.title;
    return span;
  }
  eq(other: CalloutTitleWidget) { return other.title === this.title; }
}

class HrWidget extends WidgetType {
  toDOM() {
    const div = document.createElement('div');
    div.className = 'cm-lp-hr';
    return div;
  }
  eq() { return true; }
}

class ImageWidget extends WidgetType {
  src: string;
  alt: string;
  constructor(src: string, alt: string) { super(); this.src = src; this.alt = alt; }
  toDOM(view: EditorView) {
    const img = document.createElement('img');
    img.alt = this.alt;
    img.className = 'cm-lp-image';
    const loadImage = view.state.facet(livePreviewConfig).loadImage;
    if (REMOTE_IMAGE.test(this.src) || !loadImage) {
      img.src = this.src;
    } else {
      img.dataset.lpPending = 'true';
      loadImage(this.src).then(
        (url) => { img.src = url; delete img.dataset.lpPending; },
        () => { img.alt = `Attachment not found: ${this.src}`; img.dataset.lpMissing = 'true'; },
      );
    }
    return img;
  }
  eq(other: ImageWidget) { return other.src === this.src && other.alt === this.alt; }
}

// ── Decoration builder ───────────────────────────────────────────────

const hide = Decoration.replace({});
const LIST = new Set(['BulletList', 'OrderedList']);
const NO_GAPS = new Set(['FencedCode', 'CodeBlock', 'Frontmatter', 'Table', 'HTMLBlock']);

interface Built {
  decorations: DecorationSet;
  /** List markers the caret steps over and Backspace removes whole. */
  atomic: DecorationSet;
}

/**
 * The editor draws a page the way the reader does (see the `--doc-*` tokens in
 * index.css): block markers never appear or move with the caret, a blank line
 * is the reader's gap between blocks, and list items hang at the same indent.
 */
function buildDecorations(view: EditorView): Built {
  const decos: Range<Decoration>[] = [];
  const atomic: Range<Decoration>[] = [];
  const { state } = view;
  const doc = state.doc;
  const tree = syntaxTree(state);

  /** True when a selection endpoint or range touches [from, to]. */
  const reveal = (from: number, to: number) =>
    state.selection.ranges.some((r) => r.from <= to && r.to >= from);
  /** Reveal across the whole line(s) the range sits on (for block markup). */
  const revealLine = (from: number, to: number) =>
    reveal(doc.lineAt(from).from, doc.lineAt(Math.min(to, doc.length)).to);

  const decoratedLines = new Set<string>();
  const lineDeco = (pos: number, className: string) => {
    const line = doc.lineAt(pos);
    const key = `${line.from}:${className}`;
    if (decoratedLines.has(key)) return;
    decoratedLines.add(key);
    decos.push(Decoration.line({ class: className }).range(line.from));
  };
  const eachLine = (from: number, to: number, fn: (lineFrom: number, first: boolean, last: boolean) => void) => {
    let pos = from;
    while (pos <= to) {
      const line = doc.lineAt(pos);
      fn(line.from, line.from <= from, line.to >= to);
      if (line.to >= to) break;
      pos = line.to + 1;
    }
  };
  const withSpace = (to: number) => (to < doc.length && state.sliceDoc(to, to + 1) === ' ' ? to + 1 : to);
  /** Hide a mark, swallowing one following space (e.g. `# `, `> `). */
  const hideMark = (from: number, to: number) => { decos.push(hide.range(from, withSpace(to))); };
  const hideMarker = (from: number, to: number) => {
    if (to <= from) return;
    decos.push(hide.range(from, to));
    atomic.push(hide.range(from, to));
  };
  const hideChildMarks = (node: SyntaxNode, markName: string) => {
    for (let child = node.firstChild; child; child = child.nextSibling) {
      if (child.name === markName) decos.push(hide.range(child.from, child.to));
    }
  };
  const inside = (node: SyntaxNode | null, names: Set<string>) => {
    for (let at = node; at; at = at.parent) if (names.has(at.name)) return true;
    return false;
  };

  // Gaps between top-level blocks: the first block has none above it; a block
  // written on the line right under the previous one gets the reader's gap.
  let previous: SyntaxNode | null = null;
  for (let block = tree.topNode.firstChild; block; block = block.nextSibling) {
    if (!previous) lineDeco(block.from, 'cm-lp-first');
    else if (doc.lineAt(block.from).number === doc.lineAt(previous.to).number + 1) lineDeco(block.from, 'cm-lp-tight');
    previous = block;
  }

  for (const { from, to } of view.visibleRanges) {
    // Blank lines, and list items hanging at their indent.
    for (let pos = from; pos <= to;) {
      const line = doc.lineAt(pos);
      const text = line.text;
      const indent = /^\s*/.exec(text)![0].length;
      const at = tree.resolveInner(line.from + indent, 1);
      if (/^\s*$/.test(text) || (/^\s*(>\s*)+$/.test(text) && inside(at, new Set(['Blockquote'])))) {
        if (!inside(at, NO_GAPS)) lineDeco(line.from, 'cm-lp-blank');
      } else if (!inside(at, NO_GAPS)) {
        let item: SyntaxNode | null = null;
        let depth = -1;
        for (let node: SyntaxNode | null = at; node; node = node.parent) {
          if (!item && node.name === 'ListItem') item = node;
          if (item && LIST.has(node.name)) depth++;
        }
        if (item) {
          const first = item.from >= line.from && item.from <= line.to;
          const style = `padding-left: calc(var(--doc-indent) * ${depth + 1})`
            + (first ? '; text-indent: calc(var(--doc-indent) * -1)' : '');
          decos.push(Decoration.line({ attributes: { style } }).range(line.from));
          hideMarker(line.from, first ? item.from : line.from + indent);
        }
      }
      if (line.to >= to) break;
      pos = line.to + 1;
    }

    tree.iterate({
      from, to,
      enter: (nodeRef) => {
        const name = nodeRef.name;
        const nFrom = nodeRef.from;
        const nTo = nodeRef.to;

        // Headings: size the whole line; `# ` hides, or moves to the margin
        // while the caret is on the heading.
        const headingLevel = name.startsWith('ATXHeading') ? Number(name.slice(10)) : 0;
        if (headingLevel) {
          lineDeco(nFrom, `cm-lp-h${headingLevel}`);
          const mark = nodeRef.node.getChild('HeaderMark');
          if (mark && mark.from === doc.lineAt(nFrom).from) {
            if (revealLine(nFrom, nTo)) decos.push(Decoration.mark({ class: 'cm-lp-hmark' }).range(mark.from, withSpace(mark.to)));
            else hideMark(mark.from, mark.to);
          }
          return;
        }
        if (name === 'SetextHeading1' || name === 'SetextHeading2') {
          lineDeco(nFrom, name === 'SetextHeading1' ? 'cm-lp-h1' : 'cm-lp-h2');
          return;
        }

        switch (name) {
          case 'Emphasis':
          case 'StrongEmphasis':
            if (!reveal(nFrom, nTo)) hideChildMarks(nodeRef.node, 'EmphasisMark');
            return;

          case 'Strikethrough':
            if (!reveal(nFrom, nTo)) hideChildMarks(nodeRef.node, 'StrikethroughMark');
            return;

          case 'InlineCode':
            if (!reveal(nFrom, nTo)) hideChildMarks(nodeRef.node, 'CodeMark');
            return;

          case 'Link': {
            if (reveal(nFrom, nTo)) return;
            // `[!tip]` opening a callout is not a link (see Blockquote).
            if (state.sliceDoc(nFrom, nFrom + 2) === '[!' && inside(nodeRef.node, new Set(['Blockquote']))) return false;
            const node = nodeRef.node;
            const marks = node.getChildren('LinkMark');
            const url = node.getChild('URL');
            // `[text](url)` → keep `text`, hide the brackets and the URL tail.
            if (marks.length >= 2) {
              const textFrom = marks[0].to;
              const textTo = marks[1].from;
              decos.push(hide.range(nFrom, textFrom));
              decos.push(hide.range(textTo, nTo));
              if (textTo > textFrom) {
                decos.push(Decoration.mark({
                  class: 'cm-lp-link',
                  attributes: url ? { 'data-lp-href': state.sliceDoc(url.from, url.to) } : {},
                }).range(textFrom, textTo));
              }
            }
            return false;
          }

          case 'Image': {
            if (revealLine(nFrom, nTo)) return;
            const node = nodeRef.node;
            const url = node.getChild('URL');
            if (url) {
              const marks = node.getChildren('LinkMark');
              const alt = marks.length >= 2 ? state.sliceDoc(marks[0].to, marks[1].from) : '';
              decos.push(Decoration.replace({
                widget: new ImageWidget(state.sliceDoc(url.from, url.to), alt),
              }).range(nFrom, nTo));
            }
            return false;
          }

          case 'WikiLink': {
            const embedded = nFrom > 0 && state.sliceDoc(nFrom - 1, nFrom) === '!';
            if (reveal(embedded ? nFrom - 1 : nFrom, nTo)) return;
            const inner = state.sliceDoc(nFrom + 2, nTo - 2);
            const pipe = inner.indexOf('|');
            const target = (pipe >= 0 ? inner.slice(0, pipe) : inner).trim();
            const page = target.split('#')[0].trim();
            if (embedded && IMAGE_EXTENSION.test(page)) {
              const alias = pipe >= 0 ? inner.slice(pipe + 1).trim() : '';
              decos.push(Decoration.replace({
                widget: new ImageWidget(page, /^\d+(x\d+)?$/.test(alias) ? '' : alias),
              }).range(nFrom - 1, nTo));
              return false;
            }
            if (embedded) decos.push(hide.range(nFrom - 1, nFrom));
            // Hide `[[` plus, for aliased links, the `target|` part.
            decos.push(hide.range(nFrom, nFrom + 2 + (pipe >= 0 ? pipe + 1 : 0)));
            decos.push(hide.range(nTo - 2, nTo));
            const textFrom = nFrom + 2 + (pipe >= 0 ? pipe + 1 : 0);
            if (nTo - 2 > textFrom) {
              decos.push(Decoration.mark({
                class: 'cm-lp-wikilink',
                attributes: { 'data-lp-wikilink': target },
              }).range(textFrom, nTo - 2));
            }
            return false;
          }

          case 'Blockquote': {
            eachLine(nFrom, nTo, (lineFrom) => lineDeco(lineFrom, 'cm-lp-quote'));
            // A callout (`> [!tip] Title`): coloured rule, then the reader's
            // icon and bold title; `[!tip]` shows again while the caret is on it.
            const first = doc.lineAt(nFrom);
            const callout = CALLOUT_LINE.exec(first.text);
            if (callout) {
              const { icon, family } = calloutOf(callout[3]);
              eachLine(nFrom, nTo, (lineFrom) => lineDeco(lineFrom, `cm-lp-callout-${family}`));
              const markFrom = first.from + callout[1].length;
              const markTo = withSpace(markFrom + callout[2].length);
              if (revealLine(first.from, first.to)) {
                decos.push(Decoration.mark({ class: 'cm-lp-callout-mark' }).range(markFrom, markTo));
              } else {
                decos.push(Decoration.replace({ widget: new CalloutIconWidget(icon) }).range(markFrom, markTo));
              }
              if (first.to > markTo) decos.push(Decoration.mark({ class: 'callout-title cm-lp-callout-title' }).range(markTo, first.to));
              else decos.push(Decoration.widget({ widget: new CalloutTitleWidget(calloutOf(callout[3]).title), side: 1 }).range(first.to));
            }
            return;
          }

          case 'QuoteMark':
            hideMark(nFrom, nTo);
            return;

          case 'ListMark': {
            const text = state.sliceDoc(nFrom, nTo);
            const end = withSpace(nTo);
            if (/^[-*+]$/.test(text)) {
              // A task's checkbox stands in for its bullet.
              if (state.sliceDoc(end, end + 1) === '[' && nodeRef.node.nextSibling?.name === 'Task') hideMarker(nFrom, end);
              else {
                decos.push(Decoration.replace({ widget: new BulletWidget() }).range(nFrom, end));
                atomic.push(hide.range(nFrom, end));
              }
            } else {
              decos.push(Decoration.mark({ class: 'cm-lp-ol-mark' }).range(nFrom, nTo));
              if (end > nTo) hideMarker(nTo, end);
            }
            return;
          }

          case 'TaskMarker': {
            const checked = /x/i.test(state.sliceDoc(nFrom, nTo));
            decos.push(Decoration.replace({ widget: new CheckboxWidget(checked) }).range(nFrom, withSpace(nTo)));
            atomic.push(hide.range(nFrom, withSpace(nTo)));
            return;
          }

          case 'HorizontalRule':
            lineDeco(nFrom, 'cm-lp-rule');
            if (!revealLine(nFrom, nTo)) {
              decos.push(Decoration.replace({ widget: new HrWidget() }).range(nFrom, nTo));
            }
            return;

          case 'FencedCode': {
            const marks = nodeRef.node.getChildren('CodeMark');
            const closed = marks.length > 1 && doc.lineAt(marks[marks.length - 1].from).from !== doc.lineAt(nFrom).from;
            const shown = revealLine(nFrom, nTo);
            eachLine(nFrom, nTo, (lineFrom, first, last) => {
              lineDeco(lineFrom, 'cm-lp-code');
              if (first || (last && closed)) {
                lineDeco(lineFrom, 'cm-lp-fence');
                lineDeco(lineFrom, first ? 'cm-lp-fence-open' : 'cm-lp-fence-close');
                if (shown) lineDeco(lineFrom, 'cm-lp-fence-shown');
              }
            });
            return;
          }

          case 'Frontmatter':
            eachLine(nFrom, nTo, (lineFrom) => lineDeco(lineFrom, 'cm-lp-frontmatter'));
            return;

          case 'Table':
            eachLine(nFrom, nTo, (lineFrom) => lineDeco(lineFrom, 'cm-lp-table'));
            return;
        }
      },
    });
  }

  return { decorations: Decoration.set(decos, true), atomic: Decoration.set(atomic, true) };
}

// ── Click handling ───────────────────────────────────────────────────

/** Toggle the `[ ]`/`[x]` marker that starts at the widget's position. */
function toggleCheckbox(view: EditorView, pos: number): boolean {
  const text = view.state.sliceDoc(pos, pos + 3);
  const m = /^\[( |x|X)\]$/.exec(text);
  if (!m) return false;
  view.dispatch({
    changes: { from: pos, to: pos + 3, insert: m[1] === ' ' ? '[x]' : '[ ]' },
  });
  return true;
}

// ── Plugin ───────────────────────────────────────────────────────────

export const livePreview = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    atomic: DecorationSet;
    constructor(view: EditorView) {
      ({ decorations: this.decorations, atomic: this.atomic } = buildDecorations(view));
    }
    update(update: ViewUpdate) {
      if (update.docChanged || update.selectionSet || update.viewportChanged) {
        ({ decorations: this.decorations, atomic: this.atomic } = buildDecorations(update.view));
      }
    }
  },
  {
    decorations: (v) => v.decorations,
    provide: (plugin) => EditorView.atomicRanges.of((view) => view.plugin(plugin)?.atomic ?? Decoration.none),
    eventHandlers: {
      mousedown(event, view) {
        const target = event.target as HTMLElement;
        if (target.classList.contains('cm-lp-checkbox')) {
          return toggleCheckbox(view, view.posAtDOM(target));
        }
        const link = target.closest('.cm-lp-link[data-lp-href]') as HTMLElement | null;
        if (link) {
          const href = link.getAttribute('data-lp-href')!;
          if (/^https?:\/\//.test(href)) window.open(href, '_blank', 'noopener');
          return true;
        }
        const wiki = target.closest('.cm-lp-wikilink[data-lp-wikilink]') as HTMLElement | null;
        if (wiki) {
          view.state.facet(livePreviewConfig).onWikilink?.(wiki.getAttribute('data-lp-wikilink')!);
          return true;
        }
        return false;
      },
    },
  },
);

/** Hidden block markup at the start of a line: indentation, `- `, `1. `, `- [ ] `, `> `. */
const LINE_MARKUP = /^(?:\s*(?:[-*+]|\d+[.)])(?:\s+\[[ xX]\])?\s+|\s*(?:>\s?)+)/;

/**
 * The caret never sits inside hidden list or quote markup: Home and clicks at
 * the left edge land after the bullet, so Backspace there removes the bullet
 * (markdownKeymap) instead of joining the item to the line above.
 */
export const caretAfterMarkup = EditorState.transactionFilter.of((tr) => {
  if (!tr.selection) return tr;
  const state = tr.state;
  let changed = false;
  const ranges = tr.selection.ranges.map((range) => {
    if (!range.empty) return range;
    const line = state.doc.lineAt(range.head);
    const markup = LINE_MARKUP.exec(line.text)?.[0];
    if (!markup || range.head >= line.from + markup.length) return range;
    const node = syntaxTree(state).resolveInner(line.from + markup.length, -1);
    let marked = false;
    for (let at: SyntaxNode | null = node; at; at = at.parent) {
      if (at.name === 'ListItem' || at.name === 'Blockquote') { marked = true; break; }
      if (at.name === 'FencedCode' || at.name === 'CodeBlock') break;
    }
    if (!marked) return range;
    changed = true;
    return EditorSelection.cursor(line.from + markup.length);
  });
  if (!changed) return tr;
  return [tr, { selection: EditorSelection.create(ranges, tr.selection.mainIndex), sequential: true }];
});

