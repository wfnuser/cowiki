import { Cloud, HardDrive } from 'lucide-react';
import { C } from '@/lib/design';
import type { WorkspaceContextStatus } from '@/lib/workspace-context';

export function WorkspaceContextBadge({
  context,
  onClick,
}: {
  context: WorkspaceContextStatus;
  onClick?: () => void;
}) {
  const content = (
    <>
      {context.kind === 'cloud' ? <Cloud size={11} /> : <HardDrive size={11} />}
      <span>{context.label}</span>
    </>
  );
  const style: React.CSSProperties = {
    display: 'inline-flex',
    alignItems: 'center',
    gap: 5,
    flexShrink: 0,
    padding: '3px 7px',
    border: `1px solid ${context.attention ? C.accentTintBorder : C.line}`,
    borderRadius: 999,
    background: context.attention ? C.accentSoft : C.sidebar,
    color: context.attention ? C.accent : C.muted,
    fontSize: 10.5,
    fontWeight: 650,
    lineHeight: 1,
    whiteSpace: 'nowrap',
  };

  if (onClick) {
    return (
      <button type="button" onClick={onClick} title={context.detail} style={{ ...style, cursor: 'pointer' }}>
        {content}
      </button>
    );
  }
  return <span title={context.detail} style={style}>{content}</span>;
}
