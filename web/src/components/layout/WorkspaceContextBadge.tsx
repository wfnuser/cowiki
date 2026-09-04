import { Cloud } from 'lucide-react';
import { C } from '@/lib/design';
import type { WorkspaceContextStatus } from '@/lib/workspace-context';

export function WorkspaceContextBadge({
  context,
  onClick,
  connected = true,
  style,
}: {
  context: WorkspaceContextStatus;
  onClick?: () => void;
  connected?: boolean;
  style?: React.CSSProperties;
}) {
  const actionStyle: React.CSSProperties = {
    display: 'inline-flex',
    alignItems: 'center',
    gap: 6,
    flexShrink: 0,
    padding: '6px 9px',
    border: 'none',
    borderRadius: 8,
    background: 'transparent',
    color: C.muted,
    fontSize: 12.5,
    fontWeight: 500,
    whiteSpace: 'nowrap',
    cursor: 'pointer',
    ...style,
  };

  return (
    <button
      type="button"
      onClick={onClick}
      title={connected ? context.detail : 'Publish to Cloud'}
      style={actionStyle}
    >
      <Cloud size={14} />
      <span>{connected ? 'Cloud' : 'Publish to Cloud'}</span>
    </button>
  );
}
