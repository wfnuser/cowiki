import { Check, ChevronDown, GitBranch, Rows3 } from 'lucide-react';

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

export type VersionSelection =
  | { kind: 'working' }
  | { kind: 'upstream' }
  | { kind: 'agent'; changeId: string };

type VersionSwitcherProps = {
  agentChanges: AgentChange[];
  selection: VersionSelection;
  onSelect: (selection: VersionSelection) => void;
  onSeeReviews: () => void;
};

export function VersionSwitcher({
  agentChanges,
  selection,
  onSelect,
  onSeeReviews,
}: VersionSwitcherProps) {
  const selectedAgent = selection.kind === 'agent'
    ? agentChanges.find((change) => change.id === selection.changeId)
    : undefined;
  const label = selection.kind === 'working'
    ? 'Current Draft'
    : selection.kind === 'upstream'
      ? 'main'
      : selectedAgent?.title ?? 'Agent Change';
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          aria-label="Switch version"
          title="Switch version"
          style={triggerStyle}
        >
          <GitBranch size={14} />
          <span style={{ maxWidth: 150, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{label}</span>
          <ChevronDown
            aria-hidden
            size={13}
            strokeWidth={1.75}
            style={{ color: C.faint, flexShrink: 0 }}
          />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-72" sideOffset={6}>
        <DropdownMenuLabel className="px-2 pb-1 pt-2 text-[10px] font-bold tracking-[0.09em] text-text-tertiary">
          WORKING
        </DropdownMenuLabel>
        <VersionItem
          label="Current Draft"
          meta="Local working tree"
          color={C.accent}
          selected={selection.kind === 'working'}
          onSelect={() => onSelect({ kind: 'working' })}
        />

        <DropdownMenuLabel className="px-2 pb-1 pt-3 text-[10px] font-bold tracking-[0.09em] text-text-tertiary">
          UPSTREAM
        </DropdownMenuLabel>
        <VersionItem
          label="main"
          meta="Latest local checkpoint · read-only"
          color={C.purple}
          selected={selection.kind === 'upstream'}
          onSelect={() => onSelect({ kind: 'upstream' })}
        />

        <DropdownMenuLabel className="px-2 pb-1 pt-3 text-[10px] font-bold tracking-[0.09em] text-text-tertiary">
          AGENT CHANGES
        </DropdownMenuLabel>
        {agentChanges.length ? agentChanges.map((change) => (
          <VersionItem
            key={change.id}
            label={change.title}
            meta={`${change.diffs.length} changed ${change.diffs.length === 1 ? 'file' : 'files'} · Open`}
            color={C.blue}
            selected={selection.kind === 'agent' && selection.changeId === change.id}
            onSelect={() => onSelect({ kind: 'agent', changeId: change.id })}
          />
        )) : (
          <div className="px-2 py-2 text-xs text-text-tertiary">No open Agent Changes</div>
        )}

        <DropdownMenuSeparator className="my-1" />
        <DropdownMenuItem onSelect={onSeeReviews} className="text-xs text-text-secondary">
          <Rows3 />
          See All in Reviews
        </DropdownMenuItem>
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
    <DropdownMenuItem onSelect={onSelect} className="gap-2.5 px-2 py-2">
      <span aria-hidden style={{ width: 8, height: 8, borderRadius: '50%', flexShrink: 0, background: color }} />
      <span className="min-w-0 flex-1">
        <span className="block truncate text-[13px] font-semibold text-text">{label}</span>
        <span className="block truncate font-mono text-[10.5px] text-text-tertiary">{meta}</span>
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
  padding: '0 10px',
  border: `1px solid ${C.line}`,
  borderRadius: 7,
  background: C.panel,
  color: C.ink2,
  fontSize: 12,
  fontWeight: 550,
  cursor: 'pointer',
};
