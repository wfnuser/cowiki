import { useState, useEffect, useCallback, useMemo, useRef } from 'react';
import { useNavigate, useLocation } from 'react-router-dom';
import {
  Compass,
  Pencil, FolderOpen, PanelLeft, Bot, HardDrive, FolderInput, Cloud, UserPlus, ChevronLeft,
} from 'lucide-react';
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
  renameWorkspace,
  deleteWorkspace,
  listPublicWorkspaces, joinWorkspace,
  listSources, getSource, listReviews, renamePath, deletePath,
  getLocalWorkingDiff, listLocalAgentChanges,
  type Workspace, type PageMeta, type PageFull, type SourceItem, type SourceContent,
  type AgentChange, type FileDiff,
} from '../api';
import { AddSourceDialog } from '@/components/AddSourceDialog';
import { SettingsDialog } from '../components/SettingsDialog';
import { getCurrentAuth, clearAuth, isRemoteAuth, hasAuth } from '../auth';
import { SpaceRail } from '../components/layout/SpaceRail';
import { SpacePanel, type NavTab } from '../components/layout/SpacePanel';
import {
  ContentBreadcrumb,
  ContentHeader,
  ContentHeaderActions,
} from '../components/layout/ContentHeader';
import { VersionSwitcher, type VersionSelection } from '../components/layout/VersionSwitcher';
type CreateSpaceMode = 'choose' | 'local' | 'import';
import { ReviewList } from '../components/review/ReviewList';
import { ReviewDetail } from '../components/review/ReviewDetail';
import { LocalReviewDetail } from '../components/review/LocalReviewDetail';
import {
  parseReviewRoute,
  reviewRoute,
  type ReviewTarget,
} from '../components/review/review-navigation';
import { MembersView } from '../components/views/MembersView';
import { HistoryView } from '../components/views/HistoryView';
import { LinksView } from '../components/views/LinksView';
import { InviteDialog } from '../components/InviteDialog';
import { PageEditor, type PageEditorHandle } from '../components/PageEditor';
import { PageReader } from '../components/PageReader';
import { TransferDialog } from '../components/TransferDialog';
import { NotificationsPage } from '../components/notifications/NotificationsPage';
import { notificationUnreadCount } from '../api';
import { CommentsProvider, CommentsPanel, CommentsHeaderToggle, commentMarkdownComponents } from '../components/PageCommentsLayer';
import { C } from '@/lib/design';
import { apiOrigin, isDesktopClient } from '@/runtime';
import { normalizeCloudSession } from '@/cloud/session';
import { CloudSpaceDialog } from '@/components/cloud/CloudSpaceDialog';
import { chooseLocalSpaceDirectory, localSpaceIdentityFromPath } from '@/local-space';
import {
  AgentTerminalPanel,
  type AgentPanelOpenRequest,
} from '@/components/terminal/AgentTerminalPanel';
import {
  conceptIdFromPath,
  conceptPath,
  firstConcept,
  isReservedDocument,
  pageRoute,
  visiblePageTree,
} from '@/lib/okf-pages';
import {
  clampSidebarWidth,
  loadSidebarLayout,
  saveSidebarLayout,
} from '@/lib/sidebar-layout';
import {
  loadClientSettings,
  saveClientSettings,
  type DefaultAgent,
} from '@/lib/client-settings';
import { splitSystemFrontmatter } from '@/lib/page-frontmatter';
import { sourceOrganizationTask } from '@/lib/source-ingest';
import { resolveWorkspaceSwitchTarget } from '@/lib/workspace-navigation';
import { getCloudStatus } from '@/local-api';
import { createCloudClient } from '@/cloud/client';
import { cloudPageCommentStore, localPageCommentStore } from '@/lib/page-comment-store';

type ActiveView =
  | { kind: 'page'; slug: string; path?: string; content: PageFull | null }
  | { kind: 'source'; filename: string; content: SourceContent | null }
  | { kind: 'review-list'; workspaceSlug: string }
  | { kind: 'review-detail'; workspaceSlug: string; target: ReviewTarget }
  | { kind: 'members'; workspaceSlug: string }
  | { kind: 'history'; workspaceSlug: string }
  | { kind: 'links'; workspaceSlug: string }
  | { kind: 'notifications' }
  | null;

function isPersonalSpace(ws: Workspace): boolean {
  return ws.visibility === 'private' && ws.role === 'owner';
}

/** Merge main and draft recursively by stable bundle-relative path. */
function mergePageTrees(main: PageMeta[], draft: PageMeta[]): PageMeta[] {
  const merged: PageMeta[] = [...main];
  const pathToIndex = new Map(merged.map((p, i) => [p.path, i] as const));
  for (const draftPage of draft) {
    const index = pathToIndex.get(draftPage.path);
    if (index === undefined) {
      pathToIndex.set(draftPage.path, merged.length);
      merged.push(draftPage);
    } else if (merged[index].kind === 'folder' && draftPage.kind === 'folder') {
      merged[index] = {
        ...merged[index],
        children: mergePageTrees(merged[index].children || [], draftPage.children || []),
      };
    }
  }
  return merged;
}

export function MainLayout() {
  const [auth, setAuth] = useState(() => getCurrentAuth());
  const desktop = isDesktopClient();
  const navigate = useNavigate();
  const location = useLocation();

  // Data
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [workspacesLoaded, setWorkspacesLoaded] = useState(false);
  const [spacePages, setSpacePages] = useState<Record<string, PageMeta[]>>({});
  const [spaceSources, setSpaceSources] = useState<Record<string, SourceItem[]>>({});
  const [activeView, setActiveView] = useState<ActiveView>(null);
  const [activeWorkspace, setActiveWorkspace] = useState<Workspace | null>(null);
  const [notifUnread, setNotifUnread] = useState(0);

  // Cross-space unread badge for the rail; refreshed when opening the inbox.
  useEffect(() => {
    if (isRemoteAuth(auth)) notificationUnreadCount().then(setNotifUnread).catch(() => {});
  }, [auth]);
  const articleRef = useRef<HTMLElement>(null);
  const pageEditorRef = useRef<PageEditorHandle>(null);
  const persistedPageBody = useRef<string | null>(null);
  const externalConflictNotified = useRef(false);
  const [reviewRefreshKey, setReviewRefreshKey] = useState(0);
  const [activeTab, setActiveTab] = useState<NavTab>('wiki');
  const [reviewCount, setReviewCount] = useState(0);
  const [versionSelection, setVersionSelection] = useState<VersionSelection>({ kind: 'working' });
  const [openAgentChanges, setOpenAgentChanges] = useState<AgentChange[]>([]);
  const [workingDiffs, setWorkingDiffs] = useState<FileDiff[]>([]);
  const [sidebarLayout, setSidebarLayout] = useState(() => loadSidebarLayout(window.localStorage));
  const [resizingSidebar, setResizingSidebar] = useState(false);
  const sidebarResizeStart = useRef({ pointerX: 0, width: sidebarLayout.width });
  const [agentPanelOpen, setAgentPanelOpen] = useState(false);
  const [agentOpenRequest, setAgentOpenRequest] = useState<AgentPanelOpenRequest | null>(null);
  const [clientSettings, setClientSettings] = useState(() => (
    loadClientSettings(window.localStorage)
  ));
  const [agentPanelWidth, setAgentPanelWidth] = useState(() => {
    const saved = Number(window.localStorage.getItem('cowiki.agentPanelWidth'));
    return Number.isFinite(saved) ? Math.max(320, Math.min(720, saved)) : 440;
  });
  const [resizingAgentPanel, setResizingAgentPanel] = useState(false);
  const agentResizeStart = useRef({ pointerX: 0, width: agentPanelWidth });

  useEffect(() => {
    try {
      saveSidebarLayout(window.localStorage, sidebarLayout);
    } catch {
      // A disabled storage API should not prevent the local workspace from opening.
    }
  }, [sidebarLayout]);

  useEffect(() => {
    if (!resizingSidebar) return;

    const previousCursor = document.body.style.cursor;
    const previousUserSelect = document.body.style.userSelect;
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';

    const onPointerMove = (event: PointerEvent) => {
      const delta = event.clientX - sidebarResizeStart.current.pointerX;
      setSidebarLayout((current) => ({
        ...current,
        width: clampSidebarWidth(sidebarResizeStart.current.width + delta),
      }));
    };
    const stopResize = () => setResizingSidebar(false);

    window.addEventListener('pointermove', onPointerMove);
    window.addEventListener('pointerup', stopResize, { once: true });
    window.addEventListener('pointercancel', stopResize, { once: true });
    return () => {
      window.removeEventListener('pointermove', onPointerMove);
      window.removeEventListener('pointerup', stopResize);
      window.removeEventListener('pointercancel', stopResize);
      document.body.style.cursor = previousCursor;
      document.body.style.userSelect = previousUserSelect;
    };
  }, [resizingSidebar]);

  useEffect(() => {
    window.localStorage.setItem('cowiki.agentPanelWidth', String(agentPanelWidth));
  }, [agentPanelWidth]);

  const handleDefaultAgentChange = useCallback((defaultAgent: DefaultAgent) => {
    const next = { defaultAgent };
    setClientSettings(next);
    saveClientSettings(window.localStorage, next);
  }, []);

  useEffect(() => {
    if (!resizingAgentPanel) return;
    const previousCursor = document.body.style.cursor;
    const previousUserSelect = document.body.style.userSelect;
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
    const onPointerMove = (event: PointerEvent) => {
      const delta = agentResizeStart.current.pointerX - event.clientX;
      setAgentPanelWidth(Math.max(320, Math.min(720, agentResizeStart.current.width + delta)));
    };
    const stopResize = () => setResizingAgentPanel(false);
    window.addEventListener('pointermove', onPointerMove);
    window.addEventListener('pointerup', stopResize, { once: true });
    window.addEventListener('pointercancel', stopResize, { once: true });
    return () => {
      window.removeEventListener('pointermove', onPointerMove);
      window.removeEventListener('pointerup', stopResize);
      window.removeEventListener('pointercancel', stopResize);
      document.body.style.cursor = previousCursor;
      document.body.style.userSelect = previousUserSelect;
    };
  }, [resizingAgentPanel]);

  // Modals
  const [showCreate, setShowCreate] = useState(false);
  const [createSpaceMode, setCreateSpaceMode] = useState<CreateSpaceMode>('choose');
  const [showNewPage, setShowNewPage] = useState<Workspace | null>(null);
  const [showNewFolder, setShowNewFolder] = useState<Workspace | null>(null);
  const [newPageFolder, setNewPageFolder] = useState<string | null>(null);
  const [newFolderParent, setNewFolderParent] = useState<string | null>(null);
  const [newName, setNewName] = useState('');
  const [newSlug, setNewSlug] = useState('');
  const [newLocalPath, setNewLocalPath] = useState('');
  const [creating, setCreating] = useState(false);
  const [showIngest, setShowIngest] = useState(false);
  const [showRename, setShowRename] = useState<Workspace | null>(null);
  const [renameValue, setRenameValue] = useState('');
  const [message, setMessage] = useState<{ text: string; type: 'success' | 'error' } | null>(null);
  useEffect(() => {
    if (!message) return;
    const t = setTimeout(() => setMessage(null), 4000);
    return () => clearTimeout(t);
  }, [message]);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [cloudDialogOpen, setCloudDialogOpen] = useState(false);
  const [editingPage, setEditingPage] = useState(false);
  // Tree path ops (rename/delete of pages & folders on the draft branch)
  const [pathOp, setPathOp] = useState<
    | { kind: 'rename'; path: string; isFolder: boolean; title: string; value: string }
    | { kind: 'delete'; path: string; isFolder: boolean; title: string }
    | null
  >(null);

  // Team space management state
  const [showInviteDialog, setShowInviteDialog] = useState<Workspace | null>(null);
  const [showTransferDialog, setShowTransferDialog] = useState<Workspace | null>(null);
  const [showDeleteConfirm, setShowDeleteConfirm] = useState<Workspace | null>(null);

  const userBranch = `user/${auth?.id}`;
  const cloudSession = useMemo(() => {
    if (!isRemoteAuth(auth) || !auth?.api_key) return null;
    try {
      return normalizeCloudSession({
        baseUrl: apiOrigin(),
        apiKey: auth.api_key,
        userId: auth.id,
        userName: auth.name,
      });
    } catch {
      return null;
    }
  }, [auth]);
  const cloudClient = useMemo(
    () => cloudSession ? createCloudClient(cloudSession) : null,
    [cloudSession],
  );
  const [commentCloudSpace, setCommentCloudSpace] = useState<{ slug: string; id: string } | null>(null);

  useEffect(() => {
    if (!desktop || !activeWorkspace || !cloudSession) {
      setCommentCloudSpace(null);
      return;
    }
    let active = true;
    void getCloudStatus(activeWorkspace.slug)
      .then((status) => {
        if (!active) return;
        setCommentCloudSpace(status.cloudSpaceId
          ? { slug: activeWorkspace.slug, id: status.cloudSpaceId }
          : null);
      })
      .catch(() => { if (active) setCommentCloudSpace(null); });
    return () => { active = false; };
  }, [activeWorkspace, cloudSession, desktop]);

  // Load pages for a space.
  const loadSpacePages = useCallback(async (ws: Workspace) => {
    if (ws.visibility === 'private') {
      const pages = visiblePageTree(await listPages(userBranch, ws.slug, 'all'));
      setSpacePages((previous) => ({ ...previous, [ws.id]: pages }));
      return pages;
    }
    const [mainPages, draftPages] = await Promise.all([
      listPages('main', ws.slug, 'all'),
      listPages(userBranch, ws.slug, 'all').catch(() => [] as PageMeta[]),
    ]);
    const merged = visiblePageTree(mergePageTrees(mainPages, draftPages));
    setSpacePages((previous) => ({ ...previous, [ws.id]: merged }));
    return merged;
  }, [userBranch]);

  // Load sources for a space.
  const loadSpaceSources = useCallback(async (ws: Workspace) => {
    try {
      if (ws.visibility === 'private') {
        const [userSources, mainSources] = await Promise.all([
          listSources(ws.slug, userBranch).catch(() => [] as SourceItem[]),
          listSources(ws.slug, 'main').catch(() => [] as SourceItem[]),
        ]);
        const userNames = new Set(userSources.map((source) => source.filename));
        const merged = [
          ...userSources,
          ...mainSources.filter((source) => !userNames.has(source.filename)),
        ];
        setSpaceSources((previous) => ({ ...previous, [ws.id]: merged }));
        return;
      }
      const [mainSources, draftSources] = await Promise.all([
        listSources(ws.slug, 'main'),
        listSources(ws.slug, userBranch).catch(() => [] as SourceItem[]),
      ]);
      const mainNames = new Set(mainSources.map((source) => source.filename));
      const merged = [
        ...mainSources,
        ...draftSources.filter((source) => !mainNames.has(source.filename)),
      ];
      setSpaceSources((previous) => ({ ...previous, [ws.id]: merged }));
    } catch {
      setSpaceSources((previous) => ({ ...previous, [ws.id]: [] }));
    }
  }, [userBranch]);

  // Load workspaces + restore state from URL
  const loadWorkspaces = useCallback(async () => {
    if (!auth) return;
    // Local sessions are full accounts on the local backend — only a
    // credential-less placeholder (backend unreachable) shows the empty state.
    if (!desktop && !hasAuth(auth)) {
      setWorkspaces([]);
      setActiveWorkspace(null);
      setActiveView(null);
      setWorkspacesLoaded(true);
      return;
    }
    const ws = await listWorkspaces();
    setWorkspaces(ws);
    setWorkspacesLoaded(true);

    const reviewLocation = parseReviewRoute(location.pathname, ws.map((workspace) => workspace.slug));
    if (reviewLocation) {
      const targetWs = ws.find((workspace) => workspace.slug === reviewLocation.workspaceSlug);
      if (targetWs) {
        setActiveWorkspace(targetWs);
        setActiveTab('reviews');
        setActiveView(reviewLocation.target
          ? { kind: 'review-detail', workspaceSlug: targetWs.slug, target: reviewLocation.target }
          : { kind: 'review-list', workspaceSlug: targetWs.slug });
        await loadSpacePages(targetWs);
        await loadSpaceSources(targetWs);
        return;
      }
    }

    // Auto-expand personal space and load its pages
    const personal = ws.find((w) => w.visibility === 'private' && w.role === 'owner');
    if (personal) {
      setActiveWorkspace(personal);
      await loadSpacePages(personal);
      await loadSpaceSources(personal);
    }

    // Restore a Concept from its stable bundle-relative ID:
    // /:workspaceSlug/:folder/:concept (the .md suffix is not exposed).
    const pathParts = location.pathname.split('/').filter(Boolean);
    if (pathParts.length >= 2) {
      const [wsSlug, ...pageParts] = pathParts;
      const conceptId = pageParts.map((part) => decodeURIComponent(part)).join('/');
      const targetWs = ws.find((w) => w.slug === decodeURIComponent(wsSlug));
      if (targetWs && !isReservedDocument(conceptPath(conceptId))) {
        setActiveWorkspace(targetWs);
        await loadSpacePages(targetWs);
        await loadSpaceSources(targetWs);
        const branch = targetWs.visibility === 'private' ? userBranch : 'main';
        try {
          const page = await getPage(conceptId, branch, targetWs.slug, 'wiki');
          persistedPageBody.current = page.body;
          setActiveView({ kind: 'page', slug: conceptId, path: conceptPath(conceptId), content: page });
        } catch {
          // Page not found, just show home
        }
      }
    } else if (personal) {
      const personalPages = visiblePageTree(await listPages(userBranch, personal.slug, 'all'));
      const firstPage = firstConcept(personalPages);
      if (firstPage) {
        const conceptId = conceptIdFromPath(firstPage.path);
        setActiveView({ kind: 'page', slug: conceptId, path: firstPage.path, content: null });
        navigate(pageRoute(personal.slug, conceptId), { replace: true });
        try {
          const page = await getPage(conceptId, userBranch, personal.slug, 'wiki');
          setActiveView(prev => prev?.kind === 'page' ? { ...prev, content: page } : prev);
        } catch {
          // Page not found
        }
      }
    }
  }, [auth, desktop, loadSpacePages, loadSpaceSources, location.pathname, navigate, userBranch]);

  useEffect(() => {
    if (!auth) return;
    const task = window.setTimeout(() => { void loadWorkspaces(); }, 0);
    return () => window.clearTimeout(task);
  }, [auth, loadWorkspaces]);

  // Load review count when workspace changes
  useEffect(() => {
    if (!activeWorkspace || (!desktop && isPersonalSpace(activeWorkspace))) {
      const task = window.setTimeout(() => setReviewCount(0), 0);
      return () => window.clearTimeout(task);
      return;
    }
    let cancelled = false;
    listReviews(activeWorkspace.slug)
      .then((s) => !cancelled && setReviewCount(s.filter((r) => r.status === 'pending').length))
      .catch(() => !cancelled && setReviewCount(0));
    return () => { cancelled = true; };
  }, [activeWorkspace, desktop, reviewRefreshKey]);

  // The desktop version switcher is a local Git view: Working, HEAD
  // (Upstream), and currently-open isolated Agent Changes only.
  useEffect(() => {
    if (!desktop || !activeWorkspace?.localPath) {
      const task = window.setTimeout(() => {
        setOpenAgentChanges([]);
        setWorkingDiffs([]);
        setVersionSelection({ kind: 'working' });
      }, 0);
      return () => window.clearTimeout(task);
    }
    let cancelled = false;
    Promise.all([
      listLocalAgentChanges(activeWorkspace.slug),
      getLocalWorkingDiff(activeWorkspace.slug),
    ]).then(([changes, diffs]) => {
      if (cancelled) return;
      const openChanges = changes.filter((change) => change.status === 'open');
      setOpenAgentChanges(openChanges);
      setWorkingDiffs(diffs);
      setVersionSelection((current) => (
        current.kind === 'agent' && !openChanges.some((change) => change.id === current.changeId)
          ? { kind: 'working' }
          : current
      ));
    }).catch(() => {
      if (cancelled) return;
      setOpenAgentChanges([]);
      setWorkingDiffs([]);
      setVersionSelection({ kind: 'working' });
    });
    return () => { cancelled = true; };
  }, [activeWorkspace?.localPath, activeWorkspace?.slug, desktop, reviewRefreshKey]);

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
  const handleSelectWorkspace = async (ws: Workspace) => {
    if (activeWorkspace?.id === ws.id) return;
    setActiveWorkspace(ws);
    setActiveTab('wiki');
    // Resolve navigation from the loaded tree, not the pre-request state snapshot.
    const targetPromise = resolveWorkspaceSwitchTarget(
      spacePages[ws.id],
      () => loadSpacePages(ws),
    );
    if (!spaceSources[ws.id]) loadSpaceSources(ws);
    const target = await targetPromise;
    if (target) {
      selectPage(ws, target.conceptId, target.path);
    } else {
      setActiveView(null);
    }
  };

  // Select a page
  const selectPage = async (ws: Workspace, slug: string, path?: string) => {
    const conceptId = path ? conceptIdFromPath(path) : conceptIdFromPath(slug);
    const repoPath = path ? conceptPath(conceptIdFromPath(path)) : conceptPath(conceptId);
    if (isReservedDocument(repoPath)) return;
    setActiveWorkspace(ws);
    setActiveTab('wiki');
    setEditingPage(false);
    persistedPageBody.current = null;
    externalConflictNotified.current = false;
    setActiveView({ kind: 'page', slug: conceptId, path: repoPath, content: null });
    navigate(pageRoute(ws.slug, conceptId), { replace: true });
    const setContent = (content: PageFull | null) => {
      persistedPageBody.current = content?.body ?? null;
      setActiveView(prev => prev?.kind === 'page' ? { ...prev, content } : prev);
    };

    if (ws.visibility === 'private') {
      try {
        const page = await getPage(conceptId, userBranch, ws.slug, 'wiki');
        setContent(page);
      } catch {
        setContent(null);
        setMessage({ text: `Page "${conceptId}" not found`, type: 'error' });
      }
    } else {
      try {
        const page = await getPage(conceptId, userBranch, ws.slug, 'wiki');
        setContent(page);
      } catch {
        try {
          const page = await getPage(conceptId, 'main', ws.slug, 'wiki');
          setContent(page);
        } catch {
          setContent(null);
          setMessage({ text: `Page "${conceptId}" not found`, type: 'error' });
        }
      }
    }
  };

  // Create workspace
  const handleChooseLocalFolder = useCallback(async () => {
    try {
      const localPath = await chooseLocalSpaceDirectory();
      if (!localPath) return;
      const { name, slug } = localSpaceIdentityFromPath(localPath);
      setNewLocalPath(localPath);
      if (createSpaceMode === 'import' && !newName.trim()) {
        setNewName(name);
        setNewSlug(slug);
      }
    } catch (error) {
      setMessage({
        text: error instanceof Error ? error.message : 'Could not choose this folder',
        type: 'error',
      });
    }
  }, [createSpaceMode, newName]);

  useEffect(() => {
    if (!desktop || !workspacesLoaded || workspaces.length > 0) return;
    const openOnShortcut = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'o') {
        event.preventDefault();
        if (!creating) setShowCreate(true);
      }
    };
    window.addEventListener('keydown', openOnShortcut);
    return () => window.removeEventListener('keydown', openOnShortcut);
  }, [creating, desktop, workspaces.length, workspacesLoaded]);

  const handleCreateWorkspace = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!desktop && !hasAuth(auth)) {
      navigate('/login');
      return;
    }
    if (!newName.trim() || (!desktop && !newSlug.trim()) || (desktop && !newLocalPath)) return;
    setCreating(true);
    try {
      const internalSlug = desktop
        ? `${newSlug || 'space'}-${crypto.randomUUID().slice(0, 8)}`
        : newSlug.trim();
      await createWorkspace(
        newName.trim(),
        internalSlug,
        desktop ? 'private' : 'public',
        desktop ? newLocalPath : undefined,
        desktop && createSpaceMode === 'local',
      );
      setShowCreate(false);
      setNewName('');
      setNewSlug('');
      setNewLocalPath('');
      setCreateSpaceMode('choose');
      await loadWorkspaces();
    } catch (error) {
      setMessage({
        text: error instanceof Error ? error.message : 'Could not create this Space',
        type: 'error',
      });
    } finally {
      setCreating(false);
    }
  };

  // Create page in a workspace
  const handleCreatePage = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newName.trim() || !showNewPage) return;
    const ws = showNewPage;
    const baseSlug = newName.toLowerCase().replace(/[^a-z0-9\s-]/g, '').replace(/\s+/g, '-').trim()
      || `page-${crypto.randomUUID().slice(0, 8)}`;
    const conceptId = newPageFolder ? `${newPageFolder}/${baseSlug}` : baseSlug;
    const body = `---\ntype: Note\ntitle: "${newName.trim().replaceAll('"', '\\"')}"\ndescription: ""\n---\n\n`;
    try {
      await writePage(conceptId, body, userBranch, ws.slug, 'wiki', undefined, true);
      const title = newName.trim();
      setShowNewPage(null);
      setNewName('');
      setNewPageFolder(null);
      await loadSpacePages(ws);
      const newPath = conceptPath(conceptId);
      persistedPageBody.current = body;
      setActiveView({ kind: 'page', slug: conceptId, path: newPath, content: { slug: conceptId, path: newPath, title, summary: '', body, branch: userBranch, kind: 'page', children: [] } });
      navigate(pageRoute(ws.slug, conceptId), { replace: true });
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

  // Tab navigation
  const handleTabChange = (tab: NavTab) => {
    if (!activeWorkspace) return;
    setActiveTab(tab);
    const owner = auth?.name || 'user';
    switch (tab) {
      case 'wiki': {
        const pages = spacePages[activeWorkspace.id] || [];
        const first = firstConcept(pages);
        if (first) {
          selectPage(activeWorkspace, conceptIdFromPath(first.path), first.path);
        } else {
          setActiveView(null);
        }
        break;
      }
      case 'reviews':
        setActiveView({ kind: 'review-list', workspaceSlug: activeWorkspace.slug });
        navigate(reviewRoute(owner, activeWorkspace.slug));
        break;
      case 'members':
        setActiveView({ kind: 'members', workspaceSlug: activeWorkspace.slug });
        navigate(`/${owner}/${activeWorkspace.slug}/members`);
        break;
      case 'history':
        setActiveView({ kind: 'history', workspaceSlug: activeWorkspace.slug });
        navigate(`/${owner}/${activeWorkspace.slug}/history`);
        break;
      case 'links':
        setActiveView({ kind: 'links', workspaceSlug: activeWorkspace.slug });
        navigate('/', { replace: true });
        break;
    }
  };

  const openReviewDetail = (target: ReviewTarget) => {
    if (!activeWorkspace) return;
    const owner = auth?.name || 'user';
    setActiveView({ kind: 'review-detail', workspaceSlug: activeWorkspace.slug, target });
    navigate(reviewRoute(owner, activeWorkspace.slug, target));
  };

  // Deterministic local import finishes before optional Agent organization.
  const handleSourcesImported = () => {
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

  // Handle invite submission
  const handleInviteSuccess = (msg: string) => {
    setMessage({ text: msg, type: 'success' });
  };
  const handleInviteError = (msg: string) => {
    setMessage({ text: msg, type: 'error' });
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
    setAuth(getCurrentAuth());
    navigate('/login');
  };

  const handleNameChange = (name: string) => {
    setNewName(name);
    setNewSlug(name.toLowerCase().replace(/[^a-z0-9\s-]/g, '').replace(/\s+/g, '-').trim());
  };


  // Strip frontmatter from page body
  const renderBody = (body: string) => {
    if (body.startsWith('---')) {
      const parts = body.split('---');
      if (parts.length >= 3) return parts.slice(2).join('---').trim();
    }
    return body;
  };

  // Page-view comment context: active only when reading (not editing) a page.
  const pageView = activeView?.kind === 'page' ? activeView : null;
  const commentsActive = !!pageView?.content && !editingPage;
  const commentPageSlug = commentsActive && pageView
    ? (pageView.path ?? pageView.content?.path ?? '')
    : '';
  const commentSource = commentsActive && pageView?.content ? renderBody(pageView.content.body) : '';
  const commentStore = useMemo(() => {
    if (!activeWorkspace) return null;
    if (desktop) {
      if (cloudClient && cloudSession && commentCloudSpace?.slug === activeWorkspace.slug) {
        return cloudPageCommentStore(
          cloudClient,
          commentCloudSpace.id,
          cloudSession.userId,
          cloudSession.userName,
        );
      }
      return localPageCommentStore(activeWorkspace.slug);
    }
    if (cloudClient && cloudSession) {
      return cloudPageCommentStore(
        cloudClient,
        activeWorkspace.id,
        cloudSession.userId,
        cloudSession.userName,
      );
    }
    return null;
  }, [activeWorkspace, cloudClient, cloudSession, commentCloudSpace, desktop]);

  // Execute a pending rename/delete from the tree menus.
  const handlePathOp = async () => {
    if (!pathOp || !activeWorkspace) return;
    const ws = activeWorkspace;
    try {
      if (pathOp.kind === 'rename') {
        const slugified = pathOp.value
          .toLowerCase()
          .replace(/[^a-z0-9\s-]/g, '')
          .trim()
          .replace(/\s+/g, '-') || `item-${crypto.randomUUID().slice(0, 8)}`;
        const separator = pathOp.path.lastIndexOf('/');
        const parent = separator === -1 ? '' : pathOp.path.slice(0, separator);
        const renamed = pathOp.isFolder ? slugified : `${slugified}.md`;
        const to = parent ? `${parent}/${renamed}` : renamed;
        if (to === pathOp.path) { setPathOp(null); return; }
        await renamePath(ws.slug, userBranch, pathOp.path, to);
        setMessage({ text: 'Renamed in your draft.', type: 'success' });
      } else {
        await deletePath(ws.slug, userBranch, pathOp.path);
        setMessage({ text: `Deleted ${pathOp.isFolder ? 'folder' : 'page'} from your draft.`, type: 'success' });
      }
      setPathOp(null);
      // If the open page lived under the changed path, drop the stale view.
      if (activeView?.kind === 'page') {
        const viewPath = activeView.path || conceptPath(activeView.slug);
        if (viewPath === pathOp.path || viewPath.startsWith(pathOp.path + '/')) {
          setActiveView(null);
          navigate(`/`, { replace: true });
        }
      }
      loadSpacePages(ws);
    } catch (e) {
      setMessage({ text: e instanceof Error ? e.message : 'Operation failed', type: 'error' });
    }
  };

  // Autosave an in-page edit to the user's draft branch (the backend amends a
  // single working commit, so frequent saves are fine). Refresh the tree so
  // frontmatter title changes show up while editing.
  const handleSavePage = async (body: string, expectedExternalBody?: string | null) => {
    if (!activeWorkspace || activeView?.kind !== 'page') return;
    const ws = activeWorkspace;
    const restoringDeletedPage = expectedExternalBody === null;
    const expectedBody = restoringDeletedPage
      ? undefined
      : expectedExternalBody ?? persistedPageBody.current ?? activeView.content?.body ?? undefined;
    await writePage(activeView.slug, body, userBranch, ws.slug, 'wiki', expectedBody, restoringDeletedPage);
    persistedPageBody.current = body;
    externalConflictNotified.current = false;
    setActiveView((previous) => previous?.kind === 'page' && previous.content
      ? { ...previous, content: { ...previous.content, body } }
      : previous);
    loadSpacePages(ws);
  };

  // Leave edit mode; pull the saved page back so the read view is fresh.
  const handleDoneEditing = async () => {
    setEditingPage(false);
    if (!activeWorkspace || activeView?.kind !== 'page') return;
    try {
      const page = await getPage(activeView.slug, userBranch, activeWorkspace.slug, 'wiki');
      persistedPageBody.current = page.body;
      setActiveView((prev) => (prev?.kind === 'page' ? { ...prev, content: page } : prev));
    } catch { /* keep the stale view */ }
  };

  const editingPageSlug = activeView?.kind === 'page' ? activeView.slug : null;
  const editingPagePath = activeView?.kind === 'page' ? activeView.path ?? '' : '';

  // Like VS Code, keep a clean open document in step with Agent/terminal file
  // writes. Dirty human text is never replaced: the checked-save path then
  // keeps both versions and asks the user to review the external change.
  useEffect(() => {
    if (!desktop || !editingPage || !activeWorkspace || !editingPageSlug) return;
    const workspace = activeWorkspace;
    const slug = editingPageSlug;
    let checking = false;
    const timer = window.setInterval(() => {
      if (checking) return;
      checking = true;
      getPage(slug, userBranch, workspace.slug, 'wiki')
        .then((latest) => {
          if (latest.body === persistedPageBody.current) return;
          const outcome = pageEditorRef.current?.applyExternalFile(latest.body);
          if (outcome === 'applied') {
            persistedPageBody.current = latest.body;
            externalConflictNotified.current = false;
            setActiveView((previous) => previous?.kind === 'page'
              ? { ...previous, content: latest }
              : previous);
          } else if (outcome === 'conflict' && !externalConflictNotified.current) {
            externalConflictNotified.current = true;
          }
        })
        .catch((error) => {
          if (!isMissingLocalPageError(error)) return;
          const outcome = pageEditorRef.current?.applyExternalDeletion();
          if (outcome === 'conflict') externalConflictNotified.current = true;
        })
        .finally(() => { checking = false; });
    }, 1000);
    return () => window.clearInterval(timer);
  }, [activeWorkspace, desktop, editingPage, editingPagePath, editingPageSlug, userBranch]);

  // Full Concept IDs prevent ambiguous [[overview]] suggestions when two
  // folders contain the same filename.
  const collectSlugs = (pages: PageMeta[]): string[] =>
    pages.flatMap((p) => (p.kind === 'folder' ? collectSlugs(p.children) : [conceptIdFromPath(p.path)]));

  const personal = activeWorkspace ? isPersonalSpace(activeWorkspace) : false;
  const isOwner = activeWorkspace?.role === 'owner';

  // Determine active page/source for panel highlight
  const currentActivePage = activeView?.kind === 'page' ? activeView.slug : null;
  const currentActiveSource = activeView?.kind === 'source' ? activeView.filename : null;
  const selectedAgentChange = versionSelection.kind === 'agent'
    ? openAgentChanges.find((change) => change.id === versionSelection.changeId)
    : undefined;
  const activePagePath = activeView?.kind === 'page'
    ? (activeView.path || conceptPath(activeView.slug))
    : null;
  const workingPageDiff = activePagePath ? findDiffForPath(workingDiffs, activePagePath) : undefined;
  const agentPageDiff = activePagePath && selectedAgentChange
    ? findDiffForPath(selectedAgentChange.diffs, activePagePath)
    : undefined;
  const viewedPageBody = activeView?.kind === 'page' && activeView.content
    ? versionSelection.kind === 'upstream'
      ? (workingPageDiff?.old_content ?? activeView.content.body)
      : versionSelection.kind === 'agent'
        ? (agentPageDiff?.new_content ?? activeView.content.body)
        : activeView.content.body
    : '';
  const viewedPageMissing = versionSelection.kind === 'upstream'
    ? workingPageDiff?.old_content === null
    : versionSelection.kind === 'agent'
      ? agentPageDiff?.new_content === null
      : false;
  const readonlyVersionLabel = versionSelection.kind === 'upstream'
    ? 'Viewing upstream “main”'
    : versionSelection.kind === 'agent'
      ? `Viewing ${selectedAgentChange?.title ?? 'Agent Change'} changes`
      : '';

  if (desktop && workspacesLoaded && workspaces.length === 0) {
    return (
      <>
        <LocalSpaceEmptyState
          creating={creating}
          error={message?.type === 'error' ? message.text : null}
          onOpenFolder={() => {
            setShowCreate(true); setCreateSpaceMode('choose'); setNewName(''); setNewSlug(''); setNewLocalPath('');
          }}
        />
        <CreateSpaceDialog
          open={showCreate}
          desktop
          creating={creating}
          name={newName}
          slug={newSlug}
          localPath={newLocalPath}
          mode={createSpaceMode}
          onModeChange={setCreateSpaceMode}
          onOpenChange={setShowCreate}
          onNameChange={handleNameChange}
          onSlugChange={setNewSlug}
          onChooseFolder={() => { void handleChooseLocalFolder(); }}
          onSubmit={handleCreateWorkspace}
        />
      </>
    );
  }

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
            onCreateWorkspace={() => {
              if (desktop) {
                setShowCreate(true); setCreateSpaceMode('choose'); setNewName(''); setNewSlug(''); setNewLocalPath(''); return;
              }
              if (!hasAuth(auth)) { navigate('/login'); return; }
              setShowCreate(true); setNewName(''); setNewSlug('');
            }}
            onSettings={() => setSettingsOpen(true)}
            onDiscover={() => { setActiveView(null); navigate('/discover'); }}
            onLogout={handleLogout}
            onConnectCloud={() => navigate('/login')}
            notifUnread={notifUnread}
            onShowNotifications={() => setActiveView({ kind: 'notifications' })}
            showBell={isRemoteAuth(auth)}
            showCloudActions={isRemoteAuth(auth)}
          />

          {/* Secondary Panel */}
          {!sidebarLayout.collapsed && (
            <div style={{
              width: sidebarLayout.width, minWidth: sidebarLayout.width, height: '100vh',
              position: 'relative', flexShrink: 0,
            }}>
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
                showReviews={desktop || !personal}
                showLinkDiagnostics={desktop && !!activeWorkspace?.localPath}
                isOwner={isOwner}
                onSelectPage={(slug, path) => activeWorkspace && selectPage(activeWorkspace, slug, path)}
                onSelectSource={(filename) => activeWorkspace && selectSource(activeWorkspace, filename)}
                onNewPage={() => { if (activeWorkspace) { setShowNewPage(activeWorkspace); setNewName(''); setNewPageFolder(null); } }}
                onNewFolder={() => { if (activeWorkspace) { setShowNewFolder(activeWorkspace); setNewName(''); setNewFolderParent(null); } }}
                onAddPageInFolder={(folderPath) => { if (activeWorkspace) { setShowNewPage(activeWorkspace); setNewName(''); setNewPageFolder(folderPath); } }}
                onAddFolderInFolder={(parentPath) => { if (activeWorkspace) { setShowNewFolder(activeWorkspace); setNewName(''); setNewFolderParent(parentPath); } }}
                onRenamePath={(path, isFolder, title) => setPathOp({ kind: 'rename', path, isFolder, title, value: title })}
                onDeletePath={(path, isFolder, title) => setPathOp({ kind: 'delete', path, isFolder, title })}
                onShowIngest={() => setShowIngest(true)}
                onSettings={() => setSettingsOpen(true)}
                onCollapse={() => setSidebarLayout((current) => ({ ...current, collapsed: true }))}
              />
              <div
                role="separator"
                aria-label="Resize sidebar"
                aria-orientation="vertical"
                aria-valuemin={220}
                aria-valuemax={480}
                aria-valuenow={sidebarLayout.width}
                tabIndex={0}
                onPointerDown={(event) => {
                  sidebarResizeStart.current = { pointerX: event.clientX, width: sidebarLayout.width };
                  setResizingSidebar(true);
                }}
                onKeyDown={(event) => {
                  const step = event.shiftKey ? 40 : 12;
                  if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
                    event.preventDefault();
                    const direction = event.key === 'ArrowLeft' ? -1 : 1;
                    setSidebarLayout((current) => ({ ...current, width: clampSidebarWidth(current.width + direction * step) }));
                  }
                }}
                style={{
                  position: 'absolute', top: 0, right: -3, zIndex: 30,
                  width: 6, height: '100%', cursor: 'col-resize', touchAction: 'none',
                  background: resizingSidebar ? C.accent : 'transparent', opacity: resizingSidebar ? 0.32 : 1,
                }}
                onMouseEnter={(event) => { if (!resizingSidebar) event.currentTarget.style.background = C.line; }}
                onMouseLeave={(event) => { if (!resizingSidebar) event.currentTarget.style.background = 'transparent'; }}
              />
            </div>
          )}

          {/* Main Content Area */}
          <main style={{ flex: 1, minWidth: 0, overflow: 'auto', display: 'flex', flexDirection: 'column' }}>
            <CommentsProvider
              store={commentStore}
              pageSlug={commentPageSlug}
              source={commentSource}
              articleRef={articleRef}
            >
            {/* Top bar: breadcrumb + actions */}
            <ContentHeader>
              {/* Left: breadcrumb */}
              <ContentBreadcrumb>
                {sidebarLayout.collapsed && (
                  <button
                    type="button"
                    aria-label="Expand sidebar"
                    title="Expand sidebar"
                    onClick={() => setSidebarLayout((current) => ({ ...current, collapsed: false }))}
                    style={{
                      width: 28, height: 28, padding: 0, marginRight: 4,
                      border: 'none', borderRadius: 7, background: 'transparent',
                      color: C.faint, cursor: 'pointer', flexShrink: 0,
                      display: 'inline-flex', alignItems: 'center', justifyContent: 'center',
                    }}
                    onMouseEnter={(event) => { event.currentTarget.style.background = C.rail; event.currentTarget.style.color = C.ink2; }}
                    onMouseLeave={(event) => { event.currentTarget.style.background = 'transparent'; event.currentTarget.style.color = C.faint; }}
                  >
                    <PanelLeft size={16} />
                  </button>
                )}
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
                    <span style={{ color: C.ink, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                      {activeView.content?.title || activeView.filename}
                    </span>
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
                    <span style={{ color: C.ink }}>
                      {activeView.target.kind === 'local-draft'
                        ? 'Current Draft'
                        : activeView.target.kind === 'local-agent'
                          ? `Agent #${activeView.target.changeId.slice(0, 6)}`
                          : `#${activeView.target.submissionId.slice(0, 6)}`}
                    </span>
                  </>
                )}
                {activeView?.kind === 'members' && (
                  <>
                    <span style={{ color: C.faint }}>/</span>
                    <span style={{ color: C.ink }}>Members</span>
                  </>
                )}
                {activeView?.kind === 'history' && (
                  <>
                    <span style={{ color: C.faint }}>/</span>
                    <span style={{ color: C.ink }}>History</span>
                  </>
                )}
                {activeView?.kind === 'links' && (
                  <>
                    <span style={{ color: C.faint }}>/</span>
                    <span style={{ color: C.ink }}>Links</span>
                  </>
                )}
                {activeView?.kind === 'notifications' && (
                  <span style={{ color: C.ink }}>Notifications</span>
                )}
              </ContentBreadcrumb>

              {/* Right: actions */}
              <ContentHeaderActions>

                {desktop && activeWorkspace?.localPath && (
                  <button
                    type="button"
                    onClick={() => {
                      if (!cloudSession) {
                        navigate('/login');
                        return;
                      }
                      setCloudDialogOpen(true);
                    }}
                    style={headerBtnStyle}
                    title={cloudSession ? 'Publish, sync, or submit this Space' : 'Publish to Cloud'}
                  >
                    <Cloud size={14} /> {cloudSession ? 'Cloud' : 'Publish to Cloud'}
                  </button>
                )}

                {desktop && activeWorkspace?.localPath && !editingPage && (
                  <VersionSwitcher
                    selection={versionSelection}
                    agentChanges={openAgentChanges}
                    onSelect={setVersionSelection}
                    onSeeReviews={() => handleTabChange('reviews')}
                  />
                )}

                {desktop && activeWorkspace?.localPath && (
                  <button
                    type="button"
                    onClick={() => setAgentPanelOpen((open) => !open)}
                    style={{
                      ...headerBtnStyle,
                      color: agentPanelOpen ? C.accent : C.muted,
                      background: agentPanelOpen ? C.accentSoft : 'transparent',
                    }}
                    title="Open Codex or Claude in this Space"
                  >
                    <Bot size={14} /> Agent
                  </button>
                )}

                {/* Wiki-specific actions */}
                {versionSelection.kind === 'working' && activeTab === 'wiki' && (activeView?.kind === 'page' || activeView?.kind === 'source') && (
                  <>
                    {activeView?.kind === 'page' && activeView.content && !editingPage && (
                      <button
                        onClick={() => setEditingPage(true)}
                        style={headerBtnStyle}
                        title={desktop ? 'Edit this local file' : 'Edit this page in your draft'}
                      >
                        <Pencil size={13} /> Edit
                      </button>
                    )}
                    <CommentsHeaderToggle style={{ ...headerBtnStyle, marginLeft: 2 }} />
                  </>
                )}

                {/* User menu moved to Rail bottom avatar */}
              </ContentHeaderActions>
            </ContentHeader>

            {/* Message */}
            {message && (
              <div style={{
                margin: '8px 24px 0', padding: '8px 12px', borderRadius: 6, fontSize: 12,
                background: message.type === 'success' ? C.greenBg : C.redBg,
                color: message.type === 'success' ? C.green : C.red,
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
              defaultAgent={clientSettings.defaultAgent}
              onImported={handleSourcesImported}
              onOrganize={(sources) => {
                setAgentPanelOpen(true);
                setAgentOpenRequest({
                  requestId: crypto.randomUUID(),
                  kind: 'new-change',
                  agent: clientSettings.defaultAgent,
                  title: sources.length === 1
                    ? `Organize Source: ${sources[0].title || sources[0].filename}`
                    : `Organize ${sources.length} Sources`,
                  initialTask: sourceOrganizationTask(sources),
                });
              }}
            />

            {/* Content */}
            <div style={{ flex: 1, padding: '36px 56px 56px', position: 'relative' }}>
              {/* Notifications (cross-space inbox) */}
              {activeView?.kind === 'notifications' ? (
                <NotificationsPage onUnreadChange={setNotifUnread} />

              /* Review detail */
              ) : activeView?.kind === 'review-detail' ? (
                activeView.target.kind === 'cloud' ? (
                  <ReviewDetail
                    workspaceSlug={activeView.workspaceSlug}
                    submissionId={activeView.target.submissionId}
                    onBack={() => handleTabChange('reviews')}
                    onActioned={() => setReviewRefreshKey((k) => k + 1)}
                  />
                ) : (
                  <LocalReviewDetail
                    workspaceSlug={activeView.workspaceSlug}
                    target={activeView.target}
                    onBack={() => handleTabChange('reviews')}
                    onDraftChanged={() => {
                      if (!activeWorkspace || activeWorkspace.slug !== activeView.workspaceSlug) return;
                      void loadSpacePages(activeWorkspace);
                      void loadSpaceSources(activeWorkspace);
                    }}
                    onReviewsChanged={() => setReviewRefreshKey((key) => key + 1)}
                    onContinueAgent={(change: AgentChange) => {
                      setAgentPanelOpen(true);
                      setAgentOpenRequest({
                        requestId: crypto.randomUUID(),
                        kind: 'existing-change',
                        agent: clientSettings.defaultAgent,
                        changeId: change.id,
                        worktreePath: change.worktreePath,
                      });
                    }}
                  />
                )

              /* Review list */
              ) : activeView?.kind === 'review-list' ? (
                <ReviewList
                  workspaceSlug={activeView.workspaceSlug}
                  onOpen={openReviewDetail}
                  refreshKey={reviewRefreshKey}
                />

              /* Members */
              ) : activeView?.kind === 'members' && activeWorkspace ? (
                <MembersView
                  workspaceSlug={activeWorkspace.slug}
                  canManage={isOwner || activeWorkspace.role === 'manager'}
                  currentUserRole={activeWorkspace.role}
                  currentUserId={auth?.id || ''}
                  isOwner={isOwner}
                  onInvite={() => setShowInviteDialog(activeWorkspace)}
                  onTransfer={() => setShowTransferDialog(activeWorkspace)}
                />

              /* Space history */
              ) : activeView?.kind === 'history' && activeWorkspace ? (
                <HistoryView
                  workspaceSlug={activeWorkspace.slug}
                  local={desktop && !!activeWorkspace.localPath}
                />

              /* Link diagnostics */
              ) : activeView?.kind === 'links' && activeWorkspace ? (
                <LinksView
                  workspaceSlug={activeWorkspace.slug}
                  onOpenPage={(path) => selectPage(activeWorkspace, conceptIdFromPath(path), path)}
                />

              /* Source view */
              ) : activeView?.kind === 'source' && activeView.content ? (
                <div>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 16 }}>
                    <span style={{
                      padding: '2px 10px', fontSize: 11, borderRadius: 12,
                      background: C.blueSoft, color: C.blue, fontWeight: 500,
                    }}>
                      Source File
                    </span>
                  </div>
                  <article>
                    <h1 className="page-title page-title--compact source-title">
                      {activeView.content.title || activeView.content.filename}
                    </h1>
                    <div className="prose source-body">
                      <ReactMarkdown remarkPlugins={[remarkGfm]}>
                        {splitSystemFrontmatter(activeView.content.content).body}
                      </ReactMarkdown>
                    </div>
                  </article>
                </div>

              /* Page view */
              ) : activeView?.kind === 'page' && activeView.content ? (
                editingPage ? (
                  <PageEditor
                    ref={pageEditorRef}
                    key={activeView.path ?? activeView.slug}
                    initialBody={activeView.content.body}
                    onSave={handleSavePage}
                    onExternalReload={(fullBody) => {
                      persistedPageBody.current = fullBody;
                      externalConflictNotified.current = false;
                      setActiveView((previous) => previous?.kind === 'page' && previous.content
                        ? { ...previous, content: { ...previous.content, body: fullBody } }
                        : previous);
                    }}
                    onExternalDeleted={() => {
                      externalConflictNotified.current = false;
                      setEditingPage(false);
                      setActiveView(null);
                      navigate('/', { replace: true });
                      if (activeWorkspace) loadSpacePages(activeWorkspace);
                    }}
                    onDone={() => { void handleDoneEditing(); }}
                    onWikilink={(target) => activeWorkspace && selectPage(activeWorkspace, target)}
                    getPageSlugs={() => (activeWorkspace ? collectSlugs(spacePages[activeWorkspace.id] || []) : [])}
                  />
                ) : (
                  // Fill the content box edge-to-edge: the doc scrolls on the left
                  // (left-anchored, same left edge as every other view), the comment
                  // panel is flush to the right edge and full height with its own
                  // scroll. Opening it shrinks the doc from the right only.
                  <PageReader
                    articleRef={articleRef}
                    body={renderBody(viewedPageBody)}
                    markdownComponents={commentMarkdownComponents}
                    byline={versionSelection.kind === 'working' ? {
                      name: activeView.content.edited_by,
                      editedAt: activeView.content.edited_at,
                    } : undefined}
                    readOnlyLabel={versionSelection.kind !== 'working' ? readonlyVersionLabel : undefined}
                    readOnlyDotColor={versionSelection.kind === 'upstream' ? C.purple : C.blue}
                    missingMessage={viewedPageMissing
                      ? 'This page does not exist in the selected version.'
                      : undefined}
                    aside={<CommentsPanel />}
                  />
                )

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
            </CommentsProvider>
          </main>

          {desktop && activeWorkspace?.localPath && (
            <div hidden={!agentPanelOpen} style={{
              width: agentPanelWidth, minWidth: agentPanelWidth, height: '100vh',
              position: 'relative', flexShrink: 0,
            }}>
              <div
                role="separator"
                aria-label="Resize agent terminal"
                aria-orientation="vertical"
                aria-valuemin={320}
                aria-valuemax={720}
                aria-valuenow={agentPanelWidth}
                tabIndex={0}
                onPointerDown={(event) => {
                  agentResizeStart.current = { pointerX: event.clientX, width: agentPanelWidth };
                  setResizingAgentPanel(true);
                }}
                onKeyDown={(event) => {
                  if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
                  event.preventDefault();
                  const step = event.shiftKey ? 40 : 12;
                  const delta = event.key === 'ArrowLeft' ? step : -step;
                  setAgentPanelWidth((width) => Math.max(320, Math.min(720, width + delta)));
                }}
                style={{
                  position: 'absolute', top: 0, left: -3, zIndex: 40,
                  width: 6, height: '100%', cursor: 'col-resize', touchAction: 'none',
                  background: resizingAgentPanel ? C.accent : 'transparent',
                  opacity: resizingAgentPanel ? 0.45 : 1,
                }}
              />
              <AgentTerminalPanel
                key={activeWorkspace.id}
                spacePath={activeWorkspace.localPath}
                spaceSlug={activeWorkspace.slug}
                defaultAgent={clientSettings.defaultAgent}
                openRequest={agentOpenRequest}
                onOpenRequestHandled={(requestId) => {
                  setAgentOpenRequest((current) => (
                    current?.requestId === requestId ? null : current
                  ));
                }}
                onClose={() => setAgentPanelOpen(false)}
              />
            </div>
          )}
        </div>
      </TooltipProvider>

      {/* ── Modals ── */}

      <CreateSpaceDialog
        open={showCreate}
        desktop={desktop}
        creating={creating}
        name={newName}
        slug={newSlug}
        localPath={newLocalPath}
        mode={createSpaceMode}
        onModeChange={setCreateSpaceMode}
        onOpenChange={setShowCreate}
        onNameChange={handleNameChange}
        onSlugChange={setNewSlug}
        onChooseFolder={() => { void handleChooseLocalFolder(); }}
        onSubmit={handleCreateWorkspace}
      />

      {/* New page */}
      <Dialog open={!!showNewPage} onOpenChange={(open) => !open && setShowNewPage(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader><DialogTitle>New Page</DialogTitle></DialogHeader>
          <form onSubmit={handleCreatePage} className="space-y-4 mt-2">
            <div>
              <label className="mb-1.5 block text-sm text-text-secondary">Title</label>
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
              <label className="mb-1.5 block text-sm text-text-secondary">Name</label>
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
              <label className="mb-1.5 block text-sm text-text-secondary">Name</label>
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
      <InviteDialog
        open={!!showInviteDialog}
        workspaceName={showInviteDialog?.name || ''}
        workspaceSlug={showInviteDialog?.slug || ''}
        onOpenChange={(open) => { if (!open) setShowInviteDialog(null); }}
        onInvited={handleInviteSuccess}
        onError={handleInviteError}
      />

      {/* Transfer ownership */}
      <TransferDialog
        open={!!showTransferDialog}
        workspaceName={showTransferDialog?.name || ''}
        workspaceSlug={showTransferDialog?.slug || ''}
        currentUserId={auth?.id || ''}
        onOpenChange={(open) => { if (!open) setShowTransferDialog(null); }}
        onSuccess={handleInviteSuccess}
        onError={handleInviteError}
      />

      {/* Delete workspace confirmation */}
      <Dialog open={!!showDeleteConfirm} onOpenChange={(open) => !open && setShowDeleteConfirm(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader><DialogTitle>Delete Workspace</DialogTitle></DialogHeader>
          <div className="space-y-4 mt-2">
            <p className="text-sm text-text-secondary">
              Are you sure you want to delete <strong>{showDeleteConfirm?.name}</strong>? This action cannot be undone.
            </p>
            <div className="flex justify-end gap-2">
              <Button variant="outline" onClick={() => setShowDeleteConfirm(null)}>Cancel</Button>
              <Button variant="destructive" onClick={handleDeleteWorkspace}>Delete Workspace</Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>

      {/* Rename page/folder */}
      <Dialog open={pathOp?.kind === 'rename'} onOpenChange={(open) => !open && setPathOp(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader><DialogTitle>Rename {pathOp?.isFolder ? 'Folder' : 'Page'}</DialogTitle></DialogHeader>
          <div className="space-y-4 mt-2">
            <Input
              value={pathOp?.kind === 'rename' ? pathOp.value : ''}
              onChange={(e) => setPathOp((p) => (p?.kind === 'rename' ? { ...p, value: e.target.value } : p))}
              onKeyDown={(e) => e.key === 'Enter' && handlePathOp()}
              autoFocus
            />
            <p className="text-xs text-text-secondary">
              Renames the {pathOp?.isFolder ? 'folder path' : 'page slug'} in your draft; links to the old name will need updating.
            </p>
            <div className="flex justify-end gap-2">
              <Button variant="outline" onClick={() => setPathOp(null)}>Cancel</Button>
              <Button onClick={handlePathOp}>Rename</Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>

      {/* Delete page/folder confirm */}
      <Dialog open={pathOp?.kind === 'delete'} onOpenChange={(open) => !open && setPathOp(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader><DialogTitle>Delete {pathOp?.isFolder ? 'Folder' : 'Page'}</DialogTitle></DialogHeader>
          <div className="space-y-4 mt-2">
            <p className="text-sm text-text-secondary">
              Delete <strong>{pathOp?.title}</strong>
              {pathOp?.isFolder ? ' and everything inside it' : ''}?
              {desktop
                ? ' The deletion remains a local Git change until you keep it from Reviews.'
                : ' The change lands on the shared wiki when your submission is merged.'}
            </p>
            <div className="flex justify-end gap-2">
              <Button variant="outline" onClick={() => setPathOp(null)}>Cancel</Button>
              <Button variant="destructive" onClick={handlePathOp}>Delete</Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>

      {/* Settings dialog */}
      <SettingsDialog
        open={settingsOpen}
        onOpenChange={setSettingsOpen}
        clientMode={desktop}
        cloudConnected={isRemoteAuth(auth)}
        auth={auth}
        defaultAgent={clientSettings.defaultAgent}
        onDefaultAgentChange={handleDefaultAgentChange}
        onConnectCloud={() => navigate('/login')}
      />

      {cloudSession && (
        <CloudSpaceDialog
          open={cloudDialogOpen}
          onOpenChange={setCloudDialogOpen}
          space={activeWorkspace ? { name: activeWorkspace.name, slug: activeWorkspace.slug } : null}
          session={cloudSession}
          hasLocalChanges={workingDiffs.length > 0}
          onChanged={() => setReviewRefreshKey((key) => key + 1)}
        />
      )}
    </>
  );
}

function CreateSpaceDialog({
  open,
  desktop,
  creating,
  name,
  slug,
  localPath,
  mode,
  onModeChange,
  onOpenChange,
  onNameChange,
  onSlugChange,
  onChooseFolder,
  onSubmit,
}: {
  open: boolean;
  desktop: boolean;
  creating: boolean;
  name: string;
  slug: string;
  localPath: string;
  mode: CreateSpaceMode;
  onModeChange: (mode: CreateSpaceMode) => void;
  onOpenChange: (open: boolean) => void;
  onNameChange: (name: string) => void;
  onSlugChange: (slug: string) => void;
  onChooseFolder: () => void;
  onSubmit: (event: React.FormEvent) => void;
}) {
  const choice = (
    <div className="grid gap-2 mt-2">
      <SpaceChoice icon={<HardDrive size={19} />} title="Create local Space" description="Start a new OKF knowledge space on this Mac." onClick={() => onModeChange('local')} />
      <SpaceChoice icon={<FolderInput size={19} />} title="Import a folder" description="Open an existing Markdown folder or Git repository." onClick={() => onModeChange('import')} />
      <SpaceChoice icon={<Cloud size={19} />} title="Create Cloud Space" description="Create locally and enable Cloud collaboration." disabled badge="Coming soon" />
      <SpaceChoice icon={<UserPlus size={19} />} title="Join Cloud Space" description="Download a shared Space as a local copy." disabled badge="Coming soon" />
    </div>
  );

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{desktop ? (mode === 'choose' ? 'Add a Space' : mode === 'local' ? 'Create local Space' : 'Import a folder') : 'Create a Space'}</DialogTitle>
        </DialogHeader>
        {desktop && mode === 'choose' ? choice : (
          <form onSubmit={onSubmit} className="space-y-4 mt-2">
            {desktop && (
              <button type="button" onClick={() => onModeChange('choose')} className="inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground">
                <ChevronLeft size={14} /> Back
              </button>
            )}
            <div>
              <label className="mb-1.5 block text-sm text-text-secondary">Space name</label>
              <Input value={name} onChange={(event) => onNameChange(event.target.value)} placeholder="e.g. Research Notes" autoFocus />
            </div>
            {desktop ? (
              <div>
                <label className="mb-1.5 block text-sm text-text-secondary">
                  {mode === 'local' ? 'Create inside' : 'Folder'}
                </label>
                <button type="button" onClick={onChooseFolder} className="w-full rounded-md border px-3 py-2.5 text-left text-sm hover:bg-muted/50">
                  {localPath || (mode === 'local' ? 'Choose a parent folder…' : 'Choose an existing folder…')}
                </button>
                <p className="mt-1.5 text-xs text-muted-foreground">
                  {mode === 'local' ? `CoWiki will create “${name || 'Space name'}” here.` : 'Existing files stay in place and remain the source of truth.'}
                </p>
              </div>
            ) : (
              <input type="hidden" value={slug} onChange={(event) => onSlugChange(event.target.value)} />
            )}
            <div className="flex justify-end gap-2 pt-2">
              <Button variant="outline" type="button" onClick={() => onOpenChange(false)}>Cancel</Button>
              <Button type="submit" disabled={creating || !name.trim() || (desktop && !localPath)}>
                {creating ? 'Creating…' : mode === 'import' ? 'Import' : 'Create'}
              </Button>
            </div>
          </form>
        )}
      </DialogContent>
    </Dialog>
  );
}

function SpaceChoice({ icon, title, description, onClick, disabled, badge }: {
  icon: React.ReactNode;
  title: string;
  description: string;
  onClick?: () => void;
  disabled?: boolean;
  badge?: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className="flex items-center gap-3 rounded-xl border p-3 text-left transition-colors hover:bg-muted/50 disabled:cursor-not-allowed disabled:opacity-45"
    >
      <span className="grid size-9 shrink-0 place-items-center rounded-lg bg-muted text-muted-foreground">{icon}</span>
      <span className="min-w-0 flex-1">
        <span className="flex items-center gap-2 text-sm font-semibold">{title}{badge && <span className="rounded-full bg-muted px-2 py-0.5 text-[10px] font-medium text-muted-foreground">{badge}</span>}</span>
        <span className="mt-0.5 block text-xs text-muted-foreground">{description}</span>
      </span>
    </button>
  );
}

function LocalSpaceEmptyState({
  creating,
  error,
  onOpenFolder,
}: {
  creating: boolean;
  error: string | null;
  onOpenFolder: () => void;
}) {
  return (
    <main style={{
      minHeight: '100vh', display: 'grid', placeItems: 'center',
      background: C.bg, color: C.ink, fontFamily: 'var(--font-sans)',
    }}>
      <section style={{ width: 'min(560px, calc(100vw - 48px))', textAlign: 'center', marginTop: -32 }}>
        <div style={{
          width: 68, height: 68, margin: '0 auto 28px', borderRadius: 20,
          display: 'grid', placeItems: 'center', color: C.accent,
          background: C.accentSoft,
          border: `1px solid color-mix(in srgb, ${C.accent} 20%, transparent)`,
        }}>
          <FolderOpen size={31} strokeWidth={1.8} />
        </div>
        <h1 style={{
          margin: '0 0 18px', fontFamily: 'var(--font-serif)', fontSize: 40,
          lineHeight: 1.1, letterSpacing: '-0.025em', fontWeight: 700,
        }}>
          Add your first Space
        </h1>
        <p style={{ margin: '0 auto 34px', maxWidth: 490, color: C.ink2, fontSize: 17, lineHeight: 1.65 }}>
          Create a new local knowledge space or import an existing folder.<br />
          Cloud can be enabled later without changing its local-first foundation.
        </p>
        <button
          type="button"
          onClick={onOpenFolder}
          disabled={creating}
          style={{
            minWidth: 220, height: 58, padding: '0 24px', border: 'none', borderRadius: 14,
            display: 'inline-flex', alignItems: 'center', justifyContent: 'center', gap: 18,
            background: C.accent, color: C.onAccent, fontSize: 17, fontWeight: 700,
            cursor: creating ? 'default' : 'pointer', opacity: creating ? 0.7 : 1,
            boxShadow: `0 8px 22px color-mix(in srgb, ${C.accent} 20%, transparent)`,
          }}
        >
          {creating ? 'Creating…' : 'Add a Space'}
          {!creating && (
            <kbd style={{
              padding: '2px 8px', borderRadius: 6,
              background: `color-mix(in srgb, ${C.onAccent} 20%, transparent)`,
              font: '600 13px var(--font-sans)',
            }}>
              ⌘O
            </kbd>
          )}
        </button>
        {error && <p role="alert" style={{ color: C.red, fontSize: 13, marginTop: 18 }}>{error}</p>}
      </section>
    </main>
  );
}

const headerBtnStyle: React.CSSProperties = {
  display: 'flex', alignItems: 'center', gap: 6,
  padding: '4px 10px', borderRadius: 6, border: 'none', cursor: 'pointer',
  background: 'transparent', color: C.muted, fontSize: 12,
  transition: 'background 0.1s',
};

function findDiffForPath(diffs: FileDiff[], path: string): FileDiff | undefined {
  const normalized = path.replace(/^\.\//, '');
  return diffs.find((diff) => diff.path.replace(/^\.\//, '') === normalized);
}

function isMissingLocalPageError(error: unknown): boolean {
  const message = String(error).toLowerCase();
  return message.includes('no such file or directory')
    || message.includes('not found')
    || message.includes('cannot find the file');
}

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
                  fontSize: 12, color: C.onAccent, background: C.ink,
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
