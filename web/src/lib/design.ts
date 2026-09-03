/** CSS-variable bridge for inline-style and editor-theme call sites. */
export const colors = {
  bg: 'var(--color-bg)',
  panel: 'var(--color-panel)',
  sidebar: 'var(--color-bg-secondary)',
  rail: 'var(--color-bg-hover)',
  ink: 'var(--color-text)',
  ink2: 'var(--color-text-secondary)',
  muted: 'var(--color-text-tertiary)',
  faint: 'var(--color-text-faint)',
  line: 'var(--color-border)',
  lineSoft: 'var(--color-border-soft)',
  accent: 'var(--color-accent)',
  accentHover: 'var(--color-accent-hover)',
  accentFill: 'var(--color-accent-fill)',
  accentSoft: 'var(--color-accent-soft)',
  accentSoftHover: 'var(--color-accent-soft-hover)',
  accentInk: 'var(--color-accent-ink)',
  onAccent: 'var(--color-on-accent)',
  green: 'var(--color-green)',
  greenFill: 'var(--color-green-fill)',
  greenSoft: 'var(--color-green-soft)',
  greenInk: 'var(--color-green-ink)',
  greenBg: 'var(--color-green-bg)',
  greenBgSoft: 'var(--color-green-bg-soft)',
  red: 'var(--color-red)',
  redSoft: 'var(--color-red-soft)',
  redBg: 'var(--color-red-bg)',
  redBgSoft: 'var(--color-red-bg-soft)',
  amber: 'var(--color-amber)',
  amberSoft: 'var(--color-amber-soft)',
  amberInk: 'var(--color-amber-ink)',
  blue: 'var(--color-blue)',
  blueSoft: 'var(--color-blue-soft)',
  purple: 'var(--color-purple)',
  tag: 'var(--color-tag)',
  // Historical tints kept as named tokens so TSX does not hardcode them.
  accentTintBorder: 'var(--color-accent-tint-border)',
  accentWash: 'var(--color-accent-wash)',
  accentWashSoft: 'var(--color-accent-wash-soft)',
  accentTintLine: 'var(--color-accent-tint-line)',
  greenTintBorder: 'var(--color-green-tint-border)',
  diffMute: 'var(--color-diff-mute)',
  codeBg: 'var(--color-code-bg)',
  codeFg: 'var(--color-code-fg)',
  scrim: 'var(--color-scrim)',
} as const;

export const fonts = {
  sans: 'var(--font-sans)',
  serif: 'var(--font-serif)',
  // Concrete stack: xterm measures this on a canvas and cannot resolve CSS variables.
  mono: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
} as const;

export const radii = {
  xs: 4,
  sm: 6,
  md: 8,
  lg: 10,
  xl: 12,
  tile: 13,
  full: 999,
} as const;

/**
 * Named after the surfaces that already used these exact values.
 * Prefer `raised` / `subtle` / `focus` for new chrome; the rest exist so this
 * cleanup does not restyle floating comments, toasts, or the login card.
 */
export const shadows = {
  focus: '0 0 0 3px rgba(226, 89, 11, 0.18)',
  raised: '0 8px 28px rgba(29, 28, 26, 0.12), 0 0 0 1px rgba(29, 28, 26, 0.04)',
  lifted: '0 8px 28px rgba(29, 28, 26, 0.12)',
  subtle: '0 1px 2px rgba(29, 28, 26, 0.06)',
  faint: '0 1px 2px rgba(29, 28, 26, 0.03)',
  tick: '0 1px 2px rgba(0, 0, 0, 0.04)',
  toast: '0 2px 10px rgba(29, 28, 26, 0.08)',
  overlay: '0 4px 16px rgba(29, 28, 26, 0.12)',
  float: '0 4px 16px -10px rgba(29, 28, 26, 0.25)',
  popover: '0 6px 18px rgba(0, 0, 0, 0.12)',
  comment: '0 10px 28px -8px rgba(0, 0, 0, 0.4)',
  login: '0 12px 36px rgba(29, 28, 26, 0.08), 0 1px 4px rgba(29, 28, 26, 0.05)',
} as const;

export const APP_HEADER_HEIGHT = 48;

/** xterm cannot consume CSS variables; these match the warm ink/accent palette. */
export const terminalTheme = {
  background: '#1d1c1a',
  foreground: '#eeeae3',
  cursor: '#e2590b',
  selectionBackground: '#5c5149',
} as const;

function hashCharCodes(value: string): number {
  return Array.from(value).reduce((sum, char) => sum + char.charCodeAt(0), 0);
}

function hashDjb2(value: string): number {
  let hash = 0;
  for (let i = 0; i < value.length; i++) {
    hash = ((hash << 5) - hash) + value.charCodeAt(i);
    hash |= 0;
  }
  return hash === -2147483648 ? 0 : Math.abs(hash);
}

function hashTimes31(value: string): number {
  let hash = 0;
  for (let i = 0; i < value.length; i++) hash = (hash * 31 + value.charCodeAt(i)) >>> 0;
  return hash;
}

/** Space-rail / space-header tiles. Hash is the original sum-of-charCodes. */
export const spaceTileColors = [
  '#3f6c8c',
  '#5d8a6c',
  '#9a6f93',
  '#c2410c',
  '#6366f1',
  '#14b8a6',
  '#f59e0b',
  '#8b5cf6',
] as const;

/** Invitation-page monogram; historically this fixed tile green. */
export const spaceMonogramColor = spaceTileColors[1];

export function colorForSpaceId(id: string): string {
  return spaceTileColors[hashCharCodes(id) % spaceTileColors.length];
}

/**
 * Members / invite / transfer avatars. Same djb2 + space-tile palette as
 * before those screens shared AvatarBadge.
 */
export function colorForName(name: string): string {
  return spaceTileColors[hashDjb2(name) % spaceTileColors.length];
}

/** AvatarBadge name hash (`h * 31`) and its original palette. */
export const avatarColors = [
  '#2f6bb0',
  '#2f8a5b',
  '#8b5cf6',
  '#c2410c',
  '#3f6c8c',
  '#5d8a6c',
  '#9a6f93',
  '#b5790f',
] as const;

export function colorForAvatarName(name: string): string {
  return avatarColors[hashTimes31(name) % avatarColors.length];
}

/** Comments hashed user ids with a slightly different palette order. */
export const commentAvatarColors = [
  '#2f6bb0',
  '#2f8a5b',
  '#8b5cf6',
  '#c2410c',
  '#3f6c8c',
  '#9a6f93',
  '#5d8a6c',
  '#b5790f',
] as const;

export function colorForUserId(id: string): string {
  return commentAvatarColors[hashTimes31(id) % commentAvatarColors.length];
}

export type AvatarKind = 'default' | 'member' | 'comment';

/** Letter counts as they originally appeared on each surface. */
export function avatarInitials(name: string, kind: AvatarKind = 'default'): string {
  const trimmed = name.trim();
  if (!trimmed) return '?';
  if (kind === 'member') return trimmed[0].toUpperCase();
  const parts = trimmed.split(/\s+/).filter(Boolean);
  if (kind === 'comment') {
    if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
    return `${parts[0][0]}${parts[1][0]}`.toUpperCase();
  }
  return parts.map((word) => word[0]).slice(0, 2).join('').toUpperCase();
}

export const C = colors;
