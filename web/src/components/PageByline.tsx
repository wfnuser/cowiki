import { C } from '@/lib/design';

const BYLINE_COLORS = ['#2f6bb0', '#2f8a5b', '#8b5cf6', '#c2410c', '#e2590b', '#3f6c8c'];
function colorFor(s: string): string {
  let h = 0;
  for (let i = 0; i < s.length; i++) h = (h * 31 + s.charCodeAt(i)) >>> 0;
  return BYLINE_COLORS[h % BYLINE_COLORS.length];
}
function initials(name: string): string {
  const p = name.trim().split(/\s+/).filter(Boolean);
  if (!p.length) return '?';
  return (p.length === 1 ? p[0].slice(0, 2) : p[0][0] + p[1][0]).toUpperCase();
}

/** "● Author · Edited Jan 9, 2024" byline above a wiki page, from git history. */
export function PageByline({ name, editedAt }: { name?: string | null; editedAt?: number | null }) {
  if (!name) return null;
  const date = editedAt
    ? new Date(editedAt * 1000).toLocaleDateString('en-US', { year: 'numeric', month: 'short', day: 'numeric' })
    : null;
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 11, marginBottom: 20 }}>
      <div style={{
        width: 28, height: 28, borderRadius: '50%', background: colorFor(name), color: '#fff',
        display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: 12, fontWeight: 600,
      }}>
        {initials(name)}
      </div>
      <span style={{ fontSize: 13.5, color: C.muted }}>{name}</span>
      {date && (
        <>
          <span style={{ color: C.line }}>·</span>
          <span style={{ fontSize: 13.5, color: C.faint }}>Edited {date}</span>
        </>
      )}
    </div>
  );
}

export default PageByline;
