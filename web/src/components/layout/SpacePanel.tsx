import { useEffect, useState, useMemo } from 'react';
import {
  ChevronRight, FileText, Folder, Upload, Wand2,
  MoreHorizontal, Plus, FolderPlus, Settings, BookOpen, GitPullRequest, Users, Activity,
  CheckCircle2, Clock, FileCode, Pencil, Trash2, Search,
  PanelLeftClose,
} from 'lucide-react';
import type { Workspace, PageMeta, SourceItem } from '../../api';
import { SearchModal } from '../SearchModal';
import {
  DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { C } from '@/lib/design';

export type NavTab = 'wiki' | 'reviews' | 'members' | 'activity';

/** Supported content directory prefixes */
type ContentDir = 'wiki' | 'entities' | 'concepts';

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
  isOwner: boolean;
  onSelectPage: (slug: string, path?: string) => void;
  onSelectSource: (filename: string) => void;
  onNewPage: (dir?: ContentDir) => void;
  onNewFolder: (dir?: ContentDir) => void;
  onAddPageInFolder: (folderPath: string, dir: ContentDir) => void;
  onAddFolderInFolder: (parentPath: string, dir: ContentDir) => void;
  /** path is the repo path: <dir>/<slug>.md for pages, <dir>/<dir> for folders */
  onRenamePath: (path: string, isFolder: boolean, title: string) => void;
  onDeletePath: (path: string, isFolder: boolean, title: string) => void;
  onShowIngest: () => void;
  onCompile: () => void;
  onSettings?: () => void;
  onCollapse: () => void;
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
  isOwner,
  onSelectPage,
  onSelectSource,
  onNewPage,
  onNewFolder,
  onAddPageInFolder,
  onAddFolderInFolder,
  onRenamePath,
  onDeletePath,
  onShowIngest,
  onCompile,
  onSettings,
  onCollapse,
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
        <div style={{ padding: '12px 10px 12px 16px', color: C.muted, fontSize: 13, display: 'flex', alignItems: 'center' }}>
          <span style={{ flex: 1 }}>Select a space</span>
          <CollapseButton onClick={onCollapse} />
        </div>
      </aside>
    );
  }

  const navItems: { tab: NavTab; icon: React.ReactNode; label: string; badge?: number; hide?: boolean }[] = [
    { tab: 'wiki', icon: <BookOpen size={16} />, label: 'Wiki' },
    { tab: 'reviews', icon: <GitPullRequest size={16} />, label: 'Reviews', badge: reviewCount || undefined, hide: isPersonal },
    { tab: 'members', icon: <Users size={16} />, label: 'Members & roles', hide: isPersonal },
    { tab: 'activity', icon: <Activity size={16} />, label: 'Activity' },
  ];

  const wikiActive = activeTab === 'wiki';

  return (
    <aside style={panelStyle}>
      {/* Space name header */}
      <div style={{
        padding: '0 16px', display: 'flex', alignItems: 'center', gap: 9,
        borderBottom: `1px solid ${C.line}`, height: 52, minHeight: 52,
      }}>
        <div style={{
          width: 26, height: 26, borderRadius: 8,
          background: isPersonal ? C.accent : '#3f6c8c', color: '#fff',
          display: 'flex', alignItems: 'center', justifyContent: 'center',
          fontSize: 12, fontWeight: 700,
        }}>
          {workspace.name[0]?.toUpperCase()}
        </div>
        <span style={{ fontSize: 15.5, fontWeight: 650, color: C.ink, flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
          {workspace.name}
        </span>
        <CollapseButton onClick={onCollapse} />
      </div>

      {/* Nav items */}
      <div style={{ padding: '10px 0 6px' }}>
        {navItems.filter((n) => !n.hide).map((n) => (
          <button
            key={n.tab}
            onClick={() => onTabChange(n.tab)}
            style={{
              display: 'flex', alignItems: 'center', gap: 9, width: 'calc(100% - 16px)',
              padding: '8px 14px', margin: '1px 8px', borderRadius: 8, border: 'none', cursor: 'pointer',
              background: activeTab === n.tab ? '#fbeadd' : 'transparent',
              color: activeTab === n.tab ? C.accent : C.ink2,
              fontSize: 14, fontWeight: activeTab === n.tab ? 600 : 450,
              transition: 'all 0.1s',
              textAlign: 'left',
            }}
            onMouseEnter={(e) => { if (activeTab !== n.tab) e.currentTarget.style.background = C.rail; }}
            onMouseLeave={(e) => { if (activeTab !== n.tab) e.currentTarget.style.background = 'transparent'; }}
          >
            {n.icon}
            <span style={{ flex: 1 }}>{n.label}</span>
            {n.badge != null && n.badge > 0 && (
              <span style={{
                fontSize: 11.5, fontWeight: 700, padding: '0 5px', borderRadius: 999,
                background: C.accent, color: '#fff', minWidth: 18, height: 18,
                display: 'inline-flex', alignItems: 'center', justifyContent: 'center',
              }}>
                {n.badge}
              </span>
            )}
          </button>
        ))}
        {/* Space settings — same nav row style, only for non-personal owner */}
        {!isPersonal && isOwner && onSettings && (
          <button
            onClick={onSettings}
            style={{
              display: 'flex', alignItems: 'center', gap: 9, width: 'calc(100% - 16px)',
              padding: '8px 14px', margin: '1px 8px', borderRadius: 8, border: 'none', cursor: 'pointer',
              background: 'transparent',
              color: C.ink2,
              fontSize: 14, fontWeight: 450,
              transition: 'all 0.1s',
              textAlign: 'left',
            }}
            onMouseEnter={(e) => { e.currentTarget.style.background = C.rail; }}
            onMouseLeave={(e) => { e.currentTarget.style.background = 'transparent'; }}
          >
            <Settings size={16} />
            <span style={{ flex: 1 }}>Space settings</span>
          </button>
        )}
      </div>

      {/* Tree content — always visible regardless of active tab */}
      <div style={{ flex: 1, overflowY: 'auto', padding: '4px 8px' }}>
        {/* Search — opens the palette (⌘K) */}
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
          <span style={{ fontSize: 11.5, color: C.faint, opacity: 0.75, letterSpacing: '0.04em' }}>
            ⌘ K
          </span>
        </button>

        {/* Section 1: Sources */}
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
              onSelectPage={wikiActive ? onSelectPage : undefined}
            />
          )}
          menuItems={
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <button style={{ background: 'none', border: 'none', cursor: 'pointer', padding: 2, color: C.faint, display: 'flex' }}
                  onClick={(e) => e.stopPropagation()}>
                  <MoreHorizontal size={13} />
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="w-36">
                <DropdownMenuItem onClick={onShowIngest}><Upload size={14} className="mr-2" /> Add Source</DropdownMenuItem>
                <DropdownMenuItem onClick={onCompile}><Wand2 size={14} className="mr-2" /> Compile</DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          }
        />

        {/* Section 2: Wiki */}
        <ContentSection
          title="Wiki"
          defaultExpanded
          items={filterAndSortPages(pages, 'wiki')}
          emptyText="No pages yet"
          renderItem={(p) => (
            <PageTreeItem
              key={`${p.kind}:${p.slug}`}
              page={p}
              dir="wiki"
              activePage={wikiActive ? activePage : null}
              depth={0}
              onSelectPage={onSelectPage}
              onAddPageInFolder={(fp) => onAddPageInFolder(fp, 'wiki')}
              onAddFolderInFolder={(pp) => onAddFolderInFolder(pp, 'wiki')}
              onRenamePath={onRenamePath}
              onDeletePath={onDeletePath}
            />
          )}
          menuItems={
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <button style={{
                  background: 'none', border: 'none', cursor: 'pointer', padding: 2, borderRadius: 4,
                  color: C.faint, display: 'flex',
                }} onClick={(e) => e.stopPropagation()}>
                  <Plus size={14} />
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="w-40">
                <DropdownMenuItem onClick={() => onNewPage('wiki')}><FileText size={14} className="mr-2" /> New Page</DropdownMenuItem>
                <DropdownMenuItem onClick={() => onNewFolder('wiki')}><FolderPlus size={14} className="mr-2" /> New Folder</DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          }
        />

        {/* Section 3: Entities */}
        <ContentSection
          title="Entities"
          items={filterAndSortPages(pages, 'entities')}
          emptyText="No pages yet"
          renderItem={(p) => (
            <PageTreeItem
              key={`${p.kind}:${p.slug}`}
              page={p}
              dir="entities"
              activePage={wikiActive ? activePage : null}
              depth={0}
              onSelectPage={onSelectPage}
              onAddPageInFolder={(fp) => onAddPageInFolder(fp, 'entities')}
              onAddFolderInFolder={(pp) => onAddFolderInFolder(pp, 'entities')}
              onRenamePath={onRenamePath}
              onDeletePath={onDeletePath}
            />
          )}
          menuItems={
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <button style={{
                  background: 'none', border: 'none', cursor: 'pointer', padding: 2, borderRadius: 4,
                  color: C.faint, display: 'flex',
                }} onClick={(e) => e.stopPropagation()}>
                  <Plus size={14} />
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="w-40">
                <DropdownMenuItem onClick={() => onNewPage('entities')}><FileText size={14} className="mr-2" /> New Page</DropdownMenuItem>
                <DropdownMenuItem onClick={() => onNewFolder('entities')}><FolderPlus size={14} className="mr-2" /> New Folder</DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          }
        />

        {/* Section 4: Concepts */}
        <ContentSection
          title="Concepts"
          items={filterAndSortPages(pages, 'concepts')}
          emptyText="No pages yet"
          renderItem={(p) => (
            <PageTreeItem
              key={`${p.kind}:${p.slug}`}
              page={p}
              dir="concepts"
              activePage={wikiActive ? activePage : null}
              depth={0}
              onSelectPage={onSelectPage}
              onAddPageInFolder={(fp) => onAddPageInFolder(fp, 'concepts')}
              onAddFolderInFolder={(pp) => onAddFolderInFolder(pp, 'concepts')}
              onRenamePath={onRenamePath}
              onDeletePath={onDeletePath}
            />
          )}
          menuItems={
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <button style={{
                  background: 'none', border: 'none', cursor: 'pointer', padding: 2, borderRadius: 4,
                  color: C.faint, display: 'flex',
                }} onClick={(e) => e.stopPropagation()}>
                  <Plus size={14} />
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="w-40">
                <DropdownMenuItem onClick={() => onNewPage('concepts')}><FileText size={14} className="mr-2" /> New Page</DropdownMenuItem>
                <DropdownMenuItem onClick={() => onNewFolder('concepts')}><FolderPlus size={14} className="mr-2" /> New Folder</DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          }
        />
      </div>

      <SearchModal
        workspaceSlug={workspace.slug}
        workspaceName={workspace.name}
        open={searchOpen}
        onClose={() => setSearchOpen(false)}
        onSelectPage={onSelectPage}
      />

      {/* Space settings moved into nav items above */}
    </aside>
  );
}

/* ── Helpers ── */

/** Filter pages by slug prefix and sort folders-first, then A→Z */
function filterAndSortPages(pages: PageMeta[], prefix: ContentDir): PageMeta[] {
  const dirNode = pages.find((p) => p.slug === prefix && p.kind === 'folder');
  if (!dirNode || !dirNode.children) return [];
  return sortPages(dirNode.children);
}

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
        fontSize: 14, color: C.ink2,
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
  source, active, onSelect, onSelectPage,
}: {
  source: SourceItem;
  active: boolean;
  onSelect: () => void;
  onSelectPage?: (slug: string, path?: string) => void;
}) {
  return (
    <div>
      <button
        onClick={onSelect}
        style={{
          display: 'flex', alignItems: 'center', gap: 8, width: '100%',
          padding: '6px 10px', borderRadius: 6, border: 'none', cursor: 'pointer',
          background: active ? 'rgba(0,0,0,0.05)' : 'transparent',
          color: active ? C.ink : C.ink2,
          fontSize: 14, fontWeight: active ? 550 : 400,
          textAlign: 'left',
        }}
        onMouseEnter={(e) => { if (!active) e.currentTarget.style.background = C.rail; }}
        onMouseLeave={(e) => { if (!active) e.currentTarget.style.background = 'transparent'; }}
      >
        {source.compiled ? <CheckCircle2 size={14} color={C.green} /> : <Clock size={14} color={C.amber} />}
        <FileCode size={14} />
        <span style={{ flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{source.filename}</span>
      </button>
      {source.compiled && source.compiled_pages.length > 0 && onSelectPage && (
        <div style={{ paddingLeft: 24, display: 'flex', flexWrap: 'wrap', gap: 2, paddingBottom: 2 }}>
          {source.compiled_pages.map((slug) => (
            <button
              key={slug}
              onClick={(e) => { e.stopPropagation(); onSelectPage(slug); }}
              style={{
                background: 'none', border: 'none', cursor: 'pointer',
                fontSize: 10, color: C.faint, padding: 0,
              }}
            >
              {slug}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

/* ── Page tree item ── */
function PageTreeItem({
  page,
  dir,
  activePage,
  depth,
  onSelectPage,
  onAddPageInFolder,
  onAddFolderInFolder,
  onRenamePath,
  onDeletePath,
}: {
  page: PageMeta;
  dir: ContentDir;
  activePage: string | null;
  depth: number;
  onSelectPage: (slug: string, path?: string) => void;
  onAddPageInFolder: (folderPath: string) => void;
  onAddFolderInFolder: (parentPath: string) => void;
  onRenamePath: (path: string, isFolder: boolean, title: string) => void;
  onDeletePath: (path: string, isFolder: boolean, title: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const pl = depth * 14;
  const isActive = activePage === page.slug;
  const title = page.title || page.slug;

  // Hook must be at top level — not inside a conditional
  const sortedChildren = useMemo(() => {
    if (page.kind !== 'folder' || !page.children || page.children.length === 0) return [];
    return sortPages(page.children);
  }, [page.kind, page.children]);

  if (page.kind === 'folder') {
    // page.slug is the folder's path relative to the content dir; prepend the dir prefix
    const folderPath = dir + '/' + page.slug;

    return (
      <>
        <div
          style={{
            display: 'flex', alignItems: 'center', gap: 8,
            padding: '6px 10px', paddingLeft: 10 + pl, borderRadius: 6,
            cursor: 'pointer', fontSize: 14, color: C.ink2, position: 'relative',
          }}
          onMouseEnter={(e) => {
            e.currentTarget.style.background = C.rail;
            const btn = e.currentTarget.querySelector('[data-row-menu]') as HTMLElement | null;
            if (btn) btn.style.opacity = '1';
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.background = 'transparent';
            const btn = e.currentTarget.querySelector('[data-row-menu]') as HTMLElement | null;
            if (btn) btn.style.opacity = '0';
          }}
        >
          <span onClick={() => setOpen(!open)} style={{ display: 'flex', alignItems: 'center', gap: 8, flex: 1, userSelect: 'none', minWidth: 0 }}>
            <ChevronRight size={12} style={{ transform: open ? 'rotate(90deg)' : 'none', transition: 'transform 0.15s', flexShrink: 0 }} />
            <Folder size={14} style={{ flexShrink: 0 }} />
            <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{title}</span>
          </span>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button data-row-menu onClick={(e) => e.stopPropagation()} style={{
                background: 'none', border: 'none', cursor: 'pointer', padding: 2, color: C.faint, display: 'flex',
                opacity: 0, transition: 'opacity 0.1s',
              }}>
                <MoreHorizontal size={13} />
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-44">
              <DropdownMenuItem onClick={() => onAddPageInFolder(folderPath)}><FileText size={14} className="mr-2" /> New Page</DropdownMenuItem>
              <DropdownMenuItem onClick={() => onAddFolderInFolder(folderPath)}><FolderPlus size={14} className="mr-2" /> New Folder</DropdownMenuItem>
              <DropdownMenuItem onClick={() => onRenamePath(folderPath, true, title)}><Pencil size={14} className="mr-2" /> Rename</DropdownMenuItem>
              <DropdownMenuItem onClick={() => onDeletePath(folderPath, true, title)} className="text-red-600 focus:text-red-600">
                <Trash2 size={14} className="mr-2" /> Delete folder
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
        {open && sortedChildren.map((child) => (
          <PageTreeItem
            key={`${child.kind}:${child.slug}`}
            page={child}
            dir={dir}
            activePage={activePage}
            depth={depth + 1}
            onSelectPage={onSelectPage}
            onAddPageInFolder={onAddPageInFolder}
            onAddFolderInFolder={onAddFolderInFolder}
            onRenamePath={onRenamePath}
            onDeletePath={onDeletePath}
          />
        ))}
      </>
    );
  }

  const pagePath = `${dir}/${page.slug}.md`;
  return (
    <div
      style={{
        display: 'flex', alignItems: 'center', gap: 8,
        padding: '6px 10px', paddingLeft: 10 + pl, borderRadius: 6,
        cursor: 'pointer', fontSize: 14,
        background: isActive ? 'rgba(0,0,0,0.05)' : 'transparent',
        color: isActive ? C.ink : C.ink2,
        fontWeight: isActive ? 550 : 400,
        boxShadow: isActive ? '0 1px 2px rgba(0,0,0,0.04)' : 'none',
      }}
      onMouseEnter={(e) => {
        if (!isActive) e.currentTarget.style.background = C.rail;
        const btn = e.currentTarget.querySelector('[data-row-menu]') as HTMLElement | null;
        if (btn) btn.style.opacity = '1';
      }}
      onMouseLeave={(e) => {
        if (!isActive) e.currentTarget.style.background = 'transparent';
        const btn = e.currentTarget.querySelector('[data-row-menu]') as HTMLElement | null;
        if (btn) btn.style.opacity = '0';
      }}
    >
      <span onClick={() => onSelectPage(page.slug, page.path)} style={{ display: 'flex', alignItems: 'center', gap: 8, flex: 1, minWidth: 0 }}>
        <FileText size={14} style={{ flexShrink: 0 }} />
        <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{title}</span>
      </span>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button data-row-menu onClick={(e) => e.stopPropagation()} style={{
            background: 'none', border: 'none', cursor: 'pointer', padding: 2, color: C.faint, display: 'flex',
            opacity: 0, transition: 'opacity 0.1s',
          }}>
            <MoreHorizontal size={13} />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-40">
          <DropdownMenuItem onClick={() => onRenamePath(pagePath, false, title)}><Pencil size={14} className="mr-2" /> Rename</DropdownMenuItem>
          <DropdownMenuItem onClick={() => onDeletePath(pagePath, false, title)} className="text-red-600 focus:text-red-600">
            <Trash2 size={14} className="mr-2" /> Delete
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
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

function CollapseButton({ onClick }: { onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-label="Collapse sidebar"
      title="Collapse sidebar"
      style={{
        width: 28, height: 28, padding: 0, border: 'none', borderRadius: 7,
        background: 'transparent', color: C.faint, cursor: 'pointer', flexShrink: 0,
        display: 'inline-flex', alignItems: 'center', justifyContent: 'center',
      }}
      onMouseEnter={(event) => { event.currentTarget.style.background = C.rail; event.currentTarget.style.color = C.ink2; }}
      onMouseLeave={(event) => { event.currentTarget.style.background = 'transparent'; event.currentTarget.style.color = C.faint; }}
    >
      <PanelLeftClose size={16} />
    </button>
  );
}

export default SpacePanel;
