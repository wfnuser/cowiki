import { Cloud, HardDrive } from 'lucide-react';
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
    fontSize: 12.5,
    fontWeight: 500,
    whiteSpace: 'nowrap',
    cursor: onClick ? 'pointer' : 'default',
    ...style,
    color: context.attention ? C.amber : C.muted,
  };

  return (
    <button
      type="button"
      onClick={onClick}
      title={connected ? context.detail : `${context.detail}. Click to publish to Cloud.`}
      style={actionStyle}
    >
      {context.kind === 'local' ? <HardDrive size={14} /> : <Cloud size={14} />}
      <span>{context.label}</span>
    </button>
  );
}
