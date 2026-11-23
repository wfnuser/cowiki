import { Check, ChevronDown, Cloud, HardDrive, Rows3 } from 'lucide-react';

import type { AgentChange } from '@/api';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { C } from '@/lib/design';
import type { WorkspaceContextStatus } from '@/lib/workspace-context';

export type VersionSelection =
  | { kind: 'working' }
  | { kind: 'upstream' }
  | { kind: 'agent'; changeId: string };

type SpaceStatusMenuProps = {
  /** Where this Space lives and whether its changes are uploaded. */
  context: WorkspaceContextStatus;
  agentChanges: AgentChange[];
  selection: VersionSelection;
  onSelect: (selection: VersionSelection) => void;
  onSeeReviews: () => void;
  /** Publish, sync or sign in; null hides it. */
  cloudAction: { label: string; onSelect: () => void } | null;
  /** Pages without versions (HTML files) only show the status. */
  versions?: boolean;
};

/**
 * One quiet control for the Space's state: which version you are reading and
 * where the Space lives. A dot appears only when changes are not uploaded or
 * need attention; the words are in the menu.
 */
export function SpaceStatusMenu({
  context,
  agentChanges,
  selection,
  onSelect,
  onSeeReviews,
  cloudAction,
  versions = true,
}: SpaceStatusMenuProps) {
  const selectedAgent = selection.kind === 'agent'
    ? agentChanges.find((change) => change.id === selection.changeId)
    : undefined;
  const label = selection.kind === 'working'
    ? 'Draft'
    : selection.kind === 'upstream'
      ? 'main'
      : selectedAgent?.title ?? 'Agent change';
  const Place = context.kind === 'local' ? HardDrive : Cloud;
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          aria-label={`${label} · ${context.label}`}
          title={context.detail}
          style={triggerStyle}
          className="hover:bg-bg-hover data-[state=open]:bg-bg-hover"
        >
          <span style={{ position: 'relative', display: 'inline-flex', color: C.faint }}>
            <Place size={14} />
            {context.attention && (
              <span data-testid="space-status-dot" aria-hidden style={dotStyle} />
            )}
          </span>
          <span style={{ maxWidth: 150, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{label}</span>
          <ChevronDown aria-hidden size={13} strokeWidth={1.75} style={{ color: C.faint, flexShrink: 0 }} />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-72" sideOffset={6}>
        <div className="flex gap-2.5 px-2 pb-2 pt-1.5">
          <Place size={14} className="mt-0.5 shrink-0 text-text-tertiary" />
          <span className="min-w-0">
            <span className="block text-[13px] font-medium text-text">{context.label}</span>
            <span className="block text-[12px] leading-4 text-text-tertiary">{context.detail}</span>
          </span>
        </div>

        {versions && (
          <>
            <DropdownMenuSeparator className="my-1" />
            <DropdownMenuLabel className="px-2 pb-1 pt-1.5 text-[12px] font-medium text-text-tertiary">
              Version
            </DropdownMenuLabel>
            <VersionItem
              label="Current Draft"
              meta="Your working copy"
              color={C.accent}
              selected={selection.kind === 'working'}
              onSelect={() => onSelect({ kind: 'working' })}
            />
            <VersionItem
              label="main"
              meta="Latest checkpoint · read-only"
              color={C.purple}
              selected={selection.kind === 'upstream'}
              onSelect={() => onSelect({ kind: 'upstream' })}
            />
            {agentChanges.map((change) => (
              <VersionItem
                key={change.id}
                label={change.title}
                meta={`Agent change · ${change.diffs.length} ${change.diffs.length === 1 ? 'file' : 'files'}`}
                color={C.blue}
                selected={selection.kind === 'agent' && selection.changeId === change.id}
                onSelect={() => onSelect({ kind: 'agent', changeId: change.id })}
              />
            ))}
            <DropdownMenuItem onSelect={onSeeReviews} className="text-[13px] text-text-secondary">
              <Rows3 />
              See all in Reviews
            </DropdownMenuItem>
          </>
        )}

        {cloudAction && (
          <>
            <DropdownMenuSeparator className="my-1" />
            <DropdownMenuItem onSelect={cloudAction.onSelect} className="text-[13px] text-text-secondary">
              <Cloud />
              {cloudAction.label}
            </DropdownMenuItem>
          </>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function VersionItem({
  color,
  label,
  meta,
  onSelect,
  selected,
}: {
  color: string;
  label: string;
  meta: string;
  onSelect: () => void;
  selected: boolean;
}) {
  return (
    <DropdownMenuItem onSelect={onSelect} className="gap-2.5 px-2 py-1.5">
      <span aria-hidden style={{ width: 7, height: 7, borderRadius: '50%', flexShrink: 0, background: color }} />
      <span className="min-w-0 flex-1">
        <span className="block truncate text-[13px] font-medium text-text">{label}</span>
        <span className="block truncate text-[12px] text-text-tertiary">{meta}</span>
      </span>
      {selected ? <Check size={14} style={{ color: C.accent }} /> : <span style={{ width: 14 }} />}
    </DropdownMenuItem>
  );
}

const triggerStyle: React.CSSProperties = {
  height: 28,
  display: 'inline-flex',
  alignItems: 'center',
  gap: 7,
  padding: '0 8px',
  border: 'none',
  borderRadius: 7,
  color: C.ink2,
  fontSize: 12.5,
  fontWeight: 500,
  cursor: 'pointer',
};

const dotStyle: React.CSSProperties = {
  position: 'absolute',
  top: -2,
  right: -3,
  width: 7,
  height: 7,
  borderRadius: '50%',
  background: C.amber,
  boxShadow: `0 0 0 2px ${C.panel}`,
};
