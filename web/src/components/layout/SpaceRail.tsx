import { Plus, Settings, Compass, LogOut, Bell, Cloud } from 'lucide-react';
import type { Workspace } from '../../api';
import {
  Tooltip, TooltipContent, TooltipTrigger,
} from '@/components/ui/tooltip';
import {
  DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { C, spaceTileColors } from '@/lib/design';

function tileColor(index: number): string {
  return spaceTileColors[index % spaceTileColors.length];
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
  onConnectCloud: () => void;
  notifUnread: number;
  onShowNotifications: () => void;
  /** Hosted sessions only — local mode has no one to notify you about. */
  showBell: boolean;
  /** Discover and sign-out only make sense after connecting a cloud account. */
  showCloudActions: boolean;
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
  onConnectCloud,
  notifUnread,
  onShowNotifications,
  showBell,
  showCloudActions,
}: SpaceRailProps) {
  return (
    <aside style={{
      width: 68, minWidth: 68, height: '100vh',
      background: C.rail, borderRight: `1px solid ${C.line}`,
      display: 'flex', flexDirection: 'column', alignItems: 'center',
      // macOS traffic lights live in the overlay titlebar. Reserve their row
      // so they never collide with the CoWiki logo or Space buttons.
      padding: '30px 0 12px', boxSizing: 'border-box', gap: 0, position: 'sticky', top: 0,
      zIndex: 20,
    }}>
      {/* Logo — matches panel header height (52px) */}
      <div style={{
        width: 68, height: 46, display: 'flex', alignItems: 'center', justifyContent: 'center',
        cursor: 'default', flexShrink: 0,
      }}>
        <img src="/cowiki-logo.svg" alt="CoWiki" width={30} height={30} />
      </div>

      {/* Separator */}
      <div style={{ width: 26, height: 1, background: C.line, marginBottom: 6 }} />

      {/* A local Space and a cloud-enabled Space are the same product object.
          Use a stable color + initial rather than encoding storage mode here. */}
      {workspaces.map((ws) => {
        const colorIndex = Array.from(ws.id || ws.slug).reduce((sum, char) => sum + char.charCodeAt(0), 0);
        return (
        <SpaceTile
          key={ws.id}
          workspace={ws}
          active={activeWorkspaceId === ws.id}
          onClick={() => onSelectWorkspace(ws)}
          color={tileColor(colorIndex)}
          style={{
            background: tileColor(colorIndex),
            color: '#fff',
          }}
        />
        );
      })}

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
        <TooltipContent side="right">Add a Space</TooltipContent>
      </Tooltip>

      {/* Spacer */}
      <div style={{ flex: 1 }} />

      {/* Notification bell */}
      {showBell && <NotificationBell unread={notifUnread} onOpen={onShowNotifications} />}

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
            {showCloudActions ? (userName?.[0]?.toUpperCase() || 'U') : <Cloud size={17} />}
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent side="right" align="end" className="w-44">
          {showCloudActions ? (
            <>
              <div className="px-2 py-1.5 text-xs text-gray-500">{userName}</div>
              <DropdownMenuSeparator />
            </>
          ) : (
            <>
              <DropdownMenuItem onClick={onConnectCloud} className="items-start gap-2 py-2.5">
                <Cloud size={16} className="mt-0.5 shrink-0" />
                <span>
                  <span className="block font-medium">Sign up / Sign in</span>
                  <span className="block text-xs text-muted-foreground">Connect to CoWiki Cloud</span>
                </span>
              </DropdownMenuItem>
              <DropdownMenuSeparator />
            </>
          )}
          {showCloudActions && (
            <>
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
            </>
          )}
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

// ── Notification Bell ──

function NotificationBell({ unread, onOpen }: { unread: number; onOpen: () => void }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          onClick={onOpen}
          style={{
            width: 34, height: 34, borderRadius: '50%',
            border: `1px solid ${C.line}`, background: 'transparent', cursor: 'pointer',
            display: 'flex', alignItems: 'center', justifyContent: 'center',
            marginBottom: 6, color: C.ink2, position: 'relative',
          }}
        >
          <Bell size={16} />
          {unread > 0 && (
            <span style={{
              position: 'absolute', top: -2, right: -2,
              minWidth: 16, height: 16, borderRadius: 8,
              background: C.accent, color: '#fff', fontSize: 10,
              fontWeight: 600, display: 'flex', alignItems: 'center',
              justifyContent: 'center', padding: '0 4px',
            }}>
              {unread > 99 ? '99+' : unread}
            </span>
          )}
        </button>
      </TooltipTrigger>
      <TooltipContent side="right">Notifications</TooltipContent>
    </Tooltip>
  );
}

export default SpaceRail;
