/**
 * A single newline inside a paragraph is a line break, as in Obsidian (its
 * "Strict line breaks" setting is off by default) and in the editor, where
 * each source line is its own line. Without this the reader joins the lines
 * and the page reflows when you click to edit.
 */
interface MdNode {
  type: string;
  value?: string;
  children?: MdNode[];
}

const SKIP = new Set(['code', 'inlineCode', 'html', 'math', 'inlineMath', 'yaml']);

function split(node: MdNode): void {
  if (!node.children || SKIP.has(node.type)) return;
  const next: MdNode[] = [];
  for (const child of node.children) {
    if (child.type === 'text' && child.value?.includes('\n')) {
      child.value.split(/\r?\n/).forEach((value, index) => {
        if (index > 0) next.push({ type: 'break' });
        if (value) next.push({ type: 'text', value });
      });
    } else {
      split(child);
      next.push(child);
    }
  }
  node.children = next;
}

export function remarkSoftBreaks() {
  return (tree: MdNode) => { split(tree); };
}
