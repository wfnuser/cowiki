import { useContext, useEffect, useMemo, useState, type ReactNode } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import rough from 'roughjs';
import type { Options } from 'roughjs/bin/core';
import { Maximize2, PenTool, ZoomIn } from 'lucide-react';

import { ObsidianReaderContext, withObsidianReader } from '@/components/ObsidianReader';
import { C } from '@/lib/design';
import {
  DRAWING_COLORS,
  elementPoints,
  localBounds,
  parseExcalidrawPage,
  sceneBounds,
  type ExcalidrawElement,
  type ExcalidrawScene,
} from '@/lib/excalidraw';
import { obsidianToReaderMarkdown } from '@/lib/obsidian-markdown';

const generator = rough.generator();
const MARKDOWN = withObsidianReader(undefined);

const FONTS: Record<number, string> = {
  1: '"Virgil", "Excalifont", "Segoe Print", "Bradley Hand", "Comic Sans MS", cursive',
  2: 'Helvetica, Arial, sans-serif',
  3: '"Cascadia Code", Menlo, Consolas, monospace',
  5: '"Excalifont", "Virgil", "Segoe Print", "Bradley Hand", "Comic Sans MS", cursive',
  6: '"Nunito", system-ui, sans-serif',
  7: '"Lilita One", system-ui, sans-serif',
  8: '"Comic Shanns", "Comic Sans MS", cursive',
};

function transparent(color: string | undefined): boolean {
  return !color || color === 'transparent' || /^#[0-9a-f]{6}00$/i.test(color);
}

/** Excalidraw softens small shapes so they don't turn into scribbles. */
function roughnessOf(element: ExcalidrawElement, linear: boolean): number {
  const roughness = element.roughness ?? 1;
  const max = Math.max(Math.abs(element.width), Math.abs(element.height));
  const min = Math.min(Math.abs(element.width), Math.abs(element.height));
  if ((min >= 20 && max >= 50) || min >= 15 || (linear && max >= 50)) return roughness;
  return Math.min(roughness / (max < 10 ? 3 : 2), 2.5);
}

function dashOf(element: ExcalidrawElement): number[] | undefined {
  const width = element.strokeWidth ?? 1;
  if (element.strokeStyle === 'dashed') return [8, 8 + width];
  if (element.strokeStyle === 'dotted') return [1.5, 6 + width];
  return undefined;
}

function optionsFor(element: ExcalidrawElement, filled: boolean, linear = false): Options {
  const width = element.strokeWidth ?? 1;
  return {
    seed: element.seed ?? 1,
    roughness: roughnessOf(element, linear),
    stroke: element.strokeColor ?? DRAWING_COLORS.ink,
    strokeWidth: element.strokeStyle && element.strokeStyle !== 'solid' ? width + 0.5 : width,
    disableMultiStroke: !!element.strokeStyle && element.strokeStyle !== 'solid',
    fillWeight: width / 2,
    hachureGap: width * 4,
    preserveVertices: (element.roughness ?? 1) < 2,
    ...(filled && !transparent(element.backgroundColor)
      ? { fill: element.backgroundColor, fillStyle: element.fillStyle ?? 'hachure' }
      : {}),
    ...(element.type === 'ellipse' ? { curveFitting: 1 } : {}),
  };
}

type Drawable = ReturnType<typeof generator.rectangle>;

/** Turn a rough.js drawable into SVG paths, keeping dashes on the outline only. */
function paths(drawable: Drawable, element: ExcalidrawElement, key: string): ReactNode[] {
  const options = drawable.options;
  const dash = dashOf(element)?.join(' ');
  return drawable.sets.map((set, index) => {
    const d = generator.opsToPath(set);
    if (set.type === 'fillPath') return <path key={`${key}-${index}`} d={d} fill={options.fill} stroke="none" />;
    if (set.type === 'fillSketch') {
      return <path key={`${key}-${index}`} d={d} fill="none" stroke={options.fill} strokeWidth={options.fillWeight} />;
    }
    return (
      <path key={`${key}-${index}`} d={d} fill="none" stroke={options.stroke} strokeWidth={options.strokeWidth}
        strokeDasharray={dash} strokeLinecap="round" strokeLinejoin="round" />
    );
  });
}

function cornerRadius(element: ExcalidrawElement): number {
  if (!element.roundness) return 0;
  const size = Math.min(Math.abs(element.width), Math.abs(element.height));
  if (element.roundness.type === 2) return size * 0.25;
  const fixed = element.roundness.value ?? 32;
  return size <= fixed / 0.25 ? size * 0.25 : fixed;
}

function arrowhead(element: ExcalidrawElement, points: Array<[number, number]>, kind: string, atEnd: boolean, key: string): ReactNode[] {
  if (points.length < 2) return [];
  const [tip, from] = atEnd ? [points.at(-1)!, points.at(-2)!] : [points[0], points[1]];
  const dx = tip[0] - from[0];
  const dy = tip[1] - from[1];
  const length = Math.hypot(dx, dy);
  if (length === 0) return [];
  const ux = dx / length;
  const uy = dy / length;
  const width = element.strokeWidth ?? 1;
  const size = Math.min(kind === 'bar' ? 15 : 20 + width * 2, length * 0.6);
  const at = (back: number, side: number): [number, number] => [tip[0] - ux * back - uy * side, tip[1] - uy * back + ux * side];
  const base = { ...optionsFor(element, false, true), strokeLineDash: undefined };
  const solid = { ...element, strokeStyle: 'solid' };
  const spread = Math.tan((25 * Math.PI) / 180) * size;
  switch (kind) {
    case 'bar':
      return paths(generator.line(...at(0, size / 2), ...at(0, -size / 2), base), solid, key);
    case 'dot':
    case 'circle':
    case 'circle_outline': {
      const center = at(size / 4, 0);
      const fill = kind === 'circle_outline' ? undefined : element.strokeColor;
      return paths(generator.circle(center[0], center[1], size / 2, { ...base, fill, fillStyle: 'solid' }), solid, key);
    }
    case 'triangle':
    case 'triangle_outline':
      return paths(generator.polygon([tip, at(size, spread), at(size, -spread)], {
        ...base, fill: kind === 'triangle' ? element.strokeColor : undefined, fillStyle: 'solid',
      }), solid, key);
    case 'diamond':
    case 'diamond_outline':
      return paths(generator.polygon([tip, at(size / 2, spread / 1.5), at(size, 0), at(size / 2, -spread / 1.5)], {
        ...base, fill: kind === 'diamond' ? element.strokeColor : undefined, fillStyle: 'solid',
      }), solid, key);
    default:
      return paths(generator.linearPath([at(size, spread), tip, at(size, -spread)], base), solid, key);
  }
}

function freehandPath(points: Array<[number, number]>): string {
  if (points.length === 1) return `M ${points[0][0]} ${points[0][1]} l 0.01 0`;
  let d = `M ${points[0][0]} ${points[0][1]}`;
  for (let index = 1; index < points.length - 1; index += 1) {
    const [x, y] = points[index];
    const [nx, ny] = points[index + 1];
    d += ` Q ${x} ${y} ${(x + nx) / 2} ${(y + ny) / 2}`;
  }
  const last = points.at(-1)!;
  return `${d} L ${last[0]} ${last[1]}`;
}

function TextElement({ element }: { element: ExcalidrawElement }) {
  const fontSize = element.fontSize ?? 20;
  const lineHeight = fontSize * (element.lineHeight ?? 1.25);
  const lines = (element.text ?? '').split('\n');
  const anchor = element.textAlign === 'center' ? 'middle' : element.textAlign === 'right' ? 'end' : 'start';
  const x = anchor === 'middle' ? element.width / 2 : anchor === 'end' ? element.width : 0;
  return (
    <text
      fill={element.strokeColor ?? DRAWING_COLORS.ink}
      fontSize={fontSize}
      fontFamily={FONTS[element.fontFamily ?? 1] ?? FONTS[1]}
      textAnchor={anchor}
      dominantBaseline="middle"
      style={{ whiteSpace: 'pre' }}
    >
      {lines.map((line, index) => (
        <tspan key={index} x={x} y={index * lineHeight + lineHeight / 2}>{line || ' '}</tspan>
      ))}
    </text>
  );
}

function ImageElement({ element, file }: { element: ExcalidrawElement; file?: ExcalidrawScene['files'][string] }) {
  const { loadAsset } = useContext(ObsidianReaderContext);
  const link = file?.link;
  const [loaded, setLoaded] = useState<{ link: string; url: string } | null>(null);
  useEffect(() => {
    if (file?.dataURL || !link) return;
    if (/^https?:/i.test(link)) return;
    let cancelled = false;
    loadAsset?.(link).then((url) => { if (!cancelled) setLoaded({ link, url }); }, () => {});
    return () => { cancelled = true; };
  }, [file?.dataURL, link, loadAsset]);
  const href = file?.dataURL ?? (link && /^https?:/i.test(link) ? link : loaded?.link === link ? loaded?.url : undefined);
  const [sx, sy] = element.scale ?? [1, 1];
  if (!href) {
    return <rect width={element.width} height={element.height} fill="none" stroke={C.line} strokeDasharray="6 6" />;
  }
  return (
    <image
      href={href}
      width={element.width}
      height={element.height}
      preserveAspectRatio="none"
      transform={sx !== 1 || sy !== 1 ? `translate(${sx < 0 ? element.width : 0} ${sy < 0 ? element.height : 0}) scale(${sx} ${sy})` : undefined}
    />
  );
}

function renderElement(element: ExcalidrawElement, scene: ExcalidrawScene): ReactNode {
  const key = element.id;
  const { width, height } = element;
  switch (element.type) {
    case 'rectangle':
    case 'embeddable':
    case 'iframe': {
      const r = cornerRadius(element);
      const options = optionsFor(element, true);
      const shape = r > 0
        ? generator.path(`M ${r} 0 L ${width - r} 0 Q ${width} 0, ${width} ${r} L ${width} ${height - r} Q ${width} ${height}, ${width - r} ${height} L ${r} ${height} Q 0 ${height}, 0 ${height - r} L 0 ${r} Q 0 0, ${r} 0`, options)
        : generator.rectangle(0, 0, width, height, options);
      return paths(shape, element, key);
    }
    case 'ellipse':
      return paths(generator.ellipse(width / 2, height / 2, width, height, optionsFor(element, true)), element, key);
    case 'diamond':
      return paths(generator.polygon([[width / 2, 0], [width, height / 2], [width / 2, height], [0, height / 2]], optionsFor(element, true)), element, key);
    case 'line':
    case 'arrow': {
      const points = elementPoints(element);
      if (points.length < 2) return null;
      const closed = element.type === 'line' && points.length > 2
        && points[0][0] === points.at(-1)![0] && points[0][1] === points.at(-1)![1];
      const options = optionsFor(element, closed, true);
      const body = closed
        ? generator.polygon(points, options)
        : element.roundness && points.length > 2
          ? generator.curve(points, options)
          : generator.linearPath(points, options);
      return [
        ...paths(body, element, key),
        ...(element.type === 'arrow' && element.endArrowhead ? arrowhead(element, points, element.endArrowhead, true, `${key}-end`) : []),
        ...(element.type === 'arrow' && element.startArrowhead ? arrowhead(element, points, element.startArrowhead, false, `${key}-start`) : []),
      ];
    }
    case 'freedraw': {
      const points = elementPoints(element);
      if (points.length === 0) return null;
      return (
        <path d={freehandPath(points)} fill="none" stroke={element.strokeColor ?? DRAWING_COLORS.ink}
          strokeWidth={(element.strokeWidth ?? 1) * 2.2} strokeLinecap="round" strokeLinejoin="round" />
      );
    }
    case 'text':
      return <TextElement element={element} />;
    case 'image':
      return <ImageElement element={element} file={element.fileId ? scene.files[element.fileId] : undefined} />;
    case 'frame':
    case 'magicframe':
      return (
        <>
          <rect width={width} height={height} rx={8} fill="none" stroke={DRAWING_COLORS.frame} strokeWidth={1} />
          <text y={-8} fontSize={14} fill={DRAWING_COLORS.frameLabel} fontFamily="system-ui, sans-serif">{element.name || 'Frame'}</text>
        </>
      );
    default:
      return null;
  }
}

/** A read-only SVG rendering of an Excalidraw scene, hand-drawn like the original. */
export function ExcalidrawDrawing({ scene, fit = true }: { scene: ExcalidrawScene; fit?: boolean }) {
  const bounds = useMemo(() => sceneBounds(scene.elements), [scene.elements]);
  const rendered = useMemo(() => scene.elements.map((element) => {
    const box = localBounds(element);
    const cx = (box.minX + box.maxX) / 2;
    const cy = (box.minY + box.maxY) / 2;
    const degrees = ((element.angle ?? 0) * 180) / Math.PI;
    return (
      <g
        key={element.id}
        opacity={(element.opacity ?? 100) / 100}
        transform={`translate(${element.x} ${element.y})${degrees ? ` rotate(${degrees} ${cx} ${cy})` : ''}`}
      >
        {renderElement(element, scene)}
      </g>
    );
  }), [scene]);
  return (
    <svg
      role="img"
      aria-label="Excalidraw drawing"
      viewBox={`${bounds.x} ${bounds.y} ${bounds.width} ${bounds.height}`}
      width={fit ? '100%' : bounds.width}
      height={fit ? undefined : bounds.height}
      style={{
        display: 'block',
        maxWidth: fit ? bounds.width : undefined,
        maxHeight: fit ? '75vh' : undefined,
        margin: '0 auto',
        background: scene.background,
        filter: scene.dark ? 'invert(93%) hue-rotate(180deg)' : undefined,
      }}
    >
      {rendered}
    </svg>
  );
}

/** Reader for an Excalidraw page: the user's notes, then the drawing. */
export function ExcalidrawReader({ title, body }: { title: string; body: string }) {
  const page = useMemo(() => parseExcalidrawPage(body), [body]);
  const [fit, setFit] = useState(true);
  const empty = page.scene && page.scene.elements.length === 0;
  return (
    <div style={{ position: 'absolute', inset: 0, overflow: 'auto', padding: '32px 32px 56px' }}>
      <article style={{ width: 'min(1100px, 100%)', margin: '0 auto' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 6 }}>
          <PenTool size={18} color={C.accent} strokeWidth={1.8} />
          <h1 className="page-title page-title--compact" style={{ margin: 0 }}>{title}</h1>
          {page.scene && !empty && (
            <button type="button" onClick={() => setFit((value) => !value)} style={toolButton} title={fit ? 'Show at actual size' : 'Fit to the window'}>
              {fit ? <ZoomIn size={13} /> : <Maximize2 size={13} />} {fit ? 'Actual size' : 'Fit'}
            </button>
          )}
        </div>
        <p style={{ margin: '0 0 16px', fontSize: 12.5, color: C.muted }}>
          Excalidraw drawing · read-only here; edit it in Obsidian&apos;s Excalidraw plugin.
        </p>
        {page.notes && (
          <div className="page-prose" style={{ marginBottom: 18 }}>
            <ReactMarkdown remarkPlugins={[remarkGfm]} components={MARKDOWN}>{obsidianToReaderMarkdown(page.notes)}</ReactMarkdown>
          </div>
        )}
        {page.error ? (
          <div role="alert" style={notice}>Could not read this drawing: {page.error}</div>
        ) : empty ? (
          <div style={notice}>This drawing is empty.</div>
        ) : page.scene ? (
          <div style={{ border: `1px solid ${C.line}`, borderRadius: 10, overflow: 'auto', background: page.scene.background }}>
            <ExcalidrawDrawing scene={page.scene} fit={fit} />
          </div>
        ) : null}
      </article>
    </div>
  );
}

const toolButton: React.CSSProperties = {
  marginLeft: 'auto', display: 'inline-flex', alignItems: 'center', gap: 6, padding: '5px 9px',
  border: `1px solid ${C.line}`, borderRadius: 7, background: C.panel, color: C.ink2, fontSize: 12, cursor: 'pointer',
};

const notice: React.CSSProperties = {
  display: 'flex', alignItems: 'center', justifyContent: 'center', minHeight: 160, padding: 16,
  border: `1px dashed ${C.line}`, borderRadius: 11, color: C.muted, fontSize: 13,
};
