/**
 * Minimal GFM table parser & inline tokeniser for the live-preview table widget.
 * Keeps rendered tables pixel-identical to `.prose table` while the caret is
 * outside the table, and maps cell clicks back to the matching source offset.
 */

export type TableAlign = 'left' | 'center' | 'right' | null;

export interface ParsedTable {
  headers: string[];
  alignments: TableAlign[];
  rows: string[][];
}

export type InlineToken =
  | { kind: 'text'; text: string }
  | { kind: 'code'; text: string }
  | { kind: 'strong'; children: InlineToken[] }
  | { kind: 'em'; children: InlineToken[] }
  | { kind: 'del'; children: InlineToken[] }
  | { kind: 'link'; label: string; href: string }
  | { kind: 'wikilink'; label: string; target: string };

interface CellSlice {
  text: string;
  /** Offset of the first non-space character inside the source line. */
  start: number;
}

/** Split one GFM table line into cells, honouring backslash-escaped pipes. */
function splitRow(line: string): CellSlice[] {
  let start = 0;
  let end = line.length;
  while (start < end && (line[start] === ' ' || line[start] === '\t')) start++;
  while (end > start && (line[end - 1] === ' ' || line[end - 1] === '\t')) end--;
  if (start < end && line[start] === '|') start++;
  if (end > start && line[end - 1] === '|' && !isEscaped(line, end - 1)) end--;

  const cells: CellSlice[] = [];
  let cellFrom = start;
  for (let i = start; i <= end; i++) {
    const atPipe = i < end && line[i] === '|' && !isEscaped(line, i);
    if (!atPipe && i < end) continue;
    const raw = line.slice(cellFrom, i);
    const leadingSpaces = /^\s*/.exec(raw)![0].length;
    const text = raw.trim().replace(/\\\|/g, '|');
    cells.push({ text, start: Math.min(cellFrom + leadingSpaces, i) });
    cellFrom = i + 1;
  }
  return cells;
}

function isEscaped(text: string, index: number): boolean {
  let slashes = 0;
  for (let i = index - 1; i >= 0 && text[i] === '\\'; i--) slashes++;
  return slashes % 2 === 1;
}

function parseDelimiterCell(cell: string): TableAlign | false {
  const m = /^(:?)-+(:?)$/.exec(cell.trim());
  if (!m) return false;
  if (m[1] && m[2]) return 'center';
  if (m[2]) return 'right';
  if (m[1]) return 'left';
  return null;
}

/** Parse a GFM pipe table block; returns null if it is not a valid table. */
export function parseGfmTable(source: string): ParsedTable | null {
  const lines = source.split(/\r?\n/);
  if (lines.length < 2) return null;
  const headerCells = splitRow(lines[0]);
  const delimCells = splitRow(lines[1]);
  if (headerCells.length === 0 || headerCells.length !== delimCells.length) return null;

  const alignments: TableAlign[] = [];
  for (const cell of delimCells) {
    const align = parseDelimiterCell(cell.text);
    if (align === false) return null;
    alignments.push(align);
  }

  const colCount = headerCells.length;
  const rows: string[][] = [];
  for (let r = 2; r < lines.length; r++) {
    if (!lines[r].trim()) continue;
    const cells = splitRow(lines[r]).map((c) => c.text);
    while (cells.length < colCount) cells.push('');
    rows.push(cells.slice(0, colCount));
  }

  return {
    headers: headerCells.map((c) => c.text),
    alignments,
    rows,
  };
}

/**
 * Character offset inside `source` for a click on `(section, rowIndex, colIndex)`
 * of the rendered table, so clicking a cell reveals source with the caret there.
 */
export function tableCellOffset(
  source: string,
  section: 'head' | 'body',
  rowIndex: number,
  colIndex: number,
): number {
  const lines = source.split('\n');
  const lineIndex = section === 'head' ? 0 : rowIndex + 2;
  const clampedLine = Math.max(0, Math.min(lineIndex, lines.length - 1));
  let offset = 0;
  for (let i = 0; i < clampedLine; i++) offset += lines[i].length + 1;
  const cells = splitRow(lines[clampedLine]);
  const cell = cells[colIndex];
  return cell ? offset + cell.start : offset;
}

const INLINE_SOURCE = /`([^`]+)`|\[\[([^\][|]+?)(?:\|([^\]]+))?\]\]|\[([^\]]+)\]\(([^)\s]+)\)|\*\*(.+?)\*\*|__(.+?)__|~~(.+?)~~|\*([^*]+)\*|_([^_]+)_/.source;

/** Tokenise common inline Markdown inside a table cell. */
export function tokenizeCell(text: string): InlineToken[] {
  const tokens: InlineToken[] = [];
  const regex = new RegExp(INLINE_SOURCE, 'g');
  let last = 0;
  for (let match = regex.exec(text); match; match = regex.exec(text)) {
    if (match.index > last) {
      tokens.push({ kind: 'text', text: text.slice(last, match.index) });
    }
    if (match[1] !== undefined) {
      tokens.push({ kind: 'code', text: match[1] });
    } else if (match[2] !== undefined) {
      const target = match[2].trim();
      const label = (match[3] ?? match[2]).trim();
      tokens.push({ kind: 'wikilink', label, target });
    } else if (match[4] !== undefined && match[5] !== undefined) {
      tokens.push({ kind: 'link', label: match[4], href: match[5] });
    } else if (match[6] !== undefined || match[7] !== undefined) {
      tokens.push({ kind: 'strong', children: tokenizeCell(match[6] ?? match[7]) });
    } else if (match[8] !== undefined) {
      tokens.push({ kind: 'del', children: tokenizeCell(match[8]) });
    } else if (match[9] !== undefined || match[10] !== undefined) {
      tokens.push({ kind: 'em', children: tokenizeCell(match[9] ?? match[10]) });
    }
    last = match.index + match[0].length;
  }
  if (last < text.length) {
    tokens.push({ kind: 'text', text: text.slice(last) });
  }
  return tokens;
}
