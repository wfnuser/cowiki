import { Plus, Settings, Compass, LogOut } from 'lucide-react';
import type { Workspace } from '../../api';
import {
  Tooltip, TooltipContent, TooltipTrigger,
} from '@/components/ui/tooltip';
import {
  DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';

/* ── Design tokens ── */
const C = {
  bg: '#faf9f7',
  panel: '#fdfcfb',
  sidebar: '#f5f4f1',
  rail: '#efedea',
  ink: '#1d1c1a',
  ink2: '#403e3a',
  muted: '#8c897f',
  faint: '#a8a59b',
  line: '#e8e6e1',
  accent: '#e2590b',
  green: '#2f8a5b',
} as const;

const TILE_COLORS = ['#3f6c8c', '#5d8a6c', '#9a6f93', '#c2410c', '#6366f1', '#14b8a6', '#f59e0b', '#8b5cf6'];

function tileColor(index: number): string {
  return TILE_COLORS[index % TILE_COLORS.length];
}

interface SpaceRailProps {
  workspaces: Workspace[];
  activeWorkspaceId: string | null;
  userName: string;
  onSelectWorkspace: (ws: Workspace) => void;
  onCreateWorkspace: () => void;
  onSettings: () => void;
  onDiscover: () => void;
  onLogout: () => void;
}

export function SpaceRail({
  workspaces,
  activeWorkspaceId,
  userName,
  onSelectWorkspace,
  onCreateWorkspace,
  onSettings,
  onDiscover,
  onLogout,
}: SpaceRailProps) {
  const personalSpaces = workspaces.filter((w) => w.visibility === 'private' && w.role === 'owner');
  const teamSpaces = workspaces.filter((w) => !(w.visibility === 'private' && w.role === 'owner'));

  return (
    <aside style={{
      width: 64, minWidth: 64, height: '100vh',
      background: C.rail, borderRight: `1px solid ${C.line}`,
      display: 'flex', flexDirection: 'column', alignItems: 'center',
      padding: '0 0 12px', gap: 0, position: 'sticky', top: 0,
      zIndex: 20,
    }}>
      {/* Logo — matches panel header height (52px) */}
      <div style={{
        width: 64, height: 52, display: 'flex', alignItems: 'center', justifyContent: 'center',
        cursor: 'default', flexShrink: 0,
      }}>
        <span style={{
          fontFamily: '"Source Serif 4 Variable", Georgia, serif',
          fontWeight: 700, fontSize: 22, color: C.ink, letterSpacing: '-0.02em',
        }}>
          C<span style={{ color: C.accent }}>.</span>
        </span>
      </div>

      {/* Separator */}
      <div style={{ width: 26, height: 1, background: C.line, marginBottom: 6 }} />

      {/* Personal space tiles */}
      {personalSpaces.map((ws) => (
        <SpaceTile
          key={ws.id}
          workspace={ws}
          active={activeWorkspaceId === ws.id}
          onClick={() => onSelectWorkspace(ws)}
          color={C.accent}
          style={{
            background: C.accent,
            color: '#fff',
          }}
          icon={<svg width={21} height={21} viewBox="0 0 24 24" fill="none" stroke="#fff" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round"><circle cx="10" cy="7" r="3.4"/><path d="M4 19.5a6 6 0 0 1 10.5-4"/><rect x="14.5" y="15" width="6.5" height="5.5" rx="1"/><path d="M16 15v-1.2a1.75 1.75 0 0 1 3.5 0V15"/></svg>}
        />
      ))}

      {/* Team space tiles */}
      {teamSpaces.map((ws, i) => (
        <SpaceTile
          key={ws.id}
          workspace={ws}
          active={activeWorkspaceId === ws.id}
          onClick={() => onSelectWorkspace(ws)}
          color={tileColor(i)}
          style={{
            background: tileColor(i),
            color: '#fff',
          }}
        />
      ))}

      {/* Add button */}
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            onClick={onCreateWorkspace}
            style={{
              width: 42, height: 42, borderRadius: 13, border: `1.5px dashed ${C.faint}`,
              background: 'transparent', cursor: 'pointer',
              display: 'flex', alignItems: 'center', justifyContent: 'center',
              marginTop: 2, color: C.faint, transition: 'all 0.15s',
            }}
            onMouseEnter={(e) => { e.currentTarget.style.borderColor = C.muted; e.currentTarget.style.color = C.muted; }}
            onMouseLeave={(e) => { e.currentTarget.style.borderColor = C.faint; e.currentTarget.style.color = C.faint; }}
          >
            <Plus size={16} />
          </button>
        </TooltipTrigger>
        <TooltipContent side="right">New team space</TooltipContent>
      </Tooltip>

      {/* Spacer */}
      <div style={{ flex: 1 }} />

      {/* User avatar + menu */}
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button
            style={{
              width: 34, height: 34, borderRadius: '50%',
              background: C.sidebar, border: `1px solid ${C.line}`,
              cursor: 'pointer', display: 'flex', alignItems: 'center', justifyContent: 'center',
              fontSize: 13, fontWeight: 600, color: C.ink2,
            }}
          >
            {userName?.[0]?.toUpperCase() || 'U'}
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent side="right" align="end" className="w-44">
          <div className="px-2 py-1.5 text-xs text-gray-500">{userName}</div>
          <DropdownMenuSeparator />
          <DropdownMenuItem onClick={onSettings}>
            <Settings size={14} className="mr-2" /> Settings
          </DropdownMenuItem>
          <DropdownMenuItem onClick={onDiscover}>
            <Compass size={14} className="mr-2" /> Discover
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem onClick={onLogout}>
            <LogOut size={14} className="mr-2" /> Sign out
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </aside>
  );
}

function SpaceTile({
  workspace,
  active,
  onClick,
  style,
  icon,
  color,
}: {
  workspace: Workspace;
  active: boolean;
  onClick: () => void;
  style: React.CSSProperties;
  icon?: React.ReactNode;
  color: string;
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <div style={{ position: 'relative', marginBottom: 8 }}>
          {/* Active indicator bar */}
          {active && (
            <div style={{
              position: 'absolute', left: -11, top: '50%', transform: 'translateY(-50%)',
              width: 3.5, height: 20, borderRadius: 3, background: color,
            }} />
          )}
          <button
            onClick={onClick}
            style={{
              width: 42, height: 42, borderRadius: 13, border: 'none', cursor: 'pointer',
              display: 'flex', alignItems: 'center', justifyContent: 'center',
              fontSize: 15, fontWeight: 700, transition: 'all 0.15s',
              boxShadow: active ? `0 0 0 2px ${C.rail}, 0 0 0 4px ${color}` : 'none',
              ...style,
            }}
          >
            {icon ?? workspace.name[0]?.toUpperCase()}
          </button>
        </div>
      </TooltipTrigger>
      <TooltipContent side="right">{workspace.name}</TooltipContent>
    </Tooltip>
  );
}

export default SpaceRail;
