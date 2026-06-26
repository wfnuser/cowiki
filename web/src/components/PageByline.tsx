import { C } from '@/lib/design';
import { AvatarBadge } from './ui/avatar-badge';

/** "● Author · Edited Jan 9, 2024" byline above a wiki page, from git history. */
export function PageByline({ name, editedAt }: { name?: string | null; editedAt?: number | null }) {
  if (!name) return null;
  const date = editedAt
    ? new Date(editedAt * 1000).toLocaleDateString('en-US', { year: 'numeric', month: 'short', day: 'numeric' })
    : null;
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 11, marginBottom: 20 }}>
      <AvatarBadge name={name} size={28} />
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
