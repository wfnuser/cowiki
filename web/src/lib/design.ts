export const colors = {
  bg: '#faf9f7',
  panel: '#fdfcfb',
  sidebar: '#f5f4f1',
  rail: '#efedea',
  ink: '#1d1c1a',
  ink2: '#403e3a',
  muted: '#8c897f',
  faint: '#a8a59b',
  line: '#e8e6e1',
  lineSoft: '#eeece8',
  accent: '#e2590b',
  accentHover: '#c94b08',
  accentSoft: '#fbeadd',
  green: '#2f8a5b',
  greenSoft: '#e3f1e9',
  greenBg: '#dafbe1',
  greenBgSoft: '#e6f9ec',
  red: '#cf222e',
  redSoft: '#fcebe8',
  redBg: '#ffebe9',
  redBgSoft: '#fff5f4',
  amber: '#b5790f',
  amberSoft: '#f6ecd6',
  blue: '#2f6bb0',
  blueSoft: '#e6eef7',
  purple: '#8250df',
  tag: '#ece8e1',
} as const;

export const fonts = {
  sans: 'var(--font-sans)',
  serif: 'var(--font-serif)',
  mono: "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
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

export const shadows = {
  focus: '0 0 0 3px rgba(226, 89, 11, 0.18)',
  raised: '0 8px 28px rgba(29, 28, 26, 0.12), 0 0 0 1px rgba(29, 28, 26, 0.04)',
  subtle: '0 1px 2px rgba(29, 28, 26, 0.06)',
} as const;

export const APP_HEADER_HEIGHT = 48;

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

export const C = colors;
