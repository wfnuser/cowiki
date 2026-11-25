import { useEffect, useState, useMemo } from 'react';
import {
  ChevronRight, FileText, Folder,
  MoreHorizontal, Plus, FolderPlus, Settings, House, GitPullRequest, Users, History,
  FileCode, Pencil, Trash2, Search, Unlink,
  PanelLeft, CalendarDays, CheckSquare, Layers,
} from 'lucide-react';
import type { Workspace, PageMeta, SourceItem } from '../../api';
import { SearchModal } from '../SearchModal';
import {
  DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { APP_HEADER_HEIGHT, C, colorForSpaceId, shadows } from '@/lib/design';
import { conceptIdFromPath, visiblePageTree } from '@/lib/okf-pages';
import { cn } from '@/lib/utils';
import type { ModuleId } from '@/lib/space-features';
import type { PluginAction } from '@/plugins/host';

/** 'home' is the Space's start page; 'wiki' means a page or Source is open. */
export type NavTab = 'home' | 'journals' | 'wiki' | 'tasks' | 'flashcards' | 'reviews' | 'members' | 'history' | 'links' | `plugin:${string}`;

interface SpacePanelProps {
  workspace: Workspace | null;
  activeTab: NavTab;
  onTabChange: (tab: NavTab) => void;
  pages: PageMeta[];
  sources: SourceItem[];
  activePage: string | null;
  activeSource: string | null;
  reviewCount: number;
  isPersonal: boolean;
  showReviews?: boolean;
  showLinkDiagnostics?: boolean;
  isOwner: boolean;
  onSelectPage: (slug: string, path?: string) => void;
  onSelectSource: (filename: string) => void;
  onNewPage: () => void;
  onNewFolder: () => void;
  onAddPageInFolder: (folderPath: string) => void;
  onAddFolderInFolder: (parentPath: string) => void;
  /** path is the stable bundle-relative path. */
  onRenamePath: (path: string, isFolder: boolean, title: string) => void;
  onDeletePath: (path: string, isFolder: boolean, title: string) => void;
  onShowIngest: () => void;
  /** Open Settings on this Space's group (the gear at the foot of the sidebar). */
  onSettings?: () => void;
  onCollapse: () => void;
  /** Cloud main uses the same panel and tree without local mutation controls. */
  readOnly?: boolean;
  showHistory?: boolean;
  /** Optional modules this person sees in the Space (journal, tasks). */
  modules?: ModuleId[];
  pluginActions?: PluginAction[];
  onPluginAction?: (id: string) => void;
  /** The Journal module is on: Journals (the stream of days) follows Home. */
  journals?: boolean;
}

export function SpacePanel({
  workspace,
  activeTab,
  onTabChange,
  pages,
  sources,
  activePage,
  activeSource,
  reviewCount,
  isPersonal,
  showReviews = !isPersonal,
  showLinkDiagnostics = false,
  onSelectPage,
  onSelectSource,
  onNewPage,
  onNewFolder,
  onAddPageInFolder,
  onAddFolderInFolder,
  onRenamePath,
  onDeletePath,
  onShowIngest,
  onSettings,
  modules = [],
  pluginActions = [],
  onPluginAction,
  onCollapse,
  readOnly = false,
  showHistory = true,
  journals = false,
}: SpacePanelProps) {
  const [searchOpen, setSearchOpen] = useState(false);
  const hasWorkspace = !!workspace;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k' && hasWorkspace) {
        e.preventDefault();
        setSearchOpen((o) => !o);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [hasWorkspace]);

  if (!workspace) {
    return (
      <aside style={panelStyle}>
        <div style={{
          padding: '0 10px 0 16px', color: C.muted, fontSize: 13,
          display: 'flex', alignItems: 'center',
          borderBottom: `1px solid ${C.line}`,
          height: APP_HEADER_HEIGHT, minHeight: APP_HEADER_HEIGHT,
        }}>
          <span style={{ flex: 1 }}>Select a space</span>
          <CollapseButton onClick={onCollapse} />
        </div>
      </aside>
    );
  }

  const navItems: { tab: NavTab; icon: React.ReactNode; label: string; badge?: number; hide?: boolean; action?: string }[] = [
    { tab: 'home', icon: <House size={16} />, label: 'Home' },
    { tab: 'journals', icon: <CalendarDays size={16} />, label: 'Journals', hide: !journals },
    ...pluginActions.map((action) => ({ tab: `plugin:${action.id}` as const, action: action.id,
      icon: action.icon === 'calendar' ? <CalendarDays size={16} /> : <FileText size={16} />, label: action.label })),
    { tab: 'tasks', icon: <CheckSquare size={16} />, label: 'Tasks', hide: !modules.includes('tasks') },
    { tab: 'flashcards', icon: <Layers size={16} />, label: 'Flashcards', hide: !modules.includes('flashcards') },
    { tab: 'reviews', icon: <GitPullRequest size={16} />, label: 'Reviews', badge: reviewCount || undefined, hide: !showReviews },
    { tab: 'members', icon: <Users size={16} />, label: 'Members & roles', hide: isPersonal },
    { tab: 'history', icon: <History size={16} />, label: 'History', hide: !showHistory },
    { tab: 'links', icon: <Unlink size={16} />, label: 'Links', hide: !showLinkDiagnostics },
  ];

  const wikiActive = activeTab === 'wiki';
  const spaceColor = colorForSpaceId(workspace.id || workspace.slug);

  return (
    <aside style={panelStyle}>
      {/* Space name header */}
      {/* A plain title; Settings live behind the gear at the foot. */}
      <div data-tauri-drag-region="deep" style={{
        padding: '0 10px 0 10px', display: 'flex', alignItems: 'center', gap: 4,
        borderBottom: `1px solid ${C.line}`,
        height: APP_HEADER_HEIGHT, minHeight: APP_HEADER_HEIGHT, userSelect: 'none',
      }}>
        <h2 data-space-title className="m-0 flex h-8 min-w-0 flex-1 items-center gap-2 px-1.5" title={workspace.name}>
          <SpaceTile name={workspace.name} color={spaceColor} />
          <span className="min-w-0 truncate text-body font-semibold text-text">{workspace.name}</span>
        </h2>
        <CollapseButton onClick={onCollapse} />
      </div>

      {/* Nav items */}
      {/* The open one is a quiet grey row, like the page tree. */}
      <div style={{ padding: '8px 0 4px' }}>
        {navItems.filter((n) => !n.hide).map((n) => {
          const active = activeTab === n.tab;
          return (
            <button
              key={n.tab}
              aria-current={active ? 'page' : undefined}
              onClick={() => n.action ? onPluginAction?.(n.action) : onTabChange(n.tab)}
              className={cn(
                'mx-2 my-px flex w-[calc(100%-16px)] items-center gap-2.5 rounded-md px-2.5 py-[5px] text-left text-body transition-colors',
                active ? 'bg-bg-active/70 font-medium text-text' : 'text-text-nav hover:bg-bg-hover hover:text-text',
              )}
            >
              <span className={cn('flex shrink-0', active ? 'text-accent' : 'text-text-tertiary')}>{n.icon}</span>
              <span style={{ flex: 1 }}>{n.label}</span>
              {n.badge != null && n.badge > 0 && (
                <span className="inline-flex h-[18px] min-w-[18px] items-center justify-center rounded-full bg-bg-active px-1.5 text-caption font-semibold tabular-nums text-text-secondary">
                  {n.badge}
                </span>
              )}
            </button>
          );
        })}
      </div>

      {/* Tree content — always visible regardless of active tab */}
      <div style={{ flex: 1, overflowY: 'auto', padding: '4px 8px' }}>
        {/* Search — opens the palette (⌘K) */}
        {!readOnly && (
          <button
            onClick={() => setSearchOpen(true)}
            style={{
              display: 'flex', alignItems: 'center', gap: 7, width: 'calc(100% - 4px)',
              margin: '10px 2px 0', padding: '7px 10px',
              background: C.rail, borderRadius: 8, border: `1px solid ${C.lineSoft}`,
              cursor: 'pointer', color: C.faint, fontSize: 13, textAlign: 'left',
            }}
          >
            <Search size={13} style={{ flexShrink: 0 }} />
            <span style={{ flex: 1 }}>Search…</span>
            <span style={{ fontSize: 12, color: C.faint, opacity: 0.75, letterSpacing: '0.04em' }}>
              ⌘ K
            </span>
          </button>
        )}

        {/* Section 1: Sources */}
        {!readOnly && (
          <ContentSection
            title="Sources"
            items={sources}
            emptyText="No sources yet"
            renderItem={(s) => (
              <SourceEntry
                key={s.filename}
                source={s}
                active={activeSource === s.filename}
                onSelect={() => onSelectSource(s.filename)}
              />
            )}
            menuItems={
              <button
                type="button"
                aria-label="Add Source"
                title="Add Source"
                onClick={onShowIngest}
                style={{
                  background: 'none', border: 'none', cursor: 'pointer', padding: 2,
                  borderRadius: 4, color: C.faint, display: 'flex',
                }}
              >
                <Plus size={13} />
              </button>
            }
          />
        )}

        {/* OKF concepts form one arbitrary bundle-relative hierarchy. */}
        <ContentSection
          title="Knowledge"
          defaultExpanded
          items={visiblePageTree(pages)}
          emptyText="No pages yet"
          renderItem={(p) => (
            <PageTreeItem
              key={`${p.kind}:${p.path}`}
              page={p}
              activePage={wikiActive ? activePage : null}
              depth={0}
              onSelectPage={onSelectPage}
              onAddPageInFolder={onAddPageInFolder}
              onAddFolderInFolder={onAddFolderInFolder}
              onRenamePath={onRenamePath}
              onDeletePath={onDeletePath}
              readOnly={readOnly}
            />
          )}
          menuItems={!readOnly ? (
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <button type="button" aria-label="Add page or folder" title="Add page or folder" style={{
                  background: 'none', border: 'none', cursor: 'pointer', padding: 2, borderRadius: 4,
                  color: C.faint, display: 'flex',
                }} onClick={(e) => e.stopPropagation()}>
                  <Plus size={14} />
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="w-40">
                <DropdownMenuItem onClick={onNewPage}><FileText size={14} className="mr-2" /> New Page</DropdownMenuItem>
                <DropdownMenuItem onClick={onNewFolder}><FolderPlus size={14} className="mr-2" /> New Folder</DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          ) : undefined}
        />
      </div>

      {onSettings && !readOnly && (
        <div style={{ padding: '6px 8px 10px', borderTop: `1px solid ${C.lineSoft}` }}>
          <button
            type="button"
            onClick={onSettings}
            className="flex w-full items-center gap-2.5 rounded-md px-2.5 py-[5px] text-left text-body text-text-nav transition-colors hover:bg-bg-hover hover:text-text"
          >
            <span className="flex shrink-0 text-text-tertiary"><Settings size={16} /></span>
            Settings
          </button>
        </div>
      )}

      {!readOnly && (
        <SearchModal
          workspaceSlug={workspace.slug}
          workspaceName={workspace.name}
          open={searchOpen}
          onClose={() => setSearchOpen(false)}
          onSelectPage={onSelectPage}
        />
      )}

    </aside>
  );
}

/* ── Helpers ── */

/** Sort pages: folders first, then A→Z within each group */
function sortPages(pages: PageMeta[]): PageMeta[] {
  return [...pages].sort((a, b) => {
    const aFolder = a.kind === 'folder' ? 0 : 1;
    const bFolder = b.kind === 'folder' ? 0 : 1;
    if (aFolder !== bFolder) return aFolder - bFolder;
    const aName = (a.title || a.slug).toLowerCase();
    const bName = (b.title || b.slug).toLowerCase();
    return aName.localeCompare(bName);
  });
}

/* ── Section header component ── */

interface SectionHeaderProps {
  title: string;
  expanded: boolean;
  onToggle: () => void;
  menuItems?: React.ReactNode;
}

function SectionHeader({ title, expanded, onToggle, menuItems }: SectionHeaderProps) {
  return (
    <div
      style={{
        display: 'flex', alignItems: 'center', gap: 8, padding: '6px 10px',
        borderRadius: 6, cursor: 'pointer', userSelect: 'none',
        fontSize: 14, color: C.nav,
      }}
      onMouseEnter={(e) => { e.currentTarget.style.background = C.rail; }}
      onMouseLeave={(e) => { e.currentTarget.style.background = 'transparent'; }}
    >
      <span onClick={onToggle} style={{ display: 'flex', alignItems: 'center', gap: 8, flex: 1 }}>
        <ChevronRight size={12} style={{ transform: expanded ? 'rotate(90deg)' : 'none', transition: 'transform 0.15s' }} />
        <Folder size={14} />
        <span>{title}</span>
      </span>
      {menuItems}
    </div>
  );
}

/* ── Unified collapsible section ── */

interface ContentSectionProps<T> {
  title: string;
  defaultExpanded?: boolean;
  items: T[];
  emptyText: string;
  renderItem: (item: T) => React.ReactNode;
  menuItems?: React.ReactNode;
}

function ContentSection<T>({
  title, defaultExpanded = false, items, emptyText, renderItem, menuItems,
}: ContentSectionProps<T>) {
  const [expanded, setExpanded] = useState(defaultExpanded);

  return (
    <>
      <SectionHeader
        title={title}
        expanded={expanded}
        onToggle={() => setExpanded(!expanded)}
        menuItems={menuItems}
      />
      {expanded && (
        <div style={{ paddingLeft: 16 }}>
          {items.length === 0 ? (
            <div style={{ padding: '4px 8px', fontSize: 12, color: C.faint, fontStyle: 'italic' }}>{emptyText}</div>
          ) : (
            items.map(renderItem)
          )}
        </div>
      )}
    </>
  );
}

/* ── Source entry (custom view showing compilation status) ── */

function SourceEntry({
  source, active, onSelect,
}: {
  source: SourceItem;
  active: boolean;
  onSelect: () => void;
}) {
  return (
    <div>
      <button
        onClick={onSelect}
        style={{
          display: 'flex', alignItems: 'center', gap: 8, width: '100%',
          padding: '6px 10px', borderRadius: 6, border: 'none', cursor: 'pointer',
          background: active ? `color-mix(in srgb, ${C.ink} 5%, transparent)` : 'transparent',
          color: active ? C.ink : C.nav,
          fontSize: 14, fontWeight: active ? 550 : 400,
          textAlign: 'left',
        }}
        onMouseEnter={(e) => { if (!active) e.currentTarget.style.background = C.rail; }}
        onMouseLeave={(e) => { if (!active) e.currentTarget.style.background = 'transparent'; }}
      >
        <FileCode size={14} />
        <span style={{ flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
          {source.title || source.filename}
        </span>
      </button>
    </div>
  );
}

/* ── Page tree item ── */
function PageTreeItem({
  page,
  activePage,
  depth,
  onSelectPage,
  onAddPageInFolder,
  onAddFolderInFolder,
  onRenamePath,
  onDeletePath,
  readOnly,
}: {
  page: PageMeta;
  activePage: string | null;
  depth: number;
  onSelectPage: (slug: string, path?: string) => void;
  onAddPageInFolder: (folderPath: string) => void;
  onAddFolderInFolder: (parentPath: string) => void;
  onRenamePath: (path: string, isFolder: boolean, title: string) => void;
  onDeletePath: (path: string, isFolder: boolean, title: string) => void;
  readOnly: boolean;
}) {
  const [open, setOpen] = useState(false);
  const pl = depth * 14;
  const isActive = activePage === conceptIdFromPath(page.path);
  const title = page.title || page.slug;

  // Hook must be at top level — not inside a conditional
  const sortedChildren = useMemo(() => {
    if (page.kind !== 'folder' || !page.children || page.children.length === 0) return [];
    return sortPages(page.children);
  }, [page.kind, page.children]);

  if (page.kind === 'folder') {
    const folderPath = page.path;

    return (
      <>
        <div
          className="group hover:bg-bg-hover"
          style={{
            display: 'flex', alignItems: 'center', gap: 8,
            padding: '6px 10px', paddingLeft: 10 + pl, borderRadius: 6,
            cursor: 'pointer', fontSize: 14, color: C.nav, position: 'relative',
          }}
        >
          <span onClick={() => setOpen(!open)} style={{ display: 'flex', alignItems: 'center', gap: 8, flex: 1, userSelect: 'none', minWidth: 0 }}>
            <ChevronRight size={12} style={{ transform: open ? 'rotate(90deg)' : 'none', transition: 'transform 0.15s', flexShrink: 0 }} />
            <Folder size={14} style={{ flexShrink: 0 }} />
            <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{title}</span>
          </span>
          {!readOnly && <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button
                type="button"
                data-row-menu
                aria-label={`Folder options for ${title}`}
                onClick={(e) => e.stopPropagation()}
                className="opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100 focus-visible:opacity-100 data-[state=open]:opacity-100"
                style={{
                  background: 'none', border: 'none', cursor: 'pointer', padding: 2, color: C.faint, display: 'flex',
                }}
              >
                <MoreHorizontal size={13} />
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-44">
              <DropdownMenuItem onClick={() => onAddPageInFolder(folderPath)}><FileText size={14} className="mr-2" /> New Page</DropdownMenuItem>
              <DropdownMenuItem onClick={() => onAddFolderInFolder(folderPath)}><FolderPlus size={14} className="mr-2" /> New Folder</DropdownMenuItem>
              <DropdownMenuItem onClick={() => onRenamePath(folderPath, true, title)}><Pencil size={14} className="mr-2" /> Rename</DropdownMenuItem>
              <DropdownMenuItem onClick={() => onDeletePath(folderPath, true, title)} className="text-red focus:text-red">
                <Trash2 size={14} className="mr-2" /> Delete folder
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>}
        </div>
        {open && sortedChildren.map((child) => (
          <PageTreeItem
            key={`${child.kind}:${child.path}`}
            page={child}
            activePage={activePage}
            depth={depth + 1}
            onSelectPage={onSelectPage}
            onAddPageInFolder={onAddPageInFolder}
            onAddFolderInFolder={onAddFolderInFolder}
            onRenamePath={onRenamePath}
            onDeletePath={onDeletePath}
            readOnly={readOnly}
          />
        ))}
      </>
    );
  }

  const pagePath = page.path;
  const conceptId = conceptIdFromPath(pagePath);
  return (
    <div
      className={cn('group', !isActive && 'hover:bg-bg-hover')}
      style={{
        display: 'flex', alignItems: 'center', gap: 8,
        padding: '6px 10px', paddingLeft: 10 + pl, borderRadius: 6,
        cursor: 'pointer', fontSize: 14,
        background: isActive ? `color-mix(in srgb, ${C.ink} 5%, transparent)` : undefined,
        color: isActive ? C.ink : C.nav,
        fontWeight: isActive ? 550 : 400,
        boxShadow: isActive ? shadows.tick : 'none',
      }}
    >
      <span onClick={() => onSelectPage(conceptId, pagePath)} style={{ display: 'flex', alignItems: 'center', gap: 8, flex: 1, minWidth: 0 }}>
        <FileText size={14} style={{ flexShrink: 0 }} />
        <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{title}</span>
      </span>
      {!readOnly && <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button
            type="button"
            data-row-menu
            aria-label={`Page options for ${title}`}
            onClick={(e) => e.stopPropagation()}
            className="opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100 focus-visible:opacity-100 data-[state=open]:opacity-100"
            style={{
              background: 'none', border: 'none', cursor: 'pointer', padding: 2, color: C.faint, display: 'flex',
            }}
          >
            <MoreHorizontal size={13} />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-40">
          <DropdownMenuItem onClick={() => onRenamePath(pagePath, false, title)}><Pencil size={14} className="mr-2" /> Rename</DropdownMenuItem>
          <DropdownMenuItem onClick={() => onDeletePath(pagePath, false, title)} className="text-red focus:text-red">
            <Trash2 size={14} className="mr-2" /> Delete
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>}
    </div>
  );
}

const panelStyle: React.CSSProperties = {
  width: '100%', minWidth: 0, height: '100vh',
  background: C.sidebar, borderRight: `1px solid ${C.line}`,
  display: 'flex', flexDirection: 'column',
  position: 'sticky', top: 0, zIndex: 15,
  overflowY: 'auto',
};

function SpaceTile({ name, color }: { name: string; color: string }) {
  return (
    <span aria-hidden style={{
      width: 22, height: 22, borderRadius: 6, background: color, color: C.onAccent, flexShrink: 0,
      display: 'inline-flex', alignItems: 'center', justifyContent: 'center', fontSize: 11, fontWeight: 700,
    }}>
      {name[0]?.toUpperCase()}
    </span>
  );
}

function CollapseButton({ onClick }: { onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-label="Collapse sidebar"
      title="Collapse sidebar"
      style={{
        width: 28, height: 28, padding: 0, border: 'none', borderRadius: 6,
        background: 'transparent', color: C.faint, cursor: 'pointer', flexShrink: 0,
        display: 'inline-flex', alignItems: 'center', justifyContent: 'center',
      }}
      onMouseEnter={(event) => { event.currentTarget.style.background = C.rail; event.currentTarget.style.color = C.ink2; }}
      onMouseLeave={(event) => { event.currentTarget.style.background = 'transparent'; event.currentTarget.style.color = C.faint; }}
    >
      <PanelLeft size={16} />
    </button>
  );
}

export default SpacePanel;
