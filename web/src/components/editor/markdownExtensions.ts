import type { MarkdownConfig } from '@lezer/markdown';
import { tags as t } from '@lezer/highlight';

const BRACKET_L = 91; /* [ */
const BRACKET_R = 93; /* ] */
const NEWLINE = 10;

/**
 * Inline parser for Obsidian-style wikilinks: `[[target]]` and
 * `[[target|alias]]`. Produces a WikiLink node wrapping two WikiLinkMark
 * nodes for the `[[` / `]]` delimiters.
 */
export const WikiLink: MarkdownConfig = {
  defineNodes: [
    { name: 'WikiLink', style: t.link },
    { name: 'WikiLinkMark', style: t.processingInstruction },
  ],
  parseInline: [{
    name: 'WikiLink',
    before: 'Link',
    parse(cx, next, pos) {
      if (next !== BRACKET_L || cx.char(pos + 1) !== BRACKET_L) return -1;
      let close = -1;
      for (let i = pos + 2; i < cx.end - 1; i++) {
        const ch = cx.char(i);
        if (ch === NEWLINE) return -1;
        if (ch === BRACKET_R && cx.char(i + 1) === BRACKET_R) { close = i; break; }
      }
      if (close < 0 || close === pos + 2) return -1;
      return cx.addElement(cx.elt('WikiLink', pos, close + 2, [
        cx.elt('WikiLinkMark', pos, pos + 2),
        cx.elt('WikiLinkMark', close, close + 2),
      ]));
    },
  }],
};

/**
 * Block parser for a YAML frontmatter fence at the very start of the
 * document. Without this, the parser reads the `---` fences as horizontal
 * rules / setext headings and renders the metadata as prose.
 * An unterminated fence swallows the rest of the document; pages always
 * carry a closing fence in practice.
 */
export const Frontmatter: MarkdownConfig = {
  defineNodes: [{ name: 'Frontmatter', block: true, style: t.meta }],
  parseBlock: [{
    name: 'Frontmatter',
    before: 'LinkReference',
    parse(cx, line) {
      if (cx.lineStart !== 0 || line.text.trim() !== '---') return false;
      const start = cx.lineStart;
      let end = cx.lineStart + line.text.length;
      while (cx.nextLine()) {
        end = cx.lineStart + line.text.length;
        if (line.text.trim() === '---') { cx.nextLine(); break; }
      }
      cx.addElement(cx.elt('Frontmatter', start, end));
      return true;
    },
  }],
};
