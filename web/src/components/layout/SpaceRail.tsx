import { Plus, Settings, Compass, LogOut, Bell, Cloud } from 'lucide-react';
import {
  Tooltip, TooltipContent, TooltipTrigger,
} from '@/components/ui/tooltip';
import {
  DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { C, colorForSpaceId } from '@/lib/design';

export interface SpaceRailItem {
  id: string;
  name: string;
  slug: string;
}

interface SpaceRailProps<T extends SpaceRailItem> {
  workspaces: T[];
  activeWorkspaceId: string | null;
  userName: string;
  onSelectWorkspace: (ws: T) => void;
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
  /** Settings belong to the desktop client and are hidden in the focused Cloud shell. */
  showSettings?: boolean;
  showDiscover?: boolean;
  /** Allows browser surfaces to call this action "Join a Space". */
  discoverLabel?: string;
  /** The desktop window reserves room for macOS traffic lights; browsers do not. */
  titlebarInset?: boolean;
  createLabel?: string;
}

export function SpaceRail<T extends SpaceRailItem>({
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
  showSettings = true,
  showDiscover = true,
  discoverLabel = 'Discover',
  titlebarInset = true,
  createLabel = 'Add a Space',
}: SpaceRailProps<T>) {
  return (
    <aside style={{
      width: 68, minWidth: 68, height: '100vh',
      background: C.rail, borderRight: `1px solid ${C.line}`,
      display: 'flex', flexDirection: 'column', alignItems: 'center',
      // macOS traffic lights live in the overlay titlebar. Reserve their row
      // so they never collide with the CoWiki logo or Space buttons.
      padding: `${titlebarInset ? 30 : 8}px 0 12px`, boxSizing: 'border-box', gap: 0, position: 'sticky', top: 0,
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
        const color = colorForSpaceId(ws.id || ws.slug);
        return (
        <SpaceTile
          key={ws.id}
          workspace={ws}
          active={activeWorkspaceId === ws.id}
          onClick={() => onSelectWorkspace(ws)}
          color={color}
          style={{
            background: color,
            color: C.onAccent,
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
        <TooltipContent side="right">{createLabel}</TooltipContent>
      </Tooltip>

      {/* Spacer */}
      <div style={{ flex: 1 }} />

      {/* Notification bell */}
      {showBell && <NotificationBell unread={notifUnread} onOpen={onShowNotifications} />}

      {/* Account menu follows the desktop shell: a compact rounded-square
          trigger, explicit connection status, then the available actions. */}
      <DropdownMenu>
        <Tooltip>
          <TooltipTrigger asChild>
            <DropdownMenuTrigger asChild>
              <button
                style={{
                  width: 34, height: 34, borderRadius: 10,
                  background: showCloudActions ? C.sidebar : C.accentSoft,
                  border: showCloudActions ? `1px solid ${C.line}` : `1px solid color-mix(in srgb, ${C.accent} 25%, transparent)`,
                  cursor: 'pointer', display: 'flex', alignItems: 'center', justifyContent: 'center',
                  fontSize: 13, fontWeight: 600, color: showCloudActions ? C.ink2 : C.accent,
                  transition: 'background 0.15s, border-color 0.15s',
                }}
                onMouseEnter={(e) => {
                  if (showCloudActions) return;
                  e.currentTarget.style.background = C.accentSoftHover;
                  e.currentTarget.style.borderColor = `color-mix(in srgb, ${C.accent} 40%, transparent)`;
                }}
                onMouseLeave={(e) => {
                  if (showCloudActions) return;
                  e.currentTarget.style.background = C.accentSoft;
                  e.currentTarget.style.borderColor = `color-mix(in srgb, ${C.accent} 25%, transparent)`;
                }}
              >
                {showCloudActions ? (userName?.[0]?.toUpperCase() || 'U') : <Cloud size={17} />}
              </button>
            </DropdownMenuTrigger>
          </TooltipTrigger>
          {!showCloudActions && <TooltipContent side="right">Sign up / Sign in</TooltipContent>}
        </Tooltip>
        <DropdownMenuContent side="right" align="end" sideOffset={10} className="w-[210px] rounded-xl p-1.5">
          {showCloudActions ? (
            <>
              <div className="px-2 py-1.5 text-xs text-text-tertiary">{userName}</div>
              <DropdownMenuSeparator />
            </>
          ) : (
            <>
              <div className="flex items-center gap-2 px-2 py-2 text-xs text-muted-foreground">
                <span
                  aria-hidden
                  style={{ width: 8, height: 8, borderRadius: '50%', background: C.amber, boxShadow: `0 0 0 3px ${C.amberSoft}` }}
                />
                Not signed in
              </div>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                onClick={onConnectCloud}
                className="rounded-lg px-2.5 py-2 font-semibold focus:bg-accent-soft"
                style={{ color: C.accent }}
              >
                <Cloud size={16} />
                Sign in
              </DropdownMenuItem>
            </>
          )}
          {/* Settings is not a Cloud feature — local-only Spaces need it too.
              Discover and Sign out stay gated: both are meaningless without
              a connected Cloud account. */}
          {showSettings && (
            <DropdownMenuItem onClick={onSettings}>
              <Settings size={14} className="mr-2" /> Settings
            </DropdownMenuItem>
          )}
          {showCloudActions && (
            <>
              {showDiscover && (
                <DropdownMenuItem onClick={onDiscover}>
                  <Compass size={14} className="mr-2" /> {discoverLabel}
                </DropdownMenuItem>
              )}
              {(showSettings || showDiscover) && <DropdownMenuSeparator />}
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
  workspace: SpaceRailItem;
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
              background: C.accent, color: C.onAccent, fontSize: 10,
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
