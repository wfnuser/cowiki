import { useState, useEffect, useCallback } from 'react';
import { useNavigate, useLocation } from 'react-router-dom';
import {
  Compass, FileText, Search,
  Upload, Zap, ArrowUpRight, MoreHorizontal, RefreshCw, Bell, Trash2,
  CheckCircle2, Clock,
} from 'lucide-react';
import {
  DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import {
  Dialog, DialogContent, DialogHeader, DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { TooltipProvider } from '@/components/ui/tooltip';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import {
  listWorkspaces, listPages, getPage, createWorkspace, writePage, createFolder,
  compile, submit, renameWorkspace,
  listPendingInvitations, acceptInvitation, rejectInvitation,
  inviteToWorkspace, listMembers, removeMember, changeMemberRole, deleteWorkspace,
  listPublicWorkspaces, joinWorkspace,
  listSources, getSource, listReviews,
  type Workspace, type PageMeta, type PageFull, type PendingInvitation, type MemberInfo, type SourceItem, type SourceContent,
} from '../api';
import { AddSourceDialog } from '@/components/AddSourceDialog';
import { SettingsDialog } from '../components/SettingsDialog';
import { getStoredAuth, clearAuth } from '../auth';
import { SpaceRail } from '../components/layout/SpaceRail';
import { SpacePanel, type NavTab } from '../components/layout/SpacePanel';
import { ReviewList } from '../components/review/ReviewList';
import { ReviewDetail } from '../components/review/ReviewDetail';
import { MembersView } from '../components/views/MembersView';

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

type ActiveView =
  | { kind: 'page'; slug: string; content: PageFull | null }
  | { kind: 'source'; filename: string; content: SourceContent | null }
  | { kind: 'review-list'; workspaceSlug: string }
  | { kind: 'review-detail'; workspaceSlug: string; submissionId: string }
  | { kind: 'members'; workspaceSlug: string }
  | { kind: 'activity'; workspaceSlug: string }
  | null;

export function MainLayout() {
  const [auth, setAuth] = useState(() => getStoredAuth());
  const navigate = useNavigate();
  const location = useLocation();

  // Re-check auth on mount (handles OAuth redirect timing)
  useEffect(() => {
    if (!auth) {
      const stored = getStoredAuth();
      if (stored) setAuth(stored);
    }
  }, []);

  // Data
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [spacePages, setSpacePages] = useState<Record<string, PageMeta[]>>({});
  const [spaceSources, setSpaceSources] = useState<Record<string, SourceItem[]>>({});
  const [activeView, setActiveView] = useState<ActiveView>(null);
  const [activeWorkspace, setActiveWorkspace] = useState<Workspace | null>(null);
  const [reviewRefreshKey, setReviewRefreshKey] = useState(0);
  const [activeTab, setActiveTab] = useState<NavTab>('wiki');
  const [reviewCount, setReviewCount] = useState(0);

  // Modals
  const [showCreate, setShowCreate] = useState(false);
  const [showNewPage, setShowNewPage] = useState<Workspace | null>(null);
  const [showNewFolder, setShowNewFolder] = useState<Workspace | null>(null);
  const [newPageFolder, setNewPageFolder] = useState<string | null>(null);
  const [newFolderParent, setNewFolderParent] = useState<string | null>(null);
  const [newName, setNewName] = useState('');
  const [newSlug, setNewSlug] = useState('');
  const [creating, setCreating] = useState(false);
  const [showIngest, setShowIngest] = useState(false);
  const [compiling, setCompiling] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [showRename, setShowRename] = useState<Workspace | null>(null);
  const [renameValue, setRenameValue] = useState('');
  const [message, setMessage] = useState<{ text: string; type: 'success' | 'error' } | null>(null);
  useEffect(() => {
    if (!message) return;
    const t = setTimeout(() => setMessage(null), 4000);
    return () => clearTimeout(t);
  }, [message]);
  const [searchQuery, setSearchQuery] = useState('');
  const [settingsOpen, setSettingsOpen] = useState(false);

  // Team space management state
  const [pendingInvites, setPendingInvites] = useState<PendingInvitation[]>([]);
  const [showInviteDialog, setShowInviteDialog] = useState<Workspace | null>(null);
  const [inviteEmail, setInviteEmail] = useState('');
  const [inviteRole, setInviteRole] = useState('writer');
  const [showMembersPanel, setShowMembersPanel] = useState<Workspace | null>(null);
  const [membersList, setMembersList] = useState<MemberInfo[]>([]);
  const [showDeleteConfirm, setShowDeleteConfirm] = useState<Workspace | null>(null);
  const [membersLoading, setMembersLoading] = useState(false);
  const [membersError, setMembersError] = useState(false);

  const userBranch = `user/${auth?.id}`;

  // Load workspaces + restore state from URL
  const loadWorkspaces = useCallback(async () => {
    if (!auth) return;
    const ws = await listWorkspaces();
    setWorkspaces(ws);

    // Auto-expand personal space and load its pages
    const personal = ws.find((w) => w.visibility === 'private' && w.role === 'owner');
    if (personal) {
      setActiveWorkspace(personal);
      await loadSpacePages(personal);
      await loadSpaceSources(personal);
    }

    // Restore page from URL: /:wsSlug/:pageSlug
    const pathParts = location.pathname.split('/').filter(Boolean);
    if (pathParts.length >= 2) {
      const [wsSlug, ...pageParts] = pathParts;
      const pageSlug = pageParts.join('/');
      const targetWs = ws.find((w) => w.slug === wsSlug);
      if (targetWs) {
        setActiveWorkspace(targetWs);
        if (!spacePages[targetWs.id]) {
          await loadSpacePages(targetWs);
        }
        if (!spaceSources[targetWs.id]) {
          await loadSpaceSources(targetWs);
        }
        const branch = targetWs.visibility === 'private' ? userBranch : 'main';
        try {
          const page = await getPage(pageSlug, branch, targetWs.slug);
          setActiveView({ kind: 'page', slug: pageSlug, content: page });
        } catch {
          // Page not found, just show home
        }
      }
    } else if (personal) {
      const personalPages = await listPages(userBranch, personal.slug);
      const firstPage = personalPages.find((p) => p.kind === 'page');
      if (firstPage) {
        setActiveView({ kind: 'page', slug: firstPage.slug, content: null });
        navigate(`/${personal.slug}/${firstPage.slug}`, { replace: true });
        try {
          const page = await getPage(firstPage.slug, userBranch, personal.slug);
          setActiveView(prev => prev?.kind === 'page' ? { ...prev, content: page } : prev);
        } catch {
          // Page not found
        }
      }
    }
  }, [auth?.id]);

  useEffect(() => {
    if (auth) loadWorkspaces();
  }, [auth?.id, loadWorkspaces]);

  // Load review count when workspace changes
  useEffect(() => {
    if (!activeWorkspace || isPersonalSpace(activeWorkspace)) {
      setReviewCount(0);
      return;
    }
    let cancelled = false;
    listReviews(activeWorkspace.slug)
      .then((s) => !cancelled && setReviewCount(s.filter((r) => r.status === 'pending').length))
      .catch(() => !cancelled && setReviewCount(0));
    return () => { cancelled = true; };
  }, [activeWorkspace?.slug, reviewRefreshKey]);

  function isPersonalSpace(ws: Workspace): boolean {
    return ws.visibility === 'private' && ws.role === 'owner';
  }

  // Load pages for a space
  const loadSpacePages = async (ws: Workspace) => {
    if (ws.visibility === 'private') {
      const pages = await listPages(userBranch, ws.slug);
      setSpacePages((prev) => ({ ...prev, [ws.id]: pages }));
    } else {
      const [mainPages, draftPages] = await Promise.all([
        listPages('main', ws.slug),
        listPages(userBranch, ws.slug).catch(() => [] as PageMeta[]),
      ]);
      const mainSlugs = new Set(mainPages.map((p) => p.slug));
      const merged = [
        ...mainPages,
        ...draftPages.filter((p) => !mainSlugs.has(p.slug)),
      ];
      setSpacePages((prev) => ({ ...prev, [ws.id]: merged }));
    }
  };

  // Load sources for a space
  const loadSpaceSources = async (ws: Workspace) => {
    try {
      if (ws.visibility === 'private') {
        const [userSources, mainSources] = await Promise.all([
          listSources(ws.slug, userBranch).catch(() => [] as SourceItem[]),
          listSources(ws.slug, 'main').catch(() => [] as SourceItem[]),
        ]);
        const userNames = new Set(userSources.map((s) => s.filename));
        const merged = [
          ...userSources,
          ...mainSources.filter((s) => !userNames.has(s.filename)),
        ];
        setSpaceSources((prev) => ({ ...prev, [ws.id]: merged }));
      } else {
        const [mainSources, draftSources] = await Promise.all([
          listSources(ws.slug, 'main'),
          listSources(ws.slug, userBranch).catch(() => [] as SourceItem[]),
        ]);
        const mainNames = new Set(mainSources.map((s) => s.filename));
        const merged = [
          ...mainSources,
          ...draftSources.filter((s) => !mainNames.has(s.filename)),
        ];
        setSpaceSources((prev) => ({ ...prev, [ws.id]: merged }));
      }
    } catch {
      setSpaceSources((prev) => ({ ...prev, [ws.id]: [] }));
    }
  };

  // Select a source file
  const selectSource = async (ws: Workspace, filename: string) => {
    setActiveWorkspace(ws);
    setActiveTab('wiki');
    setActiveView({ kind: 'source', filename, content: null });
    try {
      let content = await getSource(ws.slug, filename, userBranch).catch(() => null);
      if (!content) {
        content = await getSource(ws.slug, filename, 'main');
      }
      setActiveView(prev => prev?.kind === 'source' ? { ...prev, content } : prev);
    } catch {
      setActiveView(prev => prev?.kind === 'source' ? { ...prev, content: null } : prev);
    }
  };

  // Switch workspace via rail
  const handleSelectWorkspace = (ws: Workspace) => {
    if (activeWorkspace?.id === ws.id) return;
    setActiveWorkspace(ws);
    setActiveTab('wiki');
    // Load pages/sources if not already loaded
    if (!spacePages[ws.id]) loadSpacePages(ws);
    if (!spaceSources[ws.id]) loadSpaceSources(ws);
    // Navigate to first page
    const pages = spacePages[ws.id] || [];
    const first = pages.find((p) => p.kind === 'page');
    if (first) {
      selectPage(ws, first.slug);
    } else {
      setActiveView(null);
    }
  };

  // Select a page
  const selectPage = async (ws: Workspace, slug: string) => {
    setActiveWorkspace(ws);
    setActiveTab('wiki');
    setActiveView({ kind: 'page', slug, content: null });
    navigate(`/${ws.slug}/${slug}`, { replace: true });

    const setContent = (content: PageFull | null) =>
      setActiveView(prev => prev?.kind === 'page' ? { ...prev, content } : prev);

    if (ws.visibility === 'private') {
      try {
        const page = await getPage(slug, userBranch, ws.slug);
        setContent(page);
      } catch {
        setContent(null);
        setMessage({ text: `Page "${slug}" not found`, type: 'error' });
      }
    } else {
      try {
        const page = await getPage(slug, userBranch, ws.slug);
        setContent(page);
      } catch {
        try {
          const page = await getPage(slug, 'main', ws.slug);
          setContent(page);
        } catch {
          setContent(null);
          setMessage({ text: `Page "${slug}" not found`, type: 'error' });
        }
      }
    }
  };

  // Create workspace
  const handleCreateWorkspace = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newName.trim() || !newSlug.trim()) return;
    setCreating(true);
    try {
      await createWorkspace(newName.trim(), newSlug.trim(), 'public');
      setShowCreate(false);
      setNewName('');
      setNewSlug('');
      loadWorkspaces();
    } finally {
      setCreating(false);
    }
  };

  // Create page in a workspace
  const handleCreatePage = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newName.trim() || !showNewPage) return;
    const ws = showNewPage;
    const baseSlug = newName.toLowerCase().replace(/[^a-z0-9\s-]/g, '').replace(/\s+/g, '-').trim();
    const slug = newPageFolder
      ? `${newPageFolder.replace('wiki/', '')}/${baseSlug}`
      : baseSlug;
    const body = `---\ntitle: "${newName.trim()}"\nsummary: ""\nkind: concept\n---\n\n`;
    try {
      await writePage(slug, body, userBranch, ws.slug);
      const title = newName.trim();
      setShowNewPage(null);
      setNewName('');
      setNewPageFolder(null);
      await loadSpacePages(ws);
      setActiveView({ kind: 'page', slug, content: { slug, title, summary: '', body, branch: userBranch, kind: 'page', children: [] } });
      navigate(`/${ws.slug}/${slug}`, { replace: true });
    } catch {
      setMessage({ text: 'Failed to create page', type: 'error' });
    }
  };

  // Create folder in a workspace
  const handleCreateFolder = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newName.trim() || !showNewFolder) return;
    const ws = showNewFolder;
    try {
      await createFolder(newName.trim(), userBranch, newFolderParent || undefined, ws.slug);
      setShowNewFolder(null);
      setNewName('');
      setNewFolderParent(null);
      await loadSpacePages(ws);
    } catch {
      setMessage({ text: 'Failed to create folder', type: 'error' });
    }
  };

  // Compile
  const handleCompile = async () => {
    if (!activeWorkspace) return;
    const ws = activeWorkspace;
    setCompiling(true);
    setMessage(null);
    try {
      const res = await compile(userBranch, ws.slug);
      const count = res.pages?.length || 0;
      const skipped = res.skipped || 0;
      setMessage({ text: `Compiled ${count} page(s)${skipped > 0 ? `, ${skipped} skipped` : ''}`, type: 'success' });
      loadSpacePages(ws);
      loadSpaceSources(ws);
    } catch {
      setMessage({ text: 'Compilation failed', type: 'error' });
    } finally {
      setCompiling(false);
    }
  };

  // Submit pages for review (or direct commit for personal)
  const handleSubmit = async () => {
    if (!activeWorkspace) return;
    const ws = activeWorkspace;
    setSubmitting(true);
    setMessage(null);
    try {
      const pages = spacePages[ws.id] || [];
      if (pages.length === 0) {
        setMessage({ text: 'No pages to submit', type: 'error' });
        return;
      }
      const slugs = pages.map((p) => p.slug);
      const personal = isPersonalSpace(ws);
      await submit(userBranch, slugs, personal, ws.slug);
      setMessage({ text: personal ? 'Committed.' : 'Submitted for review.', type: 'success' });
    } catch {
      setMessage({ text: 'Submit failed', type: 'error' });
    } finally {
      setSubmitting(false);
    }
  };

  // Tab navigation
  const handleTabChange = (tab: NavTab) => {
    if (!activeWorkspace) return;
    setActiveTab(tab);
    const owner = auth?.name || 'user';
    switch (tab) {
      case 'wiki': {
        const pages = spacePages[activeWorkspace.id] || [];
        const first = pages.find((p) => p.kind === 'page');
        if (first) {
          selectPage(activeWorkspace, first.slug);
        } else {
          setActiveView(null);
        }
        break;
      }
      case 'reviews':
        setActiveView({ kind: 'review-list', workspaceSlug: activeWorkspace.slug });
        navigate(`/${owner}/${activeWorkspace.slug}/reviews`);
        break;
      case 'members':
        setActiveView({ kind: 'members', workspaceSlug: activeWorkspace.slug });
        navigate(`/${owner}/${activeWorkspace.slug}/members`);
        break;
      case 'activity':
        setActiveView({ kind: 'activity', workspaceSlug: activeWorkspace.slug });
        navigate(`/${owner}/${activeWorkspace.slug}/activity`);
        break;
    }
  };

  const openReviewDetail = (submissionId: string) => {
    if (!activeWorkspace) return;
    const owner = auth?.name || 'user';
    setActiveView({ kind: 'review-detail', workspaceSlug: activeWorkspace.slug, submissionId });
    navigate(`/${owner}/${activeWorkspace.slug}/reviews/${submissionId}`);
  };

  // Handle ingest completion
  const handleIngestDone = () => {
    setShowIngest(false);
    if (activeWorkspace) {
      loadSpacePages(activeWorkspace);
      loadSpaceSources(activeWorkspace);
    }
  };

  const handleRename = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!renameValue.trim() || !showRename) return;
    try {
      await renameWorkspace(showRename.slug, renameValue.trim());
      setShowRename(null);
      setRenameValue('');
      loadWorkspaces();
    } catch {
      setMessage({ text: 'Failed to rename workspace', type: 'error' });
    }
  };

  // Load pending invitations
  useEffect(() => {
    if (auth) {
      listPendingInvitations().then(setPendingInvites).catch(() => {});
    }
  }, [auth?.id]);

  // Handle invite submission
  const handleInvite = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!inviteEmail.trim() || !showInviteDialog) return;
    try {
      await inviteToWorkspace(showInviteDialog.slug, inviteEmail.trim(), inviteRole);
      setShowInviteDialog(null);
      setInviteEmail('');
      setInviteRole('writer');
      setMessage({ text: 'Invitation sent.', type: 'success' });
    } catch {
      setMessage({ text: 'Failed to send invitation', type: 'error' });
    }
  };

  const handleAcceptInvite = async (inv: PendingInvitation) => {
    try {
      await acceptInvitation(inv.id);
      setPendingInvites((prev) => prev.filter((i) => i.id !== inv.id));
      loadWorkspaces();
      setMessage({ text: `Joined ${inv.workspace_name}!`, type: 'success' });
    } catch {
      setMessage({ text: 'Failed to accept invitation', type: 'error' });
    }
  };

  const handleRejectInvite = async (inv: PendingInvitation) => {
    try {
      await rejectInvitation(inv.id);
      setPendingInvites((prev) => prev.filter((i) => i.id !== inv.id));
    } catch {
      setMessage({ text: 'Failed to reject invitation', type: 'error' });
    }
  };

  // Load and show members panel (for dialog)
  const openMembersPanel = async (ws: Workspace) => {
    setShowMembersPanel(ws);
    setMembersList([]);
    setMembersLoading(true);
    setMembersError(false);
    try {
      const members = await listMembers(ws.slug);
      setMembersList(members);
    } catch {
      setMembersError(true);
    } finally {
      setMembersLoading(false);
    }
  };

  const handleRemoveMember = async (ws: Workspace, userId: string) => {
    try {
      await removeMember(ws.slug, userId);
      setMembersList((prev) => prev.filter((m) => m.id !== userId));
      setMessage({ text: 'Member removed.', type: 'success' });
    } catch {
      setMessage({ text: 'Failed to remove member', type: 'error' });
    }
  };

  const handleChangeRole = async (ws: Workspace, userId: string, role: string) => {
    try {
      await changeMemberRole(ws.slug, userId, role);
      setMembersList((prev) => prev.map((m) => m.id === userId ? { ...m, role } : m));
    } catch {
      setMessage({ text: 'Failed to change role', type: 'error' });
    }
  };

  const handleDeleteWorkspace = async () => {
    if (!showDeleteConfirm) return;
    try {
      await deleteWorkspace(showDeleteConfirm.slug);
      setShowDeleteConfirm(null);
      loadWorkspaces();
      setMessage({ text: 'Workspace deleted.', type: 'success' });
    } catch {
      setMessage({ text: 'Failed to delete workspace', type: 'error' });
    }
  };

  const handleLogout = () => {
    clearAuth();
    navigate('/login');
  };

  const handleNameChange = (name: string) => {
    setNewName(name);
    setNewSlug(name.toLowerCase().replace(/[^a-z0-9\s-]/g, '').replace(/\s+/g, '-').trim());
  };

  // Client-side search
  const searchResults = searchQuery.trim() ? (() => {
    const q = searchQuery.toLowerCase();
    const results: { workspace: Workspace; page: PageMeta }[] = [];
    for (const ws of workspaces) {
      const pages = spacePages[ws.id] || [];
      for (const p of pages) {
        if (p.title.toLowerCase().includes(q) || p.summary.toLowerCase().includes(q) || p.slug.toLowerCase().includes(q)) {
          results.push({ workspace: ws, page: p });
        }
        if (p.children) {
          for (const child of p.children) {
            if (child.title.toLowerCase().includes(q) || child.summary.toLowerCase().includes(q) || child.slug.toLowerCase().includes(q)) {
              results.push({ workspace: ws, page: child });
            }
          }
        }
      }
    }
    return results;
  })() : null;

  // Strip frontmatter from page body
  const renderBody = (body: string) => {
    if (body.startsWith('---')) {
      const parts = body.split('---');
      if (parts.length >= 3) return parts.slice(2).join('---').trim();
    }
    return body;
  };

  const personal = activeWorkspace ? isPersonalSpace(activeWorkspace) : false;
  const isOwner = activeWorkspace?.role === 'owner';

  // Determine active page/source for panel highlight
  const currentActivePage = activeView?.kind === 'page' ? activeView.slug : null;
  const currentActiveSource = activeView?.kind === 'source' ? activeView.filename : null;

  return (
    <>
      <TooltipProvider>
        <div style={{ display: 'flex', height: '100vh', overflow: 'hidden', background: C.bg }}>
          {/* Rail */}
          <SpaceRail
            workspaces={workspaces}
            activeWorkspaceId={activeWorkspace?.id ?? null}
            userName={auth?.name || 'User'}
            onSelectWorkspace={handleSelectWorkspace}
            onCreateWorkspace={() => { setShowCreate(true); setNewName(''); setNewSlug(''); }}
            onSettings={() => setSettingsOpen(true)}
            onDiscover={() => { setActiveView(null); navigate('/discover'); }}
            onLogout={handleLogout}
          />

          {/* Secondary Panel */}
          <SpacePanel
            workspace={activeWorkspace}
            activeTab={activeTab}
            onTabChange={handleTabChange}
            pages={activeWorkspace ? (spacePages[activeWorkspace.id] || []) : []}
            sources={activeWorkspace ? (spaceSources[activeWorkspace.id] || []) : []}
            activePage={currentActivePage}
            activeSource={currentActiveSource}
            reviewCount={reviewCount}
            isPersonal={personal}
            isOwner={isOwner}
            onSelectPage={(slug) => activeWorkspace && selectPage(activeWorkspace, slug)}
            onSelectSource={(filename) => activeWorkspace && selectSource(activeWorkspace, filename)}
            onNewPage={() => { if (activeWorkspace) { setShowNewPage(activeWorkspace); setNewName(''); setNewPageFolder(null); } }}
            onNewFolder={() => { if (activeWorkspace) { setShowNewFolder(activeWorkspace); setNewName(''); setNewFolderParent(null); } }}
            onAddPageInFolder={(folderPath) => { if (activeWorkspace) { setShowNewPage(activeWorkspace); setNewName(''); setNewPageFolder(folderPath); } }}
            onAddFolderInFolder={(parentPath) => { if (activeWorkspace) { setShowNewFolder(activeWorkspace); setNewName(''); setNewFolderParent(parentPath); } }}
            onShowIngest={() => setShowIngest(true)}
            onCompile={handleCompile}
            onSettings={() => activeWorkspace && openMembersPanel(activeWorkspace)}
          />

          {/* Main Content Area */}
          <main style={{ flex: 1, overflow: 'auto', display: 'flex', flexDirection: 'column' }}>
            {/* Top bar: breadcrumb + actions */}
            <div style={{
              position: 'sticky', top: 0, zIndex: 10,
              background: C.panel, borderBottom: `1px solid ${C.line}`,
              padding: '0 24px', display: 'flex', alignItems: 'center', justifyContent: 'space-between',
              height: 52, minHeight: 52,
            }}>
              {/* Left: breadcrumb */}
              <div style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 13, color: C.muted, minWidth: 0, overflow: 'hidden' }}>
                {activeWorkspace && (
                  <span style={{ color: C.ink2 }}>{activeWorkspace.name}</span>
                )}
                {activeView?.kind === 'page' && activeView.content && (
                  <>
                    <span style={{ color: C.faint }}>/</span>
                    <span style={{ color: C.ink, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{activeView.content.title}</span>
                  </>
                )}
                {activeView?.kind === 'source' && (
                  <>
                    <span style={{ color: C.faint }}>/</span>
                    <span style={{ color: C.faint }}>sources</span>
                    <span style={{ color: C.faint }}>/</span>
                    <span style={{ color: C.ink, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{activeView.filename}</span>
                  </>
                )}
                {activeView?.kind === 'review-list' && (
                  <>
                    <span style={{ color: C.faint }}>/</span>
                    <span style={{ color: C.ink }}>Reviews</span>
                  </>
                )}
                {activeView?.kind === 'review-detail' && (
                  <>
                    <span style={{ color: C.faint }}>/</span>
                    <span style={{ color: C.ink }}>Reviews</span>
                    <span style={{ color: C.faint }}>/</span>
                    <span style={{ color: C.ink }}>#{activeView.submissionId.slice(0, 6)}</span>
                  </>
                )}
                {activeView?.kind === 'members' && (
                  <>
                    <span style={{ color: C.faint }}>/</span>
                    <span style={{ color: C.ink }}>Members</span>
                  </>
                )}
                {activeView?.kind === 'activity' && (
                  <>
                    <span style={{ color: C.faint }}>/</span>
                    <span style={{ color: C.ink }}>Activity</span>
                  </>
                )}
              </div>

              {/* Right: actions */}
              <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                {/* Search */}
                <div style={{
                  display: 'flex', alignItems: 'center', gap: 6, padding: '4px 10px',
                  background: C.panel, border: `1px solid ${C.line}`, borderRadius: 6,
                  minWidth: 160,
                }}>
                  <Search size={13} color={C.faint} />
                  <input
                    type="text"
                    value={searchQuery}
                    onChange={(e) => setSearchQuery(e.target.value)}
                    placeholder="Search..."
                    style={{
                      background: 'transparent', border: 'none', outline: 'none', fontSize: 12,
                      color: C.ink, width: 120,
                    }}
                  />
                </div>

                {/* Pending invitations */}
                {pendingInvites.length > 0 && (
                  <DropdownMenu>
                    <DropdownMenuTrigger asChild>
                      <button style={{
                        display: 'flex', alignItems: 'center', gap: 4, padding: '4px 8px',
                        borderRadius: 6, border: 'none', cursor: 'pointer',
                        background: '#fff8c5', color: '#9a6700', fontSize: 11, fontWeight: 500,
                      }}>
                        <Bell size={12} /> {pendingInvites.length}
                      </button>
                    </DropdownMenuTrigger>
                    <DropdownMenuContent align="end" className="w-72">
                      {pendingInvites.map((inv) => (
                        <div key={inv.id} className="px-2 py-2 border-b last:border-0">
                          <div className="text-sm font-medium">{inv.workspace_name}</div>
                          <div className="text-xs text-gray-500">as <span className="font-mono">{inv.role}</span> · invited by {inv.invited_by_name}</div>
                          <div className="flex gap-1 mt-1.5">
                            <button onClick={() => handleAcceptInvite(inv)} className="px-2 py-0.5 text-xs rounded bg-green-600 text-white hover:bg-green-700">Accept</button>
                            <button onClick={() => handleRejectInvite(inv)} className="px-2 py-0.5 text-xs rounded bg-gray-200 text-gray-600 hover:bg-gray-300">Reject</button>
                          </div>
                        </div>
                      ))}
                    </DropdownMenuContent>
                  </DropdownMenu>
                )}

                {/* Wiki-specific actions */}
                {activeTab === 'wiki' && (activeView?.kind === 'page' || activeView?.kind === 'source') && (
                  <>
                    <button
                      onClick={() => setShowIngest(true)}
                      style={headerBtnStyle}
                      title="Add Source"
                    >
                      <Upload size={13} /> Add Source
                    </button>
                    <button
                      onClick={handleCompile}
                      disabled={compiling}
                      style={{ ...headerBtnStyle, opacity: compiling ? 0.4 : 1 }}
                    >
                      {compiling ? <RefreshCw size={13} className="animate-spin" /> : <Zap size={13} color="#e2590b" />}
                      {compiling ? 'Compiling...' : 'Compile'}
                    </button>
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild>
                        <button style={{ ...headerBtnStyle, padding: '4px 6px' }} aria-label="More actions">
                          <MoreHorizontal size={16} />
                        </button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align="end" className="w-40">
                        <DropdownMenuItem onClick={handleSubmit} disabled={submitting}>
                          <ArrowUpRight size={14} className="mr-2" />
                          {submitting ? 'Submitting...' : 'Submit'}
                        </DropdownMenuItem>
                      </DropdownMenuContent>
                    </DropdownMenu>
                  </>
                )}

                {/* User menu moved to Rail bottom avatar */}
              </div>
            </div>

            {/* Message */}
            {message && (
              <div style={{
                margin: '8px 24px 0', padding: '8px 12px', borderRadius: 6, fontSize: 12,
                background: message.type === 'success' ? '#dafbe1' : '#ffebe9',
                color: message.type === 'success' ? '#1a7f37' : '#cf222e',
              }}>
                {message.text}
              </div>
            )}

            {/* Ingest dialog */}
            <AddSourceDialog
              open={showIngest && !!activeWorkspace}
              onOpenChange={(open) => setShowIngest(open)}
              branch={userBranch}
              workspaceName={activeWorkspace?.name || ''}
              workspaceSlug={activeWorkspace?.slug || ''}
              onDone={handleIngestDone}
            />

            {/* Content */}
            <div style={{ flex: 1, padding: '36px 56px 56px' }}>
              {/* Search results */}
              {searchResults ? (
                <div>
                  <h1 className="page-title page-title--compact">
                    Search: "{searchQuery}"
                  </h1>
                  {searchResults.length === 0 ? (
                    <p style={{ color: C.muted, fontSize: 13 }}>No results found.</p>
                  ) : (
                    <div>
                      {searchResults.map((r) => (
                        <button
                          key={`${r.workspace.id}-${r.page.slug}`}
                          onClick={() => { setSearchQuery(''); selectPage(r.workspace, r.page.slug); }}
                          style={{
                            display: 'flex', alignItems: 'center', gap: 10, width: '100%',
                            padding: '8px 12px', borderRadius: 6, border: 'none', cursor: 'pointer',
                            background: 'transparent', textAlign: 'left',
                          }}
                          onMouseEnter={(e) => { e.currentTarget.style.background = C.rail; }}
                          onMouseLeave={(e) => { e.currentTarget.style.background = 'transparent'; }}
                        >
                          <FileText size={16} color={C.faint} />
                          <div>
                            <div style={{ fontSize: 14, color: C.ink }}>{r.page.title || r.page.slug}</div>
                            <div style={{ fontSize: 12, color: C.muted }}>{r.workspace.name}{r.page.summary && ` -- ${r.page.summary}`}</div>
                          </div>
                        </button>
                      ))}
                    </div>
                  )}
                </div>

              /* Review detail */
              ) : activeView?.kind === 'review-detail' ? (
                <ReviewDetail
                  workspaceSlug={activeView.workspaceSlug}
                  submissionId={activeView.submissionId}
                  onBack={() => handleTabChange('reviews')}
                  onActioned={() => setReviewRefreshKey((k) => k + 1)}
                />

              /* Review list */
              ) : activeView?.kind === 'review-list' ? (
                <ReviewList
                  workspaceSlug={activeView.workspaceSlug}
                  onOpen={openReviewDetail}
                  refreshKey={reviewRefreshKey}
                />

              /* Members */
              ) : activeView?.kind === 'members' && activeWorkspace ? (
                <MembersView workspaceSlug={activeWorkspace.slug} />

              /* Activity */
              ) : activeView?.kind === 'activity' ? (
                <div>
                  <h1 className="page-title">
                    Activity
                  </h1>
                  <p style={{ color: C.muted, fontSize: 13 }}>Activity feed coming soon.</p>
                </div>

              /* Source view */
              ) : activeView?.kind === 'source' && activeView.content ? (
                <div>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 16 }}>
                    <span style={{
                      padding: '2px 10px', fontSize: 11, borderRadius: 12,
                      background: '#ddf4ff', color: '#0969da', fontWeight: 500,
                    }}>
                      Source File
                    </span>
                    {activeView.content.compiled ? (
                      <span style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 12, color: C.green }}>
                        <CheckCircle2 size={12} /> Compiled
                      </span>
                    ) : (
                      <span style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 12, color: C.amber }}>
                        <Clock size={12} /> Pending Compile
                      </span>
                    )}
                  </div>
                  {activeView.content.compiled && activeView.content.compiled_pages.length > 0 && (
                    <div style={{
                      marginBottom: 16, padding: 12, borderRadius: 8,
                      background: C.sidebar, border: `1px solid ${C.line}`,
                    }}>
                      <span style={{ fontSize: 12, color: C.muted }}>Compiled to: </span>
                      {activeView.content.compiled_pages.map((slug, i) => (
                        <span key={slug}>
                          {i > 0 && ', '}
                          <button
                            onClick={() => activeWorkspace && selectPage(activeWorkspace, slug)}
                            style={{ fontSize: 12, color: '#0969da', background: 'none', border: 'none', cursor: 'pointer', textDecoration: 'underline' }}
                          >
                            {slug}
                          </button>
                        </span>
                      ))}
                    </div>
                  )}
                  <article>
                    <h1 className="page-title page-title--compact">
                      {activeView.content.filename}
                    </h1>
                    <div className="prose">
                      <ReactMarkdown remarkPlugins={[remarkGfm]}>{activeView.content.content}</ReactMarkdown>
                    </div>
                  </article>
                </div>

              /* Page view */
              ) : activeView?.kind === 'page' && activeView.content ? (
                <article className="prose">
                  <ReactMarkdown remarkPlugins={[remarkGfm]}>{renderBody(activeView.content.body)}</ReactMarkdown>
                </article>

              /* Discover */
              ) : location.pathname === '/discover' ? (
                <DiscoverView />

              /* Home */
              ) : (
                <div>
                  <h1 className="page-title">Home</h1>
                  <p style={{ color: C.muted, fontSize: 13 }}>
                    Select a page from the sidebar to get started.
                  </p>
                </div>
              )}
            </div>
          </main>
        </div>
      </TooltipProvider>

      {/* ── Modals ── */}

      {/* Create team space */}
      <Dialog open={showCreate} onOpenChange={setShowCreate}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader><DialogTitle>Create Team Space</DialogTitle></DialogHeader>
          <form onSubmit={handleCreateWorkspace} className="space-y-4 mt-2">
            <div>
              <label className="text-sm text-[var(--color-text-secondary)] mb-1.5 block">Name</label>
              <Input value={newName} onChange={(e) => handleNameChange(e.target.value)} placeholder="e.g. Engineering Wiki" autoFocus />
            </div>
            <div>
              <label className="text-sm text-[var(--color-text-secondary)] mb-1.5 block">URL slug</label>
              <Input value={newSlug} onChange={(e) => setNewSlug(e.target.value)} placeholder="engineering-wiki" className="font-mono" />
            </div>
            <div className="flex justify-end gap-2 pt-2">
              <Button variant="outline" type="button" onClick={() => setShowCreate(false)}>Cancel</Button>
              <Button type="submit" disabled={creating || !newName.trim() || !newSlug.trim()}>{creating ? 'Creating...' : 'Create'}</Button>
            </div>
          </form>
        </DialogContent>
      </Dialog>

      {/* New page */}
      <Dialog open={!!showNewPage} onOpenChange={(open) => !open && setShowNewPage(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader><DialogTitle>New Page</DialogTitle></DialogHeader>
          <form onSubmit={handleCreatePage} className="space-y-4 mt-2">
            <div>
              <label className="text-sm text-[var(--color-text-secondary)] mb-1.5 block">Title</label>
              <Input value={newName} onChange={(e) => setNewName(e.target.value)} placeholder="e.g. Meeting Notes" autoFocus />
            </div>
            <div className="flex justify-end gap-2 pt-2">
              <Button variant="outline" type="button" onClick={() => setShowNewPage(null)}>Cancel</Button>
              <Button type="submit" disabled={!newName.trim()}>Create</Button>
            </div>
          </form>
        </DialogContent>
      </Dialog>

      {/* Rename workspace */}
      <Dialog open={!!showRename} onOpenChange={(open) => !open && setShowRename(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader><DialogTitle>Rename Space</DialogTitle></DialogHeader>
          <form onSubmit={handleRename} className="space-y-4 mt-2">
            <div>
              <label className="text-sm text-[var(--color-text-secondary)] mb-1.5 block">Name</label>
              <Input value={renameValue} onChange={(e) => setRenameValue(e.target.value)} autoFocus />
            </div>
            <div className="flex justify-end gap-2 pt-2">
              <Button variant="outline" type="button" onClick={() => setShowRename(null)}>Cancel</Button>
              <Button type="submit" disabled={!renameValue.trim()}>Rename</Button>
            </div>
          </form>
        </DialogContent>
      </Dialog>

      {/* New folder */}
      <Dialog open={!!showNewFolder} onOpenChange={(open) => !open && setShowNewFolder(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader><DialogTitle>New Folder</DialogTitle></DialogHeader>
          <form onSubmit={handleCreateFolder} className="space-y-4 mt-2">
            <div>
              <label className="text-sm text-[var(--color-text-secondary)] mb-1.5 block">Name</label>
              <Input value={newName} onChange={(e) => setNewName(e.target.value)} placeholder="e.g. Research, Projects" autoFocus />
            </div>
            <div className="flex justify-end gap-2 pt-2">
              <Button variant="outline" type="button" onClick={() => setShowNewFolder(null)}>Cancel</Button>
              <Button type="submit" disabled={!newName.trim()}>Create</Button>
            </div>
          </form>
        </DialogContent>
      </Dialog>

      {/* Invite member */}
      <Dialog open={!!showInviteDialog} onOpenChange={(open) => !open && setShowInviteDialog(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader><DialogTitle>Invite Member -- {showInviteDialog?.name}</DialogTitle></DialogHeader>
          <form onSubmit={handleInvite} className="space-y-4 mt-2">
            <div>
              <label className="text-sm text-[var(--color-text-secondary)] mb-1.5 block">Email</label>
              <Input value={inviteEmail} onChange={(e) => setInviteEmail(e.target.value)} placeholder="colleague@example.com" autoFocus />
            </div>
            <div>
              <label className="text-sm text-[var(--color-text-secondary)] mb-1.5 block">Role</label>
              <select value={inviteRole} onChange={(e) => setInviteRole(e.target.value)}
                className="w-full rounded-md border border-input bg-background px-3 py-2 text-sm">
                <option value="writer">Writer</option>
                <option value="reader">Reader</option>
                <option value="owner">Owner</option>
              </select>
            </div>
            <div className="flex justify-end gap-2 pt-2">
              <Button variant="outline" type="button" onClick={() => setShowInviteDialog(null)}>Cancel</Button>
              <Button type="submit" disabled={!inviteEmail.trim()}>Send Invitation</Button>
            </div>
          </form>
        </DialogContent>
      </Dialog>

      {/* Members management (dialog) */}
      <Dialog open={!!showMembersPanel} onOpenChange={(open) => !open && setShowMembersPanel(null)}>
        <DialogContent className="sm:max-w-lg">
          <DialogHeader><DialogTitle>Members -- {showMembersPanel?.name}</DialogTitle></DialogHeader>
          <div className="space-y-2 mt-2 max-h-80 overflow-y-auto">
            {membersLoading ? (
              <p className="text-sm text-gray-400">Loading...</p>
            ) : membersError ? (
              <p className="text-sm text-red-500">Failed to load members.</p>
            ) : membersList.length === 0 ? (
              <p className="text-sm text-gray-400">No members found.</p>
            ) : (
              membersList.map((m) => (
                <div key={m.id} className="flex items-center justify-between py-2 border-b last:border-0">
                  <div className="min-w-0">
                    <div className="text-sm font-medium">{m.name}</div>
                    <div className="text-xs text-gray-400">{m.email || 'no email'}</div>
                  </div>
                  <div className="flex items-center gap-2">
                    <select
                      value={m.role}
                      onChange={(e) => showMembersPanel && handleChangeRole(showMembersPanel, m.id, e.target.value)}
                      disabled={m.role === 'owner'}
                      className="text-xs rounded border border-input bg-background px-2 py-1"
                    >
                      <option value="owner">Owner</option>
                      <option value="writer">Writer</option>
                      <option value="reader">Reader</option>
                    </select>
                    {m.role !== 'owner' && (
                      <button
                        onClick={() => showMembersPanel && handleRemoveMember(showMembersPanel, m.id)}
                        className="p-1 rounded text-red-400 hover:text-red-600 hover:bg-red-50 transition-colors"
                        title="Remove member"
                      >
                        <Trash2 size={14} />
                      </button>
                    )}
                  </div>
                </div>
              ))
            )}
          </div>
        </DialogContent>
      </Dialog>

      {/* Delete workspace confirmation */}
      <Dialog open={!!showDeleteConfirm} onOpenChange={(open) => !open && setShowDeleteConfirm(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader><DialogTitle>Delete Workspace</DialogTitle></DialogHeader>
          <div className="space-y-4 mt-2">
            <p className="text-sm text-[var(--color-text-secondary)]">
              Are you sure you want to delete <strong>{showDeleteConfirm?.name}</strong>? This action cannot be undone.
            </p>
            <div className="flex justify-end gap-2">
              <Button variant="outline" onClick={() => setShowDeleteConfirm(null)}>Cancel</Button>
              <Button variant="destructive" onClick={handleDeleteWorkspace}>Delete Workspace</Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>

      {/* Settings dialog */}
      <SettingsDialog open={settingsOpen} onOpenChange={setSettingsOpen} />
    </>
  );
}

const headerBtnStyle: React.CSSProperties = {
  display: 'flex', alignItems: 'center', gap: 6,
  padding: '4px 10px', borderRadius: 6, border: 'none', cursor: 'pointer',
  background: 'transparent', color: C.muted, fontSize: 12,
  transition: 'background 0.1s',
};

// ── Discover View ──
function DiscoverView() {
  const [spaces, setSpaces] = useState<Workspace[]>([]);
  const [loading, setLoading] = useState(true);
  const [joining, setJoining] = useState<string | null>(null);

  useEffect(() => {
    listPublicWorkspaces().then(setSpaces).finally(() => setLoading(false));
  }, []);

  const handleJoin = async (slug: string) => {
    setJoining(slug);
    try {
      await joinWorkspace(slug);
      setSpaces((prev) => prev.filter((w) => w.slug !== slug));
    } finally {
      setJoining(null);
    }
  };

  return (
    <>
      <h1 className="page-title page-title--tight">Discover</h1>
      <p style={{ color: C.muted, fontSize: 13, marginBottom: 32 }}>Public knowledge spaces you can browse and join.</p>
      {loading ? (
        <div style={{ padding: '32px 0', textAlign: 'center', color: C.muted, fontSize: 13 }}>Loading...</div>
      ) : spaces.length === 0 ? (
        <div style={{ padding: '48px 0', textAlign: 'center' }}>
          <Compass size={24} style={{ margin: '0 auto 12px', display: 'block', color: C.faint }} />
          <p style={{ color: C.muted, fontSize: 13 }}>No public spaces yet.</p>
        </div>
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
          {spaces.map((ws) => (
            <div key={ws.id} style={{
              borderRadius: 8, border: `1px solid ${C.line}`, background: C.panel,
              padding: 20, display: 'flex', alignItems: 'center', justifyContent: 'space-between',
            }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
                <div style={{
                  width: 40, height: 40, borderRadius: 8, background: C.rail,
                  display: 'flex', alignItems: 'center', justifyContent: 'center',
                  fontSize: 18, fontWeight: 500, color: C.ink2,
                }}>
                  {ws.name[0]?.toUpperCase()}
                </div>
                <div>
                  <div style={{ fontSize: 14, fontWeight: 500, color: C.ink }}>{ws.name}</div>
                  <div style={{ fontSize: 12, color: C.muted }}>/{ws.slug}</div>
                </div>
              </div>
              <button
                onClick={() => handleJoin(ws.slug)}
                disabled={joining === ws.slug}
                style={{
                  fontSize: 12, color: '#fff', background: C.ink,
                  padding: '6px 14px', borderRadius: 6, border: 'none', cursor: 'pointer',
                  opacity: joining === ws.slug ? 0.4 : 1,
                }}
              >
                {joining === ws.slug ? 'Joining...' : 'Join'}
              </button>
            </div>
          ))}
        </div>
      )}
    </>
  );
}
