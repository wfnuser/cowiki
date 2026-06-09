import { useState } from 'react';
import {
  ChevronRight, FileText, Folder, Upload, Wand2,
  MoreHorizontal, Plus, FolderPlus, Settings, BookOpen, GitPullRequest, Users, Activity,
  CheckCircle2, Clock, FileCode,
} from 'lucide-react';
import type { Workspace, PageMeta, SourceItem } from '../../api';
import {
  DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger,
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
  amber: '#b5790f',
} as const;

export type NavTab = 'wiki' | 'reviews' | 'members' | 'activity';

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
  onSelectPage: (slug: string) => void;
  onSelectSource: (filename: string) => void;
  onNewPage: () => void;
  onNewFolder: () => void;
  onAddPageInFolder: (folderPath: string) => void;
  onAddFolderInFolder: (parentPath: string) => void;
  onShowIngest: () => void;
  onCompile: () => void;
  onSettings?: () => void;
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
  onShowIngest,
  onCompile,
  onSettings,
}: SpacePanelProps) {
  if (!workspace) {
    return (
      <aside style={panelStyle}>
        <div style={{ padding: 16, color: C.muted, fontSize: 13 }}>Select a space</div>
      </aside>
    );
  }

  const navItems: { tab: NavTab; icon: React.ReactNode; label: string; badge?: number; hide?: boolean }[] = [
    { tab: 'wiki', icon: <BookOpen size={16} />, label: 'Wiki' },
    { tab: 'reviews', icon: <GitPullRequest size={16} />, label: 'Reviews', badge: reviewCount || undefined, hide: isPersonal },
    { tab: 'members', icon: <Users size={16} />, label: 'Members & roles', hide: isPersonal },
    { tab: 'activity', icon: <Activity size={16} />, label: 'Activity' },
  ];

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

      {/* Tree content — only show when wiki tab is active */}
      {activeTab === 'wiki' && (
        <div style={{ flex: 1, overflowY: 'auto', padding: '4px 8px' }}>
          {/* Pages header */}
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '0 10px', margin: '16px 0 8px' }}>
            <span style={{ fontSize: 11.5, fontWeight: 600, color: C.faint, textTransform: 'uppercase', letterSpacing: '0.07em' }}>
              Pages
            </span>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <button style={{
                  background: 'none', border: 'none', cursor: 'pointer', padding: 2, borderRadius: 4,
                  color: C.faint, display: 'flex',
                }}>
                  <Plus size={14} />
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="w-40">
                <DropdownMenuItem onClick={onNewPage}><FileText size={14} className="mr-2" /> New Page</DropdownMenuItem>
                <DropdownMenuItem onClick={onNewFolder}><FolderPlus size={14} className="mr-2" /> New Folder</DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>

          {/* Sources section */}
          <SourcesSection
            sources={sources}
            activeSource={activeSource}
            onSelectSource={onSelectSource}
            onSelectPage={onSelectPage}
            onShowIngest={onShowIngest}
            onCompile={onCompile}
          />

          {/* Page tree */}
          {pages.length === 0 ? (
            <div style={{ padding: '8px 8px', fontSize: 12, color: C.faint, fontStyle: 'italic' }}>
              No pages yet
            </div>
          ) : (
            pages.map((p) => (
              <PageTreeItem
                key={p.slug}
                page={p}
                activePage={activePage}
                depth={0}
                onSelectPage={onSelectPage}
                onAddPageInFolder={onAddPageInFolder}
                onAddFolderInFolder={onAddFolderInFolder}
              />
            ))
          )}
        </div>
      )}

      {/* Space settings moved into nav items above */}
    </aside>
  );
}

/* ── Sources section ── */
function SourcesSection({
  sources,
  activeSource,
  onSelectSource,
  onSelectPage,
  onShowIngest,
  onCompile,
}: {
  sources: SourceItem[];
  activeSource: string | null;
  onSelectSource: (filename: string) => void;
  onSelectPage: (slug: string) => void;
  onShowIngest: () => void;
  onCompile: () => void;
}) {
  const [expanded, setExpanded] = useState(false);

  return (
    <>
      <div
        style={{
          display: 'flex', alignItems: 'center', gap: 8, padding: '6px 10px',
          borderRadius: 6, cursor: 'pointer', userSelect: 'none',
          fontSize: 14, color: C.ink2,
        }}
        onMouseEnter={(e) => { e.currentTarget.style.background = C.rail; }}
        onMouseLeave={(e) => { e.currentTarget.style.background = 'transparent'; }}
      >
        <span onClick={() => setExpanded(!expanded)} style={{ display: 'flex', alignItems: 'center', gap: 8, flex: 1 }}>
          <ChevronRight size={12} style={{ transform: expanded ? 'rotate(90deg)' : 'none', transition: 'transform 0.15s' }} />
          <Folder size={14} />
          <span>Sources</span>
        </span>
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
      </div>
      {expanded && (
        <div style={{ paddingLeft: 16 }}>
          {sources.length === 0 ? (
            <div style={{ padding: '4px 8px', fontSize: 12, color: C.faint, fontStyle: 'italic' }}>No sources yet</div>
          ) : (
            sources.map((s) => (
              <div key={s.filename}>
                <button
                  onClick={() => onSelectSource(s.filename)}
                  style={{
                    display: 'flex', alignItems: 'center', gap: 8, width: '100%',
                    padding: '6px 10px', borderRadius: 6, border: 'none', cursor: 'pointer',
                    background: activeSource === s.filename ? 'rgba(0,0,0,0.05)' : 'transparent',
                    color: activeSource === s.filename ? C.ink : C.ink2,
                    fontSize: 14, fontWeight: activeSource === s.filename ? 550 : 400,
                    textAlign: 'left',
                  }}
                  onMouseEnter={(e) => { if (activeSource !== s.filename) e.currentTarget.style.background = C.rail; }}
                  onMouseLeave={(e) => { if (activeSource !== s.filename) e.currentTarget.style.background = 'transparent'; }}
                >
                  {s.compiled ? <CheckCircle2 size={14} color={C.green} /> : <Clock size={14} color={C.amber} />}
                  <FileCode size={14} />
                  <span style={{ flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{s.filename}</span>
                </button>
                {s.compiled && s.compiled_pages.length > 0 && (
                  <div style={{ paddingLeft: 24, display: 'flex', flexWrap: 'wrap', gap: 2, paddingBottom: 2 }}>
                    {s.compiled_pages.map((slug) => (
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
            ))
          )}
        </div>
      )}
    </>
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
}: {
  page: PageMeta;
  activePage: string | null;
  depth: number;
  onSelectPage: (slug: string) => void;
  onAddPageInFolder: (folderPath: string) => void;
  onAddFolderInFolder: (parentPath: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const pl = depth * 14;
  const isActive = activePage === page.slug;

  if (page.kind === 'folder') {
    const folderPath = 'wiki/' + page.slug.replace('/_index', '');
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
            const btn = e.currentTarget.querySelector('[data-folder-add]') as HTMLElement | null;
            if (btn) btn.style.opacity = '1';
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.background = 'transparent';
            const btn = e.currentTarget.querySelector('[data-folder-add]') as HTMLElement | null;
            if (btn) btn.style.opacity = '0';
          }}
        >
          <span onClick={() => setOpen(!open)} style={{ display: 'flex', alignItems: 'center', gap: 8, flex: 1, userSelect: 'none', minWidth: 0 }}>
            <ChevronRight size={12} style={{ transform: open ? 'rotate(90deg)' : 'none', transition: 'transform 0.15s', flexShrink: 0 }} />
            <Folder size={14} style={{ flexShrink: 0 }} />
            <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{page.title || page.slug}</span>
          </span>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button data-folder-add onClick={(e) => e.stopPropagation()} style={{
                background: 'none', border: 'none', cursor: 'pointer', padding: 2, color: C.faint, display: 'flex',
                opacity: 0, transition: 'opacity 0.1s',
              }}>
                <Plus size={13} />
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-40">
              <DropdownMenuItem onClick={() => onAddPageInFolder(folderPath)}><FileText size={14} className="mr-2" /> New Page</DropdownMenuItem>
              <DropdownMenuItem onClick={() => onAddFolderInFolder(folderPath)}><FolderPlus size={14} className="mr-2" /> New Folder</DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
        {open && page.children?.map((child) => (
          <PageTreeItem
            key={child.slug}
            page={child}
            activePage={activePage}
            depth={depth + 1}
            onSelectPage={onSelectPage}
            onAddPageInFolder={onAddPageInFolder}
            onAddFolderInFolder={onAddFolderInFolder}
          />
        ))}
      </>
    );
  }

  return (
    <button
      onClick={() => onSelectPage(page.slug)}
      style={{
        display: 'flex', alignItems: 'center', gap: 8, width: '100%',
        padding: '6px 10px', paddingLeft: 10 + pl, borderRadius: 6,
        border: 'none', cursor: 'pointer', textAlign: 'left',
        background: isActive ? 'rgba(0,0,0,0.05)' : 'transparent',
        color: isActive ? C.ink : C.ink2, fontSize: 14,
        fontWeight: isActive ? 550 : 400,
        boxShadow: isActive ? '0 1px 2px rgba(0,0,0,0.04)' : 'none',
      }}
      onMouseEnter={(e) => { if (!isActive) e.currentTarget.style.background = C.rail; }}
      onMouseLeave={(e) => { if (!isActive) e.currentTarget.style.background = 'transparent'; }}
    >
      <FileText size={14} />
      <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
        {page.title || page.slug}
      </span>
    </button>
  );
}

const panelStyle: React.CSSProperties = {
  width: 236, minWidth: 236, height: '100vh',
  background: C.sidebar, borderRight: `1px solid ${C.line}`,
  display: 'flex', flexDirection: 'column',
  position: 'sticky', top: 0, zIndex: 15,
  overflowY: 'auto',
};

export default SpacePanel;
