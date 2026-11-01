/**
 * Read drawings saved by Obsidian's Excalidraw plugin (`*.excalidraw.md`).
 *
 * The plugin keeps the scene as JSON in a `## Drawing` code block, usually
 * LZ-String compressed (`compressed-json`). Images are listed separately under
 * `## Embedded Files` as `<fileId>: [[path]]`. Everything before the
 * `# Excalidraw Data` heading is ordinary Markdown the user wrote.
 */
import LZString from 'lz-string';

/** Excalidraw's own default colors. They belong to the drawing, not the app theme. */
export const DRAWING_COLORS = {
  ink: '#1e1e1e',
  paper: '#ffffff',
  frame: '#bbbbbb',
  frameLabel: '#999999',
} as const;

export interface ExcalidrawElement {
  id: string;
  type: string;
  x: number;
  y: number;
  width: number;
  height: number;
  angle?: number;
  strokeColor?: string;
  backgroundColor?: string;
  fillStyle?: string;
  strokeWidth?: number;
  strokeStyle?: string;
  roughness?: number;
  opacity?: number;
  seed?: number;
  isDeleted?: boolean;
  roundness?: { type: number; value?: number } | null;
  points?: Array<[number, number]>;
  startArrowhead?: string | null;
  endArrowhead?: string | null;
  text?: string;
  fontSize?: number;
  fontFamily?: number;
  textAlign?: string;
  verticalAlign?: string;
  lineHeight?: number;
  fileId?: string | null;
  scale?: [number, number];
  name?: string | null;
  link?: string | null;
}

export interface ExcalidrawScene {
  elements: ExcalidrawElement[];
  background: string;
  dark: boolean;
  /** fileId → data URL stored in the scene, or a vault link / URL from `## Embedded Files`. */
  files: Record<string, { dataURL?: string; link?: string }>;
}

export interface ExcalidrawPage {
  /** The user's own Markdown before the drawing data. */
  notes: string;
  scene: ExcalidrawScene | null;
  error: string | null;
}

export function isExcalidrawPage(path: string, body: string): boolean {
  if (/\.excalidraw(\.md)?$/i.test(path)) return true;
  const frontmatter = /^---\r?\n([\s\S]*?)\r?\n---/.exec(body)?.[1] ?? '';
  return /^excalidraw-plugin\s*:/m.test(frontmatter);
}

function stripFrontmatter(body: string): string {
  return body.replace(/^---\r?\n[\s\S]*?\r?\n---\r?\n?/, '');
}

function drawingJson(body: string): string | null {
  const compressed = /```compressed-json\s*\n([\s\S]*?)\n```/.exec(body);
  if (compressed) {
    const json = LZString.decompressFromBase64(compressed[1].replace(/\s+/g, ''));
    if (!json) throw new Error('The compressed drawing data could not be decompressed.');
    return json;
  }
  const plain = /##\s*Drawing\s*\n```json\s*\n([\s\S]*?)\n```/.exec(body);
  if (plain) return plain[1];
  const trimmed = body.trim();
  return trimmed.startsWith('{') ? trimmed : null;
}

function embeddedFiles(body: string): Record<string, { link: string }> {
  const files: Record<string, { link: string }> = {};
  const section = /##\s*Embedded Files\s*\n([\s\S]*?)(?=\n#|\n%%|$)/.exec(body)?.[1] ?? '';
  for (const line of section.split(/\r?\n/)) {
    const match = /^([\w-]+):\s*(?:\[\[([^\]|]+)(?:\|[^\]]*)?\]\]|(https?:\/\/\S+))/.exec(line.trim());
    if (match) files[match[1]] = { link: match[2] ?? match[3] };
  }
  return files;
}

function finite(value: unknown, fallback = 0): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback;
}

/** Parse an Excalidraw page into the user's notes and the scene to draw. */
export function parseExcalidrawPage(body: string): ExcalidrawPage {
  const content = stripFrontmatter(body);
  const dataStart = content.search(/^#\s*Excalidraw Data\s*$/m);
  const notes = (dataStart >= 0 ? content.slice(0, dataStart) : content.split(/^##\s*(?:Text Elements|Embedded Files|Drawing)\s*$/m)[0])
    .replace(/^==⚠.*?==.*$/gm, '')
    .replace(/%%[\s\S]*?(%%|$)/g, '')
    .trim();
  let raw: unknown;
  try {
    const json = drawingJson(content);
    if (json === null) return { notes, scene: null, error: 'This page has no Excalidraw drawing data.' };
    raw = JSON.parse(json);
  } catch (error) {
    return { notes, scene: null, error: error instanceof Error ? error.message : String(error) };
  }
  const data = raw as { elements?: unknown; appState?: { viewBackgroundColor?: string; theme?: string }; files?: Record<string, { dataURL?: string }> };
  const elements = (Array.isArray(data.elements) ? data.elements : [])
    .filter((item): item is ExcalidrawElement => !!item && typeof item === 'object' && typeof (item as ExcalidrawElement).type === 'string')
    .filter((item) => !item.isDeleted)
    .map((item) => ({ ...item, x: finite(item.x), y: finite(item.y), width: finite(item.width), height: finite(item.height) }));
  const files: ExcalidrawScene['files'] = { ...embeddedFiles(content) };
  for (const [id, file] of Object.entries(data.files ?? {})) {
    if (file && typeof file.dataURL === 'string') files[id] = { ...files[id], dataURL: file.dataURL };
  }
  const background = data.appState?.viewBackgroundColor;
  return {
    notes,
    error: null,
    scene: {
      elements,
      background: typeof background === 'string' && background ? background : DRAWING_COLORS.paper,
      dark: data.appState?.theme === 'dark',
      files,
    },
  };
}

/** Points of a line, arrow or freehand stroke, relative to the element's x/y. */
export function elementPoints(element: ExcalidrawElement): Array<[number, number]> {
  return (element.points ?? []).filter((point) => Array.isArray(point) && Number.isFinite(point[0]) && Number.isFinite(point[1]));
}

/** Local bounding box (before rotation) relative to the element's x/y. */
export function localBounds(element: ExcalidrawElement): { minX: number; minY: number; maxX: number; maxY: number } {
  const points = elementPoints(element);
  if (points.length > 0) {
    const xs = points.map((point) => point[0]);
    const ys = points.map((point) => point[1]);
    return { minX: Math.min(...xs), minY: Math.min(...ys), maxX: Math.max(...xs), maxY: Math.max(...ys) };
  }
  return { minX: 0, minY: 0, maxX: element.width, maxY: element.height };
}

/** Scene bounds including rotation, padded for strokes. */
export function sceneBounds(elements: ExcalidrawElement[], padding = 24): { x: number; y: number; width: number; height: number } {
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  for (const element of elements) {
    const box = localBounds(element);
    const cx = (box.minX + box.maxX) / 2;
    const cy = (box.minY + box.maxY) / 2;
    const angle = element.angle ?? 0;
    const corners: Array<[number, number]> = [[box.minX, box.minY], [box.maxX, box.minY], [box.maxX, box.maxY], [box.minX, box.maxY]];
    for (const [px, py] of corners) {
      const dx = px - cx;
      const dy = py - cy;
      const x = element.x + cx + dx * Math.cos(angle) - dy * Math.sin(angle);
      const y = element.y + cy + dx * Math.sin(angle) + dy * Math.cos(angle);
      minX = Math.min(minX, x);
      minY = Math.min(minY, y);
      maxX = Math.max(maxX, x);
      maxY = Math.max(maxY, y);
    }
  }
  if (!Number.isFinite(minX)) return { x: 0, y: 0, width: 0, height: 0 };
  return { x: minX - padding, y: minY - padding, width: maxX - minX + padding * 2, height: maxY - minY + padding * 2 };
}
