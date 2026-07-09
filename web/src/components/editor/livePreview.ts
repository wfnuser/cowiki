import { Decoration, EditorView, ViewPlugin, WidgetType } from '@codemirror/view';
import type { DecorationSet, ViewUpdate } from '@codemirror/view';
import { Facet } from '@codemirror/state';
import type { EditorState, Range } from '@codemirror/state';
import { syntaxTree } from '@codemirror/language';
import type { SyntaxNode } from '@lezer/common';

/** Editor-level callbacks the host app provides (wikilink navigation). */
export interface LivePreviewConfig {
  onWikilink?: (target: string) => void;
}

export const livePreviewConfig = Facet.define<LivePreviewConfig, LivePreviewConfig>({
  combine: (values) => values[0] ?? {},
});

// ── Widgets ──────────────────────────────────────────────────────────

class BulletWidget extends WidgetType {
  toDOM() {
    const span = document.createElement('span');
    span.className = 'cm-lp-bullet';
    span.textContent = '•';
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
  toDOM() {
    const img = document.createElement('img');
    img.src = this.src;
    img.alt = this.alt;
    img.className = 'cm-lp-image';
    return img;
  }
  eq(other: ImageWidget) { return other.src === this.src && other.alt === this.alt; }
}

// ── Decoration builder ───────────────────────────────────────────────

const bulletDeco = Decoration.replace({ widget: new BulletWidget() });
const hide = Decoration.replace({});

function buildDecorations(view: EditorView): DecorationSet {
  const decos: Range<Decoration>[] = [];
  const { state } = view;
  const doc = state.doc;

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

  /** Hide a mark, swallowing one following space (e.g. `# `, `> `). */
  const hideMark = (from: number, to: number, limit: number) => {
    const end = to < limit && state.sliceDoc(to, to + 1) === ' ' ? to + 1 : to;
    decos.push(hide.range(from, end));
  };

  const hideChildMarks = (node: SyntaxNode, markName: string) => {
    for (let child = node.firstChild; child; child = child.nextSibling) {
      if (child.name === markName) decos.push(hide.range(child.from, child.to));
    }
  };

  for (const { from, to } of view.visibleRanges) {
    syntaxTree(state).iterate({
      from, to,
      enter: (nodeRef) => {
        const name = nodeRef.name;
        const nFrom = nodeRef.from;
        const nTo = nodeRef.to;

        // Headings: size the whole line, hide the leading `# ` marks.
        const headingLevel = name.startsWith('ATXHeading') ? Number(name.slice(10)) : 0;
        if (headingLevel) {
          lineDeco(nFrom, `cm-lp-h${headingLevel}`);
          if (!revealLine(nFrom, nTo)) {
            const mark = nodeRef.node.getChild('HeaderMark');
            if (mark) hideMark(mark.from, mark.to, nTo);
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
            if (reveal(nFrom, nTo)) return;
            const inner = state.sliceDoc(nFrom + 2, nTo - 2);
            const pipe = inner.indexOf('|');
            const target = (pipe >= 0 ? inner.slice(0, pipe) : inner).trim();
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

          case 'Blockquote':
            eachLine(nFrom, nTo, (lineFrom) => lineDeco(lineFrom, 'cm-lp-quote'));
            return;

          case 'QuoteMark':
            if (!revealLine(nFrom, nTo)) hideMark(nFrom, nTo, doc.length);
            return;

          case 'ListMark': {
            const text = state.sliceDoc(nFrom, nTo);
            if (/^[-*+]$/.test(text) && !revealLine(nFrom, nTo)) {
              // Skip the bullet swap when a task checkbox follows: the box is enough.
              const after = state.sliceDoc(nTo + 1, nTo + 2);
              if (after === '[') decos.push(hide.range(nFrom, Math.min(nTo + 1, doc.length)));
              else decos.push(bulletDeco.range(nFrom, nTo));
            }
            return;
          }

          case 'TaskMarker': {
            if (revealLine(nFrom, nTo)) return;
            const checked = /x/i.test(state.sliceDoc(nFrom, nTo));
            hideMark(nFrom, nTo, doc.length);
            decos.push(Decoration.widget({ widget: new CheckboxWidget(checked), side: 1 }).range(nFrom));
            return;
          }

          case 'HorizontalRule':
            if (!revealLine(nFrom, nTo)) {
              decos.push(Decoration.replace({ widget: new HrWidget() }).range(nFrom, nTo));
            }
            return;

          case 'FencedCode':
            eachLine(nFrom, nTo, (lineFrom, first, last) => {
              lineDeco(lineFrom, 'cm-lp-code');
              if (first) lineDeco(lineFrom, 'cm-lp-code-first');
              if (last) lineDeco(lineFrom, 'cm-lp-code-last');
            });
            return;

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

  return Decoration.set(decos, true);
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
    constructor(view: EditorView) {
      this.decorations = buildDecorations(view);
    }
    update(update: ViewUpdate) {
      if (update.docChanged || update.selectionSet || update.viewportChanged) {
        this.decorations = buildDecorations(update.view);
      }
    }
  },
  {
    decorations: (v) => v.decorations,
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

export type { EditorState };
