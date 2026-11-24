import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { join, relative } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import {
  avatarInitials,
  colorForAvatarName,
  colorForName,
  colorForSpaceId,
  colorForUserId,
  colors,
  fonts,
  spaceMonogramColor,
  terminalTheme,
} from '../src/lib/design.ts';

const css = readFileSync(new URL('../src/index.css', import.meta.url), 'utf8');
const srcDir = fileURLToPath(new URL('../src/', import.meta.url));

function sourceOf(rel: string): string {
  return readFileSync(new URL(`../src/${rel}`, import.meta.url), 'utf8');
}

function sourceFiles(): Array<{ rel: string; source: string }> {
  const files: Array<{ rel: string; source: string }> = [];
  for (const entry of readdirSync(srcDir, { recursive: true, withFileTypes: true })) {
    if (!entry.isFile() || !/\.tsx?$/.test(entry.name)) continue;
    const path = join(entry.parentPath, entry.name);
    files.push({ rel: relative(srcDir, path), source: readFileSync(path, 'utf8') });
  }
  return files;
}

function relativeLuminance(hex: string): number {
  const channels = [1, 3, 5].map((offset) => Number.parseInt(hex.slice(offset, offset + 2), 16) / 255);
  const [red, green, blue] = channels.map((channel) => (
    channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4
  ));
  return 0.2126 * red + 0.7152 * green + 0.0722 * blue;
}

function contrastRatio(first: string, second: string): number {
  const lighter = Math.max(relativeLuminance(first), relativeLuminance(second));
  const darker = Math.min(relativeLuminance(first), relativeLuminance(second));
  return (lighter + 0.05) / (darker + 0.05);
}

function cssColor(token: string): string {
  const match = css.match(new RegExp(`--color-${token}:\\s*(#[0-9a-f]{6})`, 'i'));
  assert.ok(match, `missing --color-${token}`);
  return match[1];
}

test('the inline-style bridge references the canonical CSS palette', () => {
  for (const value of Object.values(colors)) {
    assert.match(value, /^var\(--color-[a-z-]+\)$/);
  }
});

test('CSS and JS share the same monospace stack', () => {
  assert.match(css, new RegExp(`--font-mono:\\s*${fonts.mono.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}`));
});

test('identity helpers keep the original palettes and hashes', () => {
  assert.equal(colorForSpaceId('Ada Lovelace'), '#5d8a6c');
  assert.equal(colorForSpaceId('Grace Hopper'), '#3f6c8c');
  assert.equal(colorForName('Ada Lovelace'), '#3d8a83');
  assert.equal(colorForAvatarName('Ada Lovelace'), '#c2410c');
  assert.equal(colorForUserId('Ada Lovelace'), '#c2410c');
  assert.equal(spaceMonogramColor, '#5d8a6c');
  assert.equal(terminalTheme.background, '#1d1c1a');
  assert.equal(terminalTheme.cursor, '#e2590b');
});

test('avatar kinds preserve the original letter counts', () => {
  assert.equal(avatarInitials('Ada Lovelace', 'member'), 'A');
  assert.equal(avatarInitials('Ada Lovelace', 'comment'), 'AL');
  assert.equal(avatarInitials('Alice', 'comment'), 'AL');
  assert.equal(avatarInitials('Ada Lovelace'), 'AL');
  assert.equal(avatarInitials('Alice'), 'A');
  assert.match(sourceOf('components/views/MembersView.tsx'), /kind="member"/);
  assert.match(sourceOf('components/InviteDialog.tsx'), /kind="member"/);
  assert.match(sourceOf('components/TransferDialog.tsx'), /kind="member"/);
  assert.match(sourceOf('components/PageCommentsLayer.tsx'), /kind="comment"/);
  assert.match(sourceOf('components/PageCommentsLayer.tsx'), /identityKey=\{id\}/);
});

test('CSS-variable colors are not composed as hex strings', () => {
  for (const { rel, source } of sourceFiles()) {
    assert.doesNotMatch(
      source,
      /\+\s*['"][0-9a-f]{2}['"]/i,
      `${rel} must use color-mix() rather than append a hex alpha channel`,
    );
  }
});

test('semantic foreground tokens remain readable on their intended surfaces', () => {
  const pairs = [
    ['accent-fill', 'on-accent'],
    ['green-fill', 'on-accent'],
    ['accent-ink', 'accent-soft'],
    ['green-ink', 'green-soft'],
    ['amber-ink', 'amber-soft'],
    ['red', 'red-soft'],
    ['blue', 'blue-soft'],
  ] as const;
  for (const [foreground, background] of pairs) {
    const ratio = contrastRatio(cssColor(foreground), cssColor(background));
    assert.ok(ratio >= 4.5, `${foreground}/${background} contrast is ${ratio.toFixed(2)}:1`);
  }
});

test('existing filled controls keep original accent/green rather than fill/ink roles', () => {
  assert.match(sourceOf('components/review/DiffView.tsx'), /background: C\.accent, color: C\.onAccent/);
  assert.match(sourceOf('pages/MainLayout.tsx'), /background: C\.accent, color: C\.onAccent/);
  assert.match(sourceOf('components/review/ReviewDetail.tsx'), /background: C\.green, color: C\.onAccent/);
  assert.match(sourceOf('components/review/ReviewDetail.tsx'), /background: C\.accentSoft, color: C\.accent/);
  assert.match(sourceOf('components/ui/inline-feedback.tsx'), /bg-amber-soft text-amber/);
  assert.match(sourceOf('components/ui/inline-feedback.tsx'), /bg-green-soft text-green/);
});

test('Tailwind radii derive from the shared shadcn base radius', () => {
  assert.match(css, /--radius-sm:\s*calc\(var\(--radius\) - 4px\)/);
  assert.match(css, /--radius-md:\s*calc\(var\(--radius\) - 2px\)/);
  assert.match(css, /--radius-lg:\s*var\(--radius\)/);
  assert.match(css, /--radius-xl:\s*calc\(var\(--radius\) \+ 4px\)/);
});

test('search uses the shared accessible dialog primitive', () => {
  const searchModal = sourceOf('components/SearchModal.tsx');
  assert.match(searchModal, /import \{ Dialog, DialogContent, DialogTitle \}/);
  assert.match(searchModal, /<DialogTitle className="sr-only">Search<\/DialogTitle>/);
  assert.doesNotMatch(searchModal, /createPortal|role="dialog"/);
  assert.match(sourceOf('components/ui/dialog.tsx'), /bg-dialog-scrim/);
  assert.match(sourceOf('components/ui/dialog.tsx'), /shadow-\[var\(--shadow-dialog\)\]/);
});

test('source files do not hardcode hex colors outside the palette', () => {
  const allowedByFile = new Map<string, ReadonlySet<string>>([
    ['lib/design.ts', new Set([
      '#3f6c8c', '#5d8a6c', '#9a6f93', '#c2410c',
      '#5b679e', '#3d8a83', '#b07d2c', '#7a65a6',
      '#2f6bb0', '#2f8a5b', '#b5790f',
      '#1d1c1a', '#eeeae3', '#e2590b', '#5c5149',
    ])],
    ['components/editor/theme.ts', new Set([
      '#cf222e', '#0a3069', '#6e7781', '#0550ae',
      '#953800', '#8250df', '#116329',
    ])],
    // Excalidraw's default ink, paper and frame colors, drawn inside the drawing itself.
    ['lib/excalidraw.ts', new Set(['#1e1e1e', '#ffffff', '#bbbbbb', '#999999'])],
  ]);
  const hexPattern = /#(?:[0-9a-f]{8}|[0-9a-f]{6}|[0-9a-f]{3,4})\b/gi;
  const offenders: string[] = [];
  for (const { rel, source } of sourceFiles()) {
    const allowedValues = allowedByFile.get(rel) ?? new Set<string>();
    const matches = source.match(hexPattern) ?? [];
    for (const match of matches) {
      if (!allowedValues.has(match.toLowerCase())) {
        offenders.push(`${rel}: ${match}`);
      }
    }
  }
  assert.deepEqual(offenders, [], 'hardcoded hex colors must go through src/lib/design.ts tokens');
});

test('source files do not bypass tokens with raw color functions', () => {
  const allowedByFile = new Map<string, ReadonlySet<string>>([
    ['lib/design.ts', new Set([
      'rgba(226, 89, 11, 0.18)',
      'rgba(29, 28, 26, 0.12)',
      'rgba(29, 28, 26, 0.04)',
      'rgba(29, 28, 26, 0.06)',
      'rgba(29, 28, 26, 0.03)',
      'rgba(29, 28, 26, 0.08)',
      'rgba(29, 28, 26, 0.25)',
      'rgba(0, 0, 0, 0.04)',
      'rgba(0, 0, 0, 0.12)',
      'rgba(0, 0, 0, 0.4)',
      'rgba(29, 28, 26, 0.08)',
      'rgba(29, 28, 26, 0.05)',
    ])],
  ]);
  const colorFunctionPattern = /\b(?:rgb|hsl)a?\([^)]*\)/gi;
  const offenders: string[] = [];
  for (const { rel, source } of sourceFiles()) {
    const allowedValues = allowedByFile.get(rel) ?? new Set<string>();
    for (const match of source.match(colorFunctionPattern) ?? []) {
      if (!allowedValues.has(match)) offenders.push(`${rel}: ${match}`);
    }
  }
  assert.deepEqual(offenders, [], 'raw color functions must use tokens or documented definitions');
});

test('source files use semantic Tailwind color utilities', () => {
  const fixedPalettePattern = /\b(?:bg|border|fill|stroke|text)-(?:red|orange|amber|yellow|lime|green|emerald|teal|cyan|sky|blue|indigo|violet|purple|fuchsia|pink|rose|slate|gray|zinc|neutral|stone)-\d{2,3}\b/g;
  const rawVariablePattern = /\b(?:bg|border|fill|stroke|text)-\[var\(--color-[^)]+\)\]/g;
  const literalNeutralPattern = /\b(?:bg|border|text)-(?:black|white)(?:\/\d+)?\b/g;
  const offenders: string[] = [];
  for (const { rel, source } of sourceFiles()) {
    for (const pattern of [fixedPalettePattern, rawVariablePattern, literalNeutralPattern]) {
      for (const match of source.match(pattern) ?? []) offenders.push(`${rel}: ${match}`);
    }
  }
  assert.deepEqual(offenders, [], 'Tailwind colors must use CoWiki semantic token utilities');
});

test('space monograms keep the original tile green and shared foreground token', () => {
  const cloudHome = sourceOf('cloud/CloudHome.tsx');
  assert.match(cloudHome, /spaceMonogramColor/);
  assert.match(cloudHome, /text-on-accent/);
  assert.doesNotMatch(sourceOf('cloud/CloudInvitationPage.tsx'), /identityKey/);
});

test('review detail views share the ReviewBackButton chrome control', () => {
  const reviewBackButton = sourceOf('components/review/ReviewBackButton.tsx');
  assert.match(reviewBackButton, /<button/);
  assert.doesNotMatch(reviewBackButton, /from '@\/components\/ui\/button'/);
  const consumers = [
    'components/review/ReviewDetail.tsx',
    'components/review/LocalReviewDetail.tsx',
    'cloud/CloudReviewsView.tsx',
  ];
  for (const rel of consumers) {
    const source = sourceOf(rel);
    assert.match(source, /import \{ ReviewBackButton \}/, `${rel} must use the shared back button`);
    assert.doesNotMatch(source, /const backBtnStyle|const backButtonStyle/, `${rel} must not define its own back button style`);
  }
});

test('components never use a platform control that has a CoWiki replacement', () => {
  const offenders: string[] = [];
  for (const { rel, source } of sourceFiles()) {
    if (rel.startsWith('components/ui/')) continue;
    if (/<select\b/.test(source)) offenders.push(`${rel}: <select> (use components/ui/select)`);
    if (/type=["']number["']/.test(source)) offenders.push(`${rel}: number input (use components/ui/number-field)`);
    if (/type=["']date["']/.test(source)) offenders.push(`${rel}: date input (use components/ui/date-picker)`);
  }
  assert.deepEqual(offenders, [], 'see DESIGN_SYSTEM.md, "Form controls"');
});

test('checkboxes, switches, focus and bare borders are drawn by the design system', () => {
  assert.match(css, /input\[type="checkbox"\] \{[^}]*appearance: none/);
  assert.match(css, /input\[type="checkbox"\]\.cw-switch \{/);
  assert.match(css, /input\[type="number"\]::-webkit-inner-spin-button/);
  assert.match(css, /:focus-visible \{\s*outline: 2px solid color-mix\(in srgb, var\(--color-accent\)/);
  assert.match(css, /::file-selector-button \{ border-color: var\(--color-border\); \}/);
  for (const token of ['popover', 'card', 'chip', 'rest']) {
    assert.match(css, new RegExp(`--shadow-${token}:`), `missing --shadow-${token}`);
  }
});

test('every colour token has a dark value', () => {
  const block = (start: RegExp) => {
    const at = css.search(start);
    assert.ok(at >= 0, `missing ${start}`);
    return css.slice(at, css.indexOf('\n}', at));
  };
  const names = (source: string, prefix: string) => [...source.matchAll(new RegExp(`(--${prefix}[\\w-]*):`, 'g'))].map((match) => match[1]);
  const light = block(/@theme \{/);
  const dark = block(/html\.dark \{/);
  const missing = names(light, 'color-').filter((name) => !names(dark, 'color-').includes(name));
  assert.deepEqual(missing, [], 'see DESIGN_SYSTEM.md, "Dark theme"');
  const shadcn = block(/:root \{\n {2}\/\* ── CoWiki shadcn theme/);
  for (const prefix of ['background', 'foreground', 'card', 'popover', 'primary', 'secondary', 'muted', 'accent', 'destructive', 'border', 'input', 'ring', 'shadow-', 'sidebar']) {
    const absent = names(shadcn, prefix).filter((name) => !names(dark, prefix).includes(name));
    assert.deepEqual(absent, [], `dark theme misses ${absent.join(', ')}`);
  }
});

test('labels are sentence case and tool views share the sans view title', () => {
  const caps: string[] = [];
  for (const { rel, source } of sourceFiles()) {
    if (/\buppercase\b|textTransform: ['"]uppercase['"]/.test(source)) caps.push(rel);
  }
  assert.deepEqual(caps, [], 'no ALL CAPS labels (DESIGN_SYSTEM.md, "Interface principles")');
  const views = sourceFiles().filter(({ rel }) => /^components\/(views|review|notifications)\/|^cloud\/Cloud(Members|Reviews|Notifications)/.test(rel));
  assert.ok(views.length >= 10);
  for (const { rel, source } of views) {
    assert.doesNotMatch(source, /page-title|font-serif|fonts\.serif/, `${rel}: use ViewHeader / .view-title`);
  }
  assert.match(css, /\.view-title \{[^}]*font-family: var\(--font-sans\)/);
});

test('interface text uses the type scale', () => {
  // DESIGN_SYSTEM.md, "Type scale": 11 · 12 · 13 · 14 · 16, then display
  // sizes (22 and up) for titles. Content uses the --doc-* tokens.
  const scale = new Set([11, 12, 13, 14, 16]);
  const off: string[] = [];
  for (const { rel, source } of sourceFiles()) {
    for (const match of source.matchAll(/text-\[(\d+(?:\.\d+)?)px\]/g)) off.push(`${rel}: text-[${match[1]}px] (use text-caption … text-title)`);
    for (const match of source.matchAll(/fontSize: ?(\d+(?:\.\d+)?)\b(?!px|em|')/g)) {
      const size = Number(match[1]);
      if (!scale.has(size) && size < 20) off.push(`${rel}: fontSize ${size}`);
    }
  }
  for (const match of css.matchAll(/font-size: ?(\d+(?:\.\d+)?)px/g)) {
    const size = Number(match[1]);
    if (!scale.has(size) && size < 20) off.push(`index.css: font-size ${size}px`);
  }
  assert.deepEqual(off, []);
  for (const [name, size] of [['caption', 11], ['meta', 12], ['small', 13], ['body', 14], ['lead', 16], ['title', 22]] as const) {
    assert.match(css, new RegExp(`--text-${name}: ${size}px;`));
  }
});

test('transitions name what they animate', () => {
  // Web Interface Guidelines: never `transition: all` (it animates layout and
  // colour changes nobody asked for).
  const all = sourceFiles().filter(({ source }) => /transition-all\b|transition: ?['"]all\b/.test(source)).map(({ rel }) => rel);
  assert.deepEqual(all, []);
  assert.doesNotMatch(css, /transition: ?all\b/);
});
