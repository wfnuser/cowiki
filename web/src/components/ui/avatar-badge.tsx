import {
  C, avatarInitials, colorForAvatarName, colorForName, colorForUserId, radii,
  type AvatarKind,
} from '@/lib/design';

interface AvatarBadgeProps {
  name: string;
  size?: number;
  color?: string;
  /** Stable key for surfaces that identify people independently of display name. */
  identityKey?: string;
  /**
   * Historical identity: members show one letter + the djb2 tile palette;
   * comments keep the two-character / user-id palette; everyone else uses
   * AvatarBadge's original word-initials + name palette.
   */
  kind?: AvatarKind;
}

function avatarColor(name: string, kind: AvatarKind, identityKey?: string, color?: string): string {
  if (color) return color;
  if (kind === 'member') return colorForName(name);
  if (identityKey) return colorForUserId(identityKey);
  return colorForAvatarName(name);
}

/** Colored-initials avatar matching the design (Avatar in cowiki-shell). */
export function AvatarBadge({
  name,
  size = 24,
  color,
  identityKey,
  kind = 'default',
}: AvatarBadgeProps) {
  const comment = kind === 'comment';
  return (
    <span
      title={name}
      style={{
        width: size, height: size, borderRadius: radii.full,
        background: avatarColor(name, kind, identityKey, color),
        color: C.onAccent,
        display: 'flex', alignItems: 'center', justifyContent: 'center',
        fontSize: size * (comment ? 0.42 : 0.4),
        fontWeight: 600, flexShrink: 0,
        letterSpacing: comment ? '-0.02em' : '0.02em',
        userSelect: 'none',
      }}
    >
      {avatarInitials(name, kind)}
    </span>
  );
}

export default AvatarBadge;
