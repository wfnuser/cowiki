import { C } from '@/lib/design';

/** Deterministic avatar palette from the design handoff (muted, warm-compatible). */
const PALETTE = ['#2f6bb0', '#2f8a5b', '#8b5cf6', '#c2410c', '#3f6c8c', '#5d8a6c', '#9a6f93', '#b5790f'];

export function nameColor(name: string): string {
  let h = 0;
  for (let i = 0; i < name.length; i++) h = (h * 31 + name.charCodeAt(i)) >>> 0;
  return PALETTE[h % PALETTE.length];
}

/** Colored-initials avatar matching the design (Avatar in cowiki-shell). */
export function AvatarBadge({ name, size = 24, color }: { name: string; size?: number; color?: string }) {
  const initials = name
    .split(/\s+/)
    .map((w) => w[0])
    .filter(Boolean)
    .slice(0, 2)
    .join('')
    .toUpperCase() || '?';
  return (
    <div
      title={name}
      style={{
        width: size, height: size, borderRadius: 999,
        background: color ?? nameColor(name), color: '#fff',
        display: 'flex', alignItems: 'center', justifyContent: 'center',
        fontSize: size * 0.4, fontWeight: 600, flexShrink: 0, letterSpacing: '0.02em',
        userSelect: 'none',
      }}
    >
      {initials}
    </div>
  );
}

export default AvatarBadge;

// keep design tokens import referenced for future tints
void C;
