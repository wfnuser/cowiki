import { useState, useEffect, useLayoutEffect, useEffectEvent, useCallback, useMemo, useRef } from 'react';
import { isSubmitEnter } from '@/lib/ime';
import { useNavigate, useLocation, useNavigationType } from 'react-router-dom';
import {
  Bot,
  Compass,
  FolderOpen, PanelLeft, MessageSquare, HardDrive, FolderInput, Cloud, UserPlus, ChevronLeft,
  ExternalLink, BookMarked,
} from 'lucide-react';
import {
  Dialog, DialogContent, DialogHeader, DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
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
import { SpaceStatusMenu, type VersionSelection } from '../components/layout/SpaceStatusMenu';
import { TopBarSlots } from '../components/layout/TopBarSlots';
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
import { PageByline } from '../components/PageByline';
import { PageReader } from '../components/PageReader';
import { TransferDialog } from '../components/TransferDialog';
import { NotificationsPage } from '../components/notifications/NotificationsPage';
import { CloudNotificationsPage } from '../cloud/CloudNotificationsPage';
import { notificationUnreadCount } from '../api';
import { CommentsProvider, CommentsPanel, CommentsHeaderToggle, commentMarkdownComponents } from '../components/PageCommentsLayer';
import { C } from '@/lib/design';
import { useI18n } from '@/lib/I18nContext';
import { apiOrigin, isDesktopClient } from '@/runtime';
import { normalizeCloudSession } from '@/cloud/session';
import { CloudSpaceDialog } from '@/components/cloud/CloudSpaceDialog';
import { getCloudStatus, readHtmlAsset, type CloudSyncState } from '@/local-api';
import { HtmlFileReader } from '../components/HtmlFileReader';
import { ExcalidrawReader } from '../components/ExcalidrawDrawing';
import { isExcalidrawPage } from '../lib/excalidraw';
import { showsRoute, viewForRoute } from '../lib/route-view';
import {
  EMPTY_HISTORY, canGoBack, canGoForward, entryKey, forgetPath, forgetSpace, forgetWhere, historyShortcut, movePath, stepAvailable, visit,
  type HistoryEntry, type HistoryView as HistoryStep,
} from '../lib/view-history';
import { HistoryButtons } from '../components/layout/HistoryButtons';
import { isHtmlPath } from '../lib/okf-pages';
import { chooseLocalSpaceDirectory, localSpaceIdentityFromPath } from '@/local-space';
import {
  AgentPanel,
  type AgentPanelOpenRequest,
} from '@/components/agent/AgentPanel';
import {
  conceptIdFromPath,
  conceptPath,
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
import { sourceUrlFromDocument, splitSystemFrontmatter } from '@/lib/page-frontmatter';
import { pageLineage, sourceFilename } from '@/lib/page-lineage';
import { sourceOrganizationTask } from '@/lib/source-ingest';
import { workspaceContextStatus } from '@/lib/workspace-context';
import { createCloudClient } from '@/cloud/client';
import { cloudPageCommentStore, desktopPageCommentStore } from '@/lib/page-comment-store';
import { openExternalUrl } from '@/external-links';
import { TasksView } from '../components/views/TasksView';
import { FlashcardsView } from '../components/views/FlashcardsView';
import { JournalDay, JournalsView } from '../components/views/JournalsView';
import { JournalCalendar } from '../components/views/JournalCalendar';
import { SpaceHome, type HomeNeed } from '../components/views/SpaceHome';
import { loadHomeCounts, type HomeCounts } from '@/lib/home-counts';
import { ObsidianImportDialog } from '../components/ObsidianImportDialog';
import { ObsidianReaderContext, withObsidianReader, type ReaderCallbacks } from '../components/ObsidianReader';
import { autoCheckpoint, readAsset, readSpaceConfig, recentPages, setAutoCheckpoint, writeSpaceConfig, type RecentPage } from '@/local-api';
import { sourceOffsetForClick } from '@/lib/click-to-edit';
import {
  DEFAULT_SPACE_FEATURES,
  loadPersonalPreferences,
  parseSpaceFeatures,
  savePersonalPreferences,
  visibleModules,
  saveSpaceFeatures,
  type PersonalPreferences,
  type SpaceFeatures,
} from '@/lib/space-features';
import { dateKey, fromDateKey, journalTimeline } from '@/lib/journal';
import { useTodayKey } from '@/lib/useTodayKey';
import { ensureDatePage } from '@/lib/date-page';
import { BUILTIN_PLUGINS } from '@/plugins/builtins';
import { createPluginHost, createSpaceActivationTracker, type PluginHostOptions, type PluginSnapshot } from '@/plugins/host';
import { createSpacePageAdapter, PluginUnavailableError } from '@/plugins/space-pages';
import { applyTaskStatus, applyTaskToggle, insertTaskLine, type BoardStatus, type TaskItem } from '@/lib/tasks';
import { flattenPages, parseWikilink, resolveWikilinkPath } from '@/lib/wikilinks';
import { obsidianToReaderMarkdown, renderedLineOffset } from '@/lib/obsidian-markdown';
import { pageFileName } from '@/lib/page-names';
import { newPageBody, templatePagePaths } from '@/lib/page-templates';

/** An automatic checkpoint waits until the draft has been quiet this long. */
const AUTO_CHECKPOINT_SETTLE_SECONDS = 5 * 60;
const LAST_SPACE_STORAGE_KEY = 'cowiki.lastSpace';
const READER_COMPONENTS = withObsidianReader(commentMarkdownComponents);

function readLastSpaceSlug(): string | null {
  try {
    return window.localStorage.getItem(LAST_SPACE_STORAGE_KEY);
  } catch {
    return null;
  }
}

function safePreferences(): PersonalPreferences {
  try {
    return loadPersonalPreferences(window.localStorage);
  } catch {
    return loadPersonalPreferences({ getItem: () => null, setItem: () => undefined });
  }
}

type ActiveView =
  /** The Space's start page: Journals or the Home overview. */
  | { kind: 'home'; workspaceSlug: string }
  | { kind: 'journals'; workspaceSlug: string }
  | { kind: 'page'; slug: string; path?: string; content: PageFull | null }
  | { kind: 'source'; filename: string; content: SourceContent | null }
  | { kind: 'review-list'; workspaceSlug: string }
  | { kind: 'review-detail'; workspaceSlug: string; target: ReviewTarget }
  | { kind: 'members'; workspaceSlug: string }
  | { kind: 'history'; workspaceSlug: string }
  | { kind: 'links'; workspaceSlug: string }
  | { kind: 'tasks'; workspaceSlug: string }
  /** `deck` (`Japanese/N2`) when one deck is open; `studying` during a session. */
  | { kind: 'flashcards'; workspaceSlug: string; deck?: string; studying?: true }
  | { kind: 'notifications' }
  | null;

/** A Flashcards step for Back and Forward, with only the keys it uses (so equal steps compare equal). */
function flashcardsStep(deck?: string | null, studying?: boolean): { kind: 'flashcards'; deck?: string; studying?: true } {
  return { kind: 'flashcards', ...(deck ? { deck } : {}), ...(studying ? { studying: true as const } : {}) };
}

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
  const { t } = useI18n();
  const [auth, setAuth] = useState(() => getCurrentAuth());
  const desktop = isDesktopClient();
  const navigate = useNavigate();
  const location = useLocation();
  const navigationType = useNavigationType();

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
  const pageRequestSeq = useRef(0);
  const sourceRequestSeq = useRef(0);
  const persistedPageBody = useRef<string | null>(null);
  const externalConflictNotified = useRef(false);
  const [reviewRefreshKey, setReviewRefreshKey] = useState(0);
  const [activeTab, setActiveTab] = useState<NavTab>('wiki');
  const [viewHistory, setViewHistory] = useState(EMPTY_HISTORY);
  const viewHistoryRef = useRef(viewHistory);
  useEffect(() => { viewHistoryRef.current = viewHistory; }, [viewHistory]);
  const closedAgentChangesRef = useRef<Set<string>>(new Set());
  const activeAgentChangeIdsRef = useRef<Map<string, ReadonlySet<string>>>(new Map());
  const isClosedAgentEntry = useCallback((entry: HistoryEntry): boolean => {
    if (entry.view.kind !== 'review-detail' || entry.view.target.kind !== 'local-agent') return false;
    const changeId = entry.view.target.changeId;
    if (closedAgentChangesRef.current.has(`${entry.workspaceSlug}\0${changeId}`)) return true;
    const activeIds = activeAgentChangeIdsRef.current.get(entry.workspaceSlug);
    return activeIds !== undefined && !activeIds.has(changeId);
  }, []);
  const pruneNonCurrentClosedAgentHistory = useCallback(() => {
    setViewHistory((history) => {
      const current = history.index >= 0 ? history.entries[history.index] : undefined;
      const currentKey = current ? entryKey(current) : null;
      const next = forgetWhere(history, (entry) => (
        entryKey(entry) !== currentKey && isClosedAgentEntry(entry)
      ));
      viewHistoryRef.current = next;
      return next;
    });
  }, [isClosedAgentEntry]);
  const markAgentChangeClosed = useCallback((workspaceSlug: string, changeId: string) => {
    closedAgentChangesRef.current.add(`${workspaceSlug}\0${changeId}`);
    pruneNonCurrentClosedAgentHistory();
  }, [pruneNonCurrentClosedAgentHistory]);
  const [spaceFeatures, setSpaceFeatures] = useState<Record<string, SpaceFeatures>>({});
  const [featureLoadErrors, setFeatureLoadErrors] = useState<Record<string, string>>({});
  const [preferences, setPreferences] = useState<PersonalPreferences>(safePreferences);
  // Bumped when an automatic checkpoint lands, so an open History view reloads.
  const [checkpointTick, setCheckpointTick] = useState(0);
  const [obsidianImportOpen, setObsidianImportOpen] = useState(false);
  const assetUrls = useRef(new Map<string, string>());
  const [spaceActivation] = useState(createSpaceActivationTracker);
  const pluginSnapshot = useRef<PluginSnapshot>({ space: null, activation: 0, readiness: 'pending', features: null });
  const pluginPages = useRef<PluginHostOptions['pages'] | null>(null);
  const [pluginHost] = useState(() => createPluginHost({
    definitions: BUILTIN_PLUGINS,
    getSnapshot: () => pluginSnapshot.current,
    pages: (space, action, isCurrent) => {
      if (!pluginPages.current) throw new PluginUnavailableError();
      return pluginPages.current(space, action, isCurrent);
    },
  }));
  // loadWorkspaces re-runs on every navigation; opening the first page at '/'
  // is a startup default and must not replace a view chosen later (Links, Tasks).
  const homePageOpened = useRef(false);
  const [reviewCount, setReviewCount] = useState(0);
  const [versionSelection, setVersionSelection] = useState<VersionSelection>({ kind: 'working' });
  const [openAgentChanges, setOpenAgentChanges] = useState<AgentChange[]>([]);
  const [workingDiffs, setWorkingDiffs] = useState<FileDiff[]>([]);
  const [workingDiffsSpaceSlug, setWorkingDiffsSpaceSlug] = useState<string | null>(null);
  const [desktopCloudStatus, setDesktopCloudStatus] = useState<{
    spaceSlug: string;
    state: CloudSyncState | 'unavailable';
  } | null>(null);
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
    setClientSettings((previous) => {
      const next = { ...previous, defaultAgent };
      try { saveClientSettings(window.localStorage, next); } catch { /* keep it for this run */ }
      return next;
    });
  }, []);

  /** New sessions start with the Agent and model used last. */
  const handleAgentUsed = useCallback((agent: DefaultAgent, model: string | null) => {
    setClientSettings((previous) => {
      const agentModels = { ...previous.agentModels };
      if (model) agentModels[agent] = model;
      else delete agentModels[agent];
      const next = { ...previous, defaultAgent: agent, agentModels };
      try { saveClientSettings(window.localStorage, next); } catch { /* keep it for this run */ }
      return next;
    });
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
  // '' (blank), 'builtin:<id>' or 'page:<path>'
  const [newPageTemplate, setNewPageTemplate] = useState('');
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
  // The Settings window opens on the Space's group from the sidebar, the app's from the avatar menu.
  const [settingsGroup, setSettingsGroup] = useState<'space' | 'app'>('app');
  const openSettings = (group: 'space' | 'app') => { setSettingsGroup(group); setSettingsOpen(true); };
  const [cloudDialogOpen, setCloudDialogOpen] = useState(false);
  const [editingPage, setEditingPage] = useState(false);
  // Where a click in the reader asked the editor to put the caret.
  const [editStart, setEditStart] = useState<{ path: string; cursor: number; top: number } | null>(null);
  // Where the editor was scrolled when editing stopped; the reader opens there.
  const [readerScroll, setReaderScroll] = useState<{ path: string; top: number } | null>(null);
  const [readerTargetLine, setReaderTargetLine] = useState<{ path: string; line: number; seq: number } | null>(null);
  const readerTargetSeq = useRef(0);
  // The comments button pressed while editing: back to reading with the panel open.
  const [commentsAfterEdit, setCommentsAfterEdit] = useState(false);
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
  const [commentCloudSpace, setCommentCloudSpace] = useState<{ slug: string; id: string | null } | null>(null);

  useEffect(() => {
    if (!desktop || !activeWorkspace) {
      setCommentCloudSpace(null);
      return;
    }
    let active = true;
    void getCloudStatus(activeWorkspace.slug)
      .then((status) => {
        if (!active) return;
        setCommentCloudSpace({ slug: activeWorkspace.slug, id: status.cloudSpaceId ?? null });
      })
      .catch(() => { if (active) setCommentCloudSpace(null); });
    return () => { active = false; };
  }, [activeWorkspace, cloudSession, desktop, reviewRefreshKey]);

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
    const firstLoad = !homePageOpened.current;
    homePageOpened.current = true;

    // Read the URL at call time: depending on location.pathname re-ran this
    // whole startup load (Space list, full page tree, page) on every click.
    const pathname = window.location.pathname;
    const reviewLocation = parseReviewRoute(pathname, ws.map((workspace) => workspace.slug));
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
    const lastSpaceSlug = desktop ? readLastSpaceSlug() : null;
    const personal = (lastSpaceSlug ? ws.find((w) => w.slug === lastSpaceSlug && w.visibility === 'private') : undefined)
      ?? ws.find((w) => w.visibility === 'private' && w.role === 'owner');
    if (personal) {
      setActiveWorkspace(personal);
      await loadSpacePages(personal);
      await loadSpaceSources(personal);
    }

    // Restore a Concept from its stable bundle-relative ID:
    // /:workspaceSlug/:folder/:concept (the .md suffix is not exposed).
    const pathParts = pathname.split('/').filter(Boolean);
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
    } else if (personal && firstLoad) {
      // Like Logseq, a Space opens on its start page, not on its first file.
      setActiveTab('home');
      setActiveView({ kind: 'home', workspaceSlug: personal.slug });
    }
  }, [auth, desktop, loadSpacePages, loadSpaceSources, userBranch]);

  // Load once per sign-in. `navigate` changes identity on every route change,
  // so depending on loadWorkspaces re-ran the whole startup load per click.
  const loadWorkspacesOnce = useEffectEvent(() => {
    void loadWorkspaces();
  });
  useEffect(() => {
    if (!auth) return;
    const task = window.setTimeout(() => loadWorkspacesOnce(), 0);
    return () => window.clearTimeout(task);
  }, [auth]);

  // Automatic checkpoints for local Spaces: ask the engine once a minute; it
  // saves after five quiet minutes, and again when the app quits.
  const localSpaceSlugs = desktop ? workspaces.filter((ws) => !!ws.localPath).map((ws) => ws.slug).join('\n') : '';
  const onAutoCheckpointTick = useEffectEvent((slugs: string[]) => {
    for (const slug of slugs) {
      autoCheckpoint(slug, AUTO_CHECKPOINT_SETTLE_SECONDS)
        .then((saved) => {
          if (saved && activeWorkspace?.slug === slug) setCheckpointTick((tick) => tick + 1);
        })
        .catch(() => {
          // A Space that cannot be read right now is retried on the next tick.
        });
    }
  });
  useEffect(() => {
    if (!desktop) return;
    setAutoCheckpoint(preferences.autoCheckpoint).catch(() => {});
    if (!preferences.autoCheckpoint || !localSpaceSlugs) return;
    const slugs = localSpaceSlugs.split('\n');
    const timer = window.setInterval(() => onAutoCheckpointTick(slugs), 60_000);
    return () => window.clearInterval(timer);
  }, [desktop, preferences.autoCheckpoint, localSpaceSlugs]);

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
        setWorkingDiffsSpaceSlug(null);
        setVersionSelection({ kind: 'working' });
      }, 0);
      return () => window.clearTimeout(task);
    }
    let cancelled = false;
    const spaceSlug = activeWorkspace.slug;
    Promise.all([
      listLocalAgentChanges(spaceSlug),
      getLocalWorkingDiff(spaceSlug),
    ]).then(([changes, diffs]) => {
      if (cancelled) return;
      const openChanges = changes.filter((change) => change.status === 'open');
      const activeIds = new Set(
        changes
          .filter((change) => change.status === 'open' || change.status === 'needsResolution')
          .map((change) => change.id),
      );
      activeAgentChangeIdsRef.current.set(spaceSlug, activeIds);
      for (const id of activeIds) closedAgentChangesRef.current.delete(`${spaceSlug}\0${id}`);
      pruneNonCurrentClosedAgentHistory();
      setOpenAgentChanges(openChanges);
      setWorkingDiffs(diffs);
      setWorkingDiffsSpaceSlug(spaceSlug);
      setVersionSelection((current) => (
        current.kind === 'agent' && !openChanges.some((change) => change.id === current.changeId)
          ? { kind: 'working' }
          : current
      ));
    }).catch(() => {
      if (cancelled) return;
      setOpenAgentChanges([]);
      setWorkingDiffs([]);
      setWorkingDiffsSpaceSlug(spaceSlug);
      setVersionSelection({ kind: 'working' });
    });
    return () => { cancelled = true; };
  }, [activeWorkspace?.localPath, activeWorkspace?.slug, desktop, pruneNonCurrentClosedAgentHistory, reviewRefreshKey]);

  useEffect(() => {
    if (!desktop || !activeWorkspace?.localPath) {
      const task = window.setTimeout(() => setDesktopCloudStatus(null), 0);
      return () => window.clearTimeout(task);
    }
    let cancelled = false;
    const spaceSlug = activeWorkspace.slug;
    getCloudStatus(spaceSlug)
      .then((status) => {
        if (!cancelled) setDesktopCloudStatus({ spaceSlug, state: status.state });
      })
      .catch(() => {
        if (!cancelled) setDesktopCloudStatus({ spaceSlug, state: 'unavailable' });
      });
    return () => { cancelled = true; };
  }, [activeWorkspace?.localPath, activeWorkspace?.slug, desktop, reviewRefreshKey]);

  // Flush pending page edits before navigating away; returns false if a conflict
  // or save failure kept the editor open.
  const flushBeforeLeave = useCallback(async (): Promise<boolean> => {
    if (editingPage && pageEditorRef.current) {
      if (!(await pageEditorRef.current.flush())) return false;
    }
    setEditingPage(false);
    return true;
  }, [editingPage]);

  // Select a source file
  const selectSource = async (ws: Workspace, filename: string) => {
    if (editingPage && !(await flushBeforeLeave())) return;
    pageRequestSeq.current += 1;
    const requestSeq = ++sourceRequestSeq.current;
    setActiveWorkspace(ws);
    setActiveTab('wiki');
    setActiveView({ kind: 'source', filename, content: null });
    try {
      let content = await getSource(ws.slug, filename, userBranch).catch(() => null);
      if (!content) {
        content = await getSource(ws.slug, filename, 'main');
      }
      if (sourceRequestSeq.current !== requestSeq) return;
      setActiveView(prev => prev?.kind === 'source' && prev.filename === filename ? { ...prev, content } : prev);
    } catch {
      if (sourceRequestSeq.current !== requestSeq) return;
      setActiveView(prev => prev?.kind === 'source' && prev.filename === filename ? { ...prev, content: null } : prev);
    }
  };

  // Switch workspace via rail
  // Invalidate a pending plugin operation immediately, not after React paints.
  const enterPluginSpace = (ws: Workspace) => {
    pluginSnapshot.current = { ...pluginSnapshot.current,
      space: { id: ws.id, local: desktop && !!ws.localPath }, activation: spaceActivation(ws.id),
      readiness: featureLoadErrors[ws.id] ? 'failed' : spaceFeatures[ws.id] ? 'ready' : 'pending',
      features: spaceFeatures[ws.id] ?? null };
  };

  const handleSelectWorkspace = async (ws: Workspace) => {
    if (activeWorkspace?.id === ws.id) return;
    if (editingPage && !(await flushBeforeLeave())) return;
    pageRequestSeq.current += 1;
    sourceRequestSeq.current += 1;
    enterPluginSpace(ws);
    setActiveWorkspace(ws);
    setActiveTab('home');
    setActiveView({ kind: 'home', workspaceSlug: ws.slug });
    navigate('/', { replace: true });
    if (!spacePages[ws.id]) void loadSpacePages(ws);
    if (!spaceSources[ws.id]) void loadSpaceSources(ws);
  };

  // Select a page
  const selectPage = async (
    ws: Workspace,
    slug: string,
    path?: string,
    updateUrl = true,
    isCurrent = () => true,
    targetLine?: number,
  ) => {
    if (!isCurrent()) return;
    const conceptId = path ? conceptIdFromPath(path) : conceptIdFromPath(slug);
    const repoPath = path ? conceptPath(conceptIdFromPath(path)) : conceptPath(conceptId);
    if (isReservedDocument(repoPath)) return;
    if (editingPage && !(await flushBeforeLeave())) return;
    if (!isCurrent()) return;
    sourceRequestSeq.current += 1;
    const requestSeq = ++pageRequestSeq.current;
    const stillCurrent = () => pageRequestSeq.current === requestSeq && isCurrent();
    setActiveWorkspace(ws);
    setActiveTab('wiki');
    setCommentsAfterEdit(false);
    setReaderScroll(null);
    if (targetLine !== undefined) {
      readerTargetSeq.current += 1;
      setReaderTargetLine({ path: repoPath, line: targetLine, seq: readerTargetSeq.current });
    } else {
      setReaderTargetLine(null);
    }
    persistedPageBody.current = null;
    externalConflictNotified.current = false;
    setActiveView({ kind: 'page', slug: conceptId, path: repoPath, content: null });
    if (updateUrl) navigate(pageRoute(ws.slug, conceptId), { replace: true });
    const setContent = (content: PageFull | null) => {
      if (!stillCurrent()) return;
      persistedPageBody.current = content?.body ?? null;
      setActiveView((prev) => (
        prev?.kind === 'page' && prev.path === repoPath ? { ...prev, content } : prev
      ));
    };
    const handleMissing = () => {
      if (!stillCurrent()) return;
      setContent(null);
      setViewHistory((history) => forgetPath(history, ws.slug, repoPath));
      setMessage({ text: `Page "${conceptId}" not found`, type: 'error' });
    };

    if (ws.visibility === 'private') {
      try {
        const page = await getPage(conceptId, userBranch, ws.slug, 'wiki');
        setContent(page);
      } catch {
        handleMissing();
      }
    } else {
      try {
        const page = await getPage(conceptId, userBranch, ws.slug, 'wiki');
        setContent(page);
      } catch {
        if (!stillCurrent()) return;
        try {
          const page = await getPage(conceptId, 'main', ws.slug, 'wiki');
          setContent(page);
        } catch {
          handleMissing();
        }
      }
    }
  };

  // Back/forward change only the URL; show the view it stands for without
  // reloading the Space. The first URL is handled by the startup load above.
  const syncViewWithRoute = useEffectEvent((pathname: string) => {
    if (!homePageOpened.current) return;
    const route = viewForRoute(pathname, workspaces.map((workspace) => workspace.slug));
    if (!route || showsRoute(activeView, activeWorkspace?.slug ?? null, route)) return;
    const ws = workspaces.find((workspace) => workspace.slug === route.workspaceSlug);
    if (!ws) return;
    if (route.kind === 'page') {
      void selectPage(ws, route.conceptId, undefined, false);
      return;
    }
    if (activeWorkspace?.slug !== ws.slug) {
      setActiveWorkspace(ws);
      void loadSpacePages(ws);
    }
    setActiveTab('reviews');
    setEditingPage(false);
    setActiveView(route);
  });
  useEffect(() => {
    if (navigationType !== 'POP') return;
    const task = window.setTimeout(() => syncViewWithRoute(location.pathname), 0);
    return () => window.clearTimeout(task);
  }, [location.key, location.pathname, navigationType]);

  // Back and forward (desktop): every view you open is one step. The browser
  // keeps its own history on the web, so this is only the desktop app's.
  const historyEntry: HistoryEntry | null = (() => {
    if (!desktop || !activeWorkspace || !activeView) return null;
    const workspaceSlug = activeWorkspace.slug;
    switch (activeView.kind) {
      case 'page': return { workspaceSlug, view: { kind: 'page', path: activeView.path ?? conceptPath(activeView.slug) } };
      case 'source': return { workspaceSlug, view: { kind: 'source', filename: activeView.filename } };
      case 'review-detail': return { workspaceSlug, view: { kind: 'review-detail', target: activeView.target } };
      case 'flashcards': return { workspaceSlug, view: flashcardsStep(activeView.deck, activeView.studying) };
      case 'notifications': return null;
      default: return { workspaceSlug, view: { kind: activeView.kind } };
    }
  })();
  const historyKey = historyEntry ? entryKey(historyEntry) : null;
  const recordHistory = useEffectEvent(() => {
    // Arriving where back or forward pointed matches the current entry, so it adds nothing.
    // Drop any closed Agent Changes before appending the new step so leaving a merged/discarded
    // change removes it from the Back stack cleanly.
    if (!historyEntry) return;
    const nextKey = entryKey(historyEntry);
    setViewHistory((history) => {
      const cleaned = forgetWhere(history, (entry) => (
        entryKey(entry) !== nextKey && isClosedAgentEntry(entry)
      ));
      const updated = visit(cleaned, historyEntry);
      viewHistoryRef.current = updated;
      return updated;
    });
  });
  useEffect(() => { recordHistory(); }, [historyKey]);

  const openHistoryView = (ws: Workspace, view: HistoryStep) => {
    if (activeWorkspace?.id !== ws.id) enterPluginSpace(ws);
    if (view.kind === 'page') {
      void selectPage(ws, conceptIdFromPath(view.path), view.path);
      return;
    }
    if (view.kind === 'source') {
      void selectSource(ws, view.filename);
      return;
    }
    pageRequestSeq.current += 1;
    sourceRequestSeq.current += 1;
    if (activeWorkspace?.id !== ws.id) {
      setActiveWorkspace(ws);
      if (!spacePages[ws.id]) void loadSpacePages(ws);
      if (!spaceSources[ws.id]) void loadSpaceSources(ws);
    }
    setEditingPage(false);
    const owner = auth?.name || 'user';
    switch (view.kind) {
      case 'review-list':
        setActiveTab('reviews');
        setActiveView({ kind: 'review-list', workspaceSlug: ws.slug });
        navigate(reviewRoute(owner, ws.slug), { replace: true });
        break;
      case 'review-detail':
        setActiveTab('reviews');
        setActiveView({ kind: 'review-detail', workspaceSlug: ws.slug, target: view.target });
        navigate(reviewRoute(owner, ws.slug, view.target), { replace: true });
        break;
      case 'members':
      case 'history':
        setActiveTab(view.kind);
        setActiveView({ kind: view.kind, workspaceSlug: ws.slug });
        navigate(`/${owner}/${ws.slug}/${view.kind}`, { replace: true });
        break;
      case 'flashcards':
        setActiveTab('flashcards');
        setActiveView({ workspaceSlug: ws.slug, ...flashcardsStep(view.deck, view.studying) });
        navigate('/', { replace: true });
        break;
      default:
        setActiveTab(view.kind);
        setActiveView({ kind: view.kind, workspaceSlug: ws.slug });
        navigate('/', { replace: true });
    }
  };

  /** A view whose module was turned off since cannot be shown (it would be a blank page). */
  const historyViewAvailable = (ws: Workspace, view: HistoryStep) => {
    const modules = spaceFeatures[ws.id]?.modules;
    if (!modules) return true;
    if (view.kind === 'journals') return modules.journal !== false;
    if (view.kind === 'tasks') return modules.tasks !== false;
    if (view.kind === 'flashcards') return modules.flashcards !== false;
    return true;
  };

  // Read the latest viewHistory via ref so stepping after an async flush never
  // acts on a stale pre-await closure.
  const stepHistory = (delta: -1 | 1) => {
    // Skip steps that can no longer be shown: a removed Space, a module turned off,
    // or an Agent Change that was merged or discarded.
    const isUnavailable = (entry: HistoryEntry) => {
      const ws = workspaces.find((workspace) => workspace.slug === entry.workspaceSlug);
      return !ws || !historyViewAvailable(ws, entry.view) || isClosedAgentEntry(entry);
    };
    const next = stepAvailable(viewHistoryRef.current, delta, isUnavailable);
    const ws = next && workspaces.find((workspace) => workspace.slug === next.entry.workspaceSlug);
    if (!next || !ws) {
      const current = viewHistoryRef.current.index >= 0
        ? viewHistoryRef.current.entries[viewHistoryRef.current.index]
        : undefined;
      const currentKey = current ? entryKey(current) : null;
      const cleaned = forgetWhere(viewHistoryRef.current, (entry) => (
        entryKey(entry) !== currentKey && isUnavailable(entry)
      ));
      viewHistoryRef.current = cleaned;
      setViewHistory(cleaned);
      return;
    }
    viewHistoryRef.current = next.history;
    setViewHistory(next.history);
    openHistoryView(ws, next.entry.view);
  };

  const goThroughHistory = (delta: -1 | 1) => {
    if (editingPage && pageEditorRef.current) {
      void (async () => {
        if (!(await flushBeforeLeave())) return;
        stepHistory(delta);
      })();
      return;
    }
    stepHistory(delta);
  };

  const onHistoryKey = useEffectEvent((event: KeyboardEvent) => {
    if (event.defaultPrevented || event.isComposing) return;
    const target = event.target instanceof Element ? event.target : null;
    const inText = !!target?.closest('input, textarea, select, [contenteditable=""], [contenteditable="true"], .cm-editor');
    const delta = historyShortcut(event, inText);
    if (delta === null) return;
    event.preventDefault();
    goThroughHistory(delta);
  });
  // The mouse's side buttons, as in a browser.
  const onHistoryMouse = useEffectEvent((event: MouseEvent) => {
    if (event.button !== 3 && event.button !== 4) return;
    event.preventDefault();
    goThroughHistory(event.button === 3 ? -1 : 1);
  });
  useEffect(() => {
    if (!desktop) return;
    const key = (event: KeyboardEvent) => onHistoryKey(event);
    const mouse = (event: MouseEvent) => onHistoryMouse(event);
    window.addEventListener('keydown', key);
    window.addEventListener('mouseup', mouse);
    return () => {
      window.removeEventListener('keydown', key);
      window.removeEventListener('mouseup', mouse);
    };
  }, [desktop]);

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

  // ── Personal workspace: Space modules, journal, tasks, Obsidian links ──

  const isLocalSpace = (ws: Workspace | null): ws is Workspace => !!ws && desktop && !!ws.localPath;

  const readFeatureConfig = useCallback(async (ws: Workspace) => {
    try {
      return await readSpaceConfig(ws.slug);
    } catch (error) {
      if (pluginSnapshot.current.space?.id === ws.id) {
        pluginSnapshot.current = { ...pluginSnapshot.current, readiness: 'failed', features: null };
      }
      setSpaceFeatures((previous) => { const next = { ...previous }; delete next[ws.id]; return next; });
      setFeatureLoadErrors((previous) => ({ ...previous, [ws.id]: error instanceof Error ? error.message : String(error) }));
      throw error;
    }
  }, []);

  const loadSpaceFeatures = useCallback(async (ws: Workspace): Promise<SpaceFeatures> => {
    if (!desktop || !ws.localPath) return DEFAULT_SPACE_FEATURES;
    const features = parseSpaceFeatures(await readFeatureConfig(ws));
    setSpaceFeatures((previous) => ({ ...previous, [ws.id]: features }));
    setFeatureLoadErrors((previous) => { const next = { ...previous }; delete next[ws.id]; return next; });
    return features;
  }, [desktop, readFeatureConfig]);

  useEffect(() => {
    if (!activeWorkspace || spaceFeatures[activeWorkspace.id] || featureLoadErrors[activeWorkspace.id] || !isLocalSpace(activeWorkspace)) return;
    void loadSpaceFeatures(activeWorkspace).catch(() => {});
    // isLocalSpace only reads stable props of the workspace.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeWorkspace, spaceFeatures, featureLoadErrors, loadSpaceFeatures]);

  useEffect(() => {
    if (!desktop || !activeWorkspace?.slug) return;
    try {
      window.localStorage.setItem(LAST_SPACE_STORAGE_KEY, activeWorkspace.slug);
    } catch {
      // Remembering the last Space is a convenience only.
    }
  }, [desktop, activeWorkspace?.slug]);

  const activeFeatures = (activeWorkspace && spaceFeatures[activeWorkspace.id]) || DEFAULT_SPACE_FEATURES;
  const activeFeaturesReady = !!activeWorkspace && !!spaceFeatures[activeWorkspace.id];
  const activeModules = isLocalSpace(activeWorkspace) && activeFeaturesReady
    ? visibleModules(activeFeatures).filter((id) => !BUILTIN_PLUGINS.some((plugin) => plugin.id === id))
    : [];
  const activeFlatPages = useMemo(
    () => flattenPages(activeWorkspace ? (spacePages[activeWorkspace.id] || []) : []),
    [activeWorkspace, spacePages],
  );
  // A page port is scoped to one Space. Plugins cannot request another Space,
  // overwrite a file, or bypass ordinary page navigation.
  const pagePorts = (ws: Workspace, isCurrent: () => boolean, actionId?: string) => ({
    read: async (path: string) => (await getPage(conceptIdFromPath(path), userBranch, ws.slug, 'wiki')).body,
    createOnly: (path: string, body: string) => writePage(conceptIdFromPath(path), body, userBranch, ws.slug, 'wiki', undefined, true),
    refresh: async () => { await loadSpacePages(ws); },
    open: async (path: string) => {
      await selectPage(ws, conceptIdFromPath(path), path, true, isCurrent);
      if (!isCurrent()) return;
      if (actionId) setActiveTab(`plugin:${actionId}`);
      setEditingPage(true);
    },
  });

  useLayoutEffect(() => {
    pluginSnapshot.current = {
      space: activeWorkspace ? { id: activeWorkspace.id, local: isLocalSpace(activeWorkspace) } : null,
      activation: spaceActivation(activeWorkspace?.id ?? null),
      readiness: activeWorkspace && featureLoadErrors[activeWorkspace.id] ? 'failed' : activeFeaturesReady ? 'ready' : 'pending',
      features: activeWorkspace ? spaceFeatures[activeWorkspace.id] ?? null : null,
    };
    pluginPages.current = (space, action, isCurrent) => {
      const ws = workspaces.find((workspace) => workspace.id === space.id);
      if (!ws) throw new PluginUnavailableError();
      return pagePorts(ws, isCurrent, action.id);
    };
  });

  // Listing is pure and uses this render's snapshot; invocation re-resolves the
  // current snapshot in the host rather than trusting a stale sidebar button.
  const pluginActions = pluginHost.actions({
    space: activeWorkspace ? { id: activeWorkspace.id, local: isLocalSpace(activeWorkspace) } : null,
    readiness: activeWorkspace && featureLoadErrors[activeWorkspace.id] ? 'failed' : activeFeaturesReady ? 'ready' : 'pending',
    features: activeFeaturesReady ? activeFeatures : null,
  });

  const dispatchPluginAction = async (id: string) => {
    try { await pluginHost.dispatch(id); }
    catch (error) { setMessage({ text: error instanceof Error ? error.message : String(error), type: 'error' }); }
  };

  /** Navigate like Obsidian: resolve by name anywhere, create the note if missing. */
  const openWikilink = async (raw: string) => {
    const ws = activeWorkspace;
    if (!ws) return;
    const currentPath = activeView?.kind === 'page' ? (activeView.path ?? conceptPath(activeView.slug)) : null;
    const resolved = resolveWikilinkPath(raw, activeFlatPages, currentPath);
    if (resolved) {
      await selectPage(ws, conceptIdFromPath(resolved), resolved);
      return;
    }
    const { page } = parseWikilink(raw);
    if (!page) return;
    if (!isLocalSpace(ws)) {
      setMessage({ text: `Page "${page}" not found`, type: 'error' });
      return;
    }
    const title = page.split('/').at(-1) ?? page;
    const body = `---\ntype: Note\ntitle: "${title.replaceAll('"', '\\"')}"\n---\n\n# ${title}\n\n`;
    try {
      await writePage(page, body, userBranch, ws.slug, 'wiki', undefined, true);
    } catch (error) {
      if (!String(error instanceof Error ? error.message : error).includes('already exists')) {
        setMessage({ text: error instanceof Error ? error.message : 'Could not create the page', type: 'error' });
        return;
      }
    }
    await loadSpacePages(ws);
    await selectPage(ws, page, conceptPath(page));
    setEditingPage(true);
  };

  const viewedPagePath = activeView?.kind === 'page' ? (activeView.path ?? conceptPath(activeView.slug)) : '';
  const viewedSpaceSlug = activeWorkspace?.slug ?? '';
  const canReadAssets = isLocalSpace(activeWorkspace);
  /** An attachment referenced from `fromPath`, as a displayable URL. */
  const loadAssetFrom = useCallback(async (fromPath: string, src: string): Promise<string> => {
    if (!canReadAssets || !viewedSpaceSlug) throw new Error('Attachments are only available for local Spaces');
    const key = `${viewedSpaceSlug}\u0000${fromPath}\u0000${src}`;
    const cached = assetUrls.current.get(key);
    if (cached) return cached;
    const asset = await readAsset(viewedSpaceSlug, fromPath, src);
    const url = URL.createObjectURL(new Blob([new Uint8Array(asset.bytes)], { type: asset.mime }));
    assetUrls.current.set(key, url);
    return url;
  }, [canReadAssets, viewedSpaceSlug]);
  const loadAssetUrl = useCallback((src: string) => loadAssetFrom(viewedPagePath, src), [loadAssetFrom, viewedPagePath]);

  /** Toggle a task from the reader; `renderedLine` is 1-based in the rendered body. */
  const toggleReaderTask = async (renderedLine: number) => {
    const ws = activeWorkspace;
    if (!ws || activeView?.kind !== 'page' || !activeView.content) return;
    const full = activeView.content.body;
    const index = renderedLineOffset(full, renderBody(full)) + renderedLine - 1;
    const raw = full.split(/\r?\n/)[index];
    if (raw === undefined) return;
    try {
      const updated = applyTaskToggle(full, { line: index, raw }, dateKey(new Date()));
      await writePage(activeView.slug, updated, userBranch, ws.slug, 'wiki', full);
      const page = await getPage(activeView.slug, userBranch, ws.slug, 'wiki');
      persistedPageBody.current = page.body;
      setActiveView((previous) => previous?.kind === 'page' ? { ...previous, content: page } : previous);
    } catch (error) {
      setMessage({ text: error instanceof Error ? error.message : 'Could not update the task', type: 'error' });
    }
  };

  const toggleTaskInPage = async (task: TaskItem) => {
    const ws = activeWorkspace;
    if (!ws) return;
    const conceptId = conceptIdFromPath(task.path);
    const page = await getPage(conceptId, userBranch, ws.slug, 'wiki');
    const updated = applyTaskToggle(page.body, task, dateKey(new Date()));
    await writePage(conceptId, updated, userBranch, ws.slug, 'wiki', page.body);
  };

  const setTaskStatusInPage = async (task: TaskItem, status: BoardStatus) => {
    const ws = activeWorkspace;
    if (!ws) return;
    const conceptId = conceptIdFromPath(task.path);
    const page = await getPage(conceptId, userBranch, ws.slug, 'wiki');
    const updated = applyTaskStatus(page.body, task, status, dateKey(new Date()));
    await writePage(conceptId, updated, userBranch, ws.slug, 'wiki', page.body);
  };

  /** Today's journal entry in the active Space, created from its template when missing. */
  const ensureTodayEntry = async (ws: Workspace) => {
    // Leaving the Space, even to come straight back, drops a write still in flight.
    const activation = pluginSnapshot.current.activation;
    const isCurrent = () => pluginSnapshot.current.space?.id === ws.id && pluginSnapshot.current.activation === activation;
    const features = spaceFeatures[ws.id] ?? await loadSpaceFeatures(ws);
    const pages = createSpacePageAdapter(pagePorts(ws, isCurrent), isCurrent);
    return { pages, path: await ensureDatePage(pages, features.journal, new Date()) };
  };

  const addTaskToToday = async (line: string) => {
    const ws = activeWorkspace;
    if (!ws) return;
    const { pages, path } = await ensureTodayEntry(ws);
    const page = await pages.read(path);
    if (page === null) throw new Error('Today’s page was removed before adding the task');
    await writePage(conceptIdFromPath(path), insertTaskLine(page, line), userBranch, ws.slug, 'wiki', page);
  };

  /** Personal preferences apply at once: only this person, only this computer. */
  const savePreferences = (nextPreferences: PersonalPreferences) => {
    try {
      savePersonalPreferences(window.localStorage, nextPreferences);
    } catch {
      // Preferences fall back to this session when storage is unavailable.
    }
    setPreferences(nextPreferences);
  };

  const saveModules = async (features: SpaceFeatures, nextPreferences: PersonalPreferences) => {
    const ws = activeWorkspace;
    if (!ws || !isLocalSpace(ws) || !spaceFeatures[ws.id]) throw new Error('Space settings are not ready');
    const saved = await saveSpaceFeatures({ read: () => readFeatureConfig(ws), write: (config) => writeSpaceConfig(ws.slug, config) }, features, spaceFeatures[ws.id]);
    if (pluginSnapshot.current.space?.id === ws.id) pluginSnapshot.current = { ...pluginSnapshot.current, features: saved };
    setSpaceFeatures((previous) => ({ ...previous, [ws.id]: saved }));
    try {
      savePersonalPreferences(window.localStorage, nextPreferences);
    } catch {
      // Preferences fall back to this session when storage is unavailable.
    }
    setPreferences(nextPreferences);
  };

  const handleObsidianImported = async (space: Workspace) => {
    setObsidianImportOpen(false);
    const ws = await listWorkspaces();
    setWorkspaces(ws);
    const created = ws.find((workspace) => workspace.slug === space.slug) ?? space;
    setActiveWorkspace(created);
    await loadSpacePages(created);
    await loadSpaceFeatures(created);
    await selectPage(created, 'CoWiki Import Report', 'CoWiki Import Report.md');
    setMessage({ text: `Imported ${created.name}. Your Obsidian vault was not changed.`, type: 'success' });
  };

  const readerCallbacks: ReaderCallbacks = {
    onWikilink: (target) => { void openWikilink(target); },
    loadAsset: canReadAssets ? loadAssetUrl : undefined,
    onToggleTask: canReadAssets && versionSelection.kind === 'working' ? (line) => { void toggleReaderTask(line); } : undefined,
  };
  const newPageTemplates = showNewPage
    ? templatePagePaths(
      flattenPages(spacePages[showNewPage.id] || []).map((page) => page.path),
      spaceFeatures[showNewPage.id] ?? DEFAULT_SPACE_FEATURES,
    )
    : [];
  // Create page in a workspace
  const handleCreatePage = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newName.trim() || !showNewPage) return;
    const ws = showNewPage;
    // Local Spaces keep readable file names so [[Name]] links find the page.
    const baseSlug = (isLocalSpace(ws)
      ? pageFileName(newName)
      : newName.toLowerCase().replace(/[^a-z0-9\s-]/g, '').replace(/\s+/g, '-').trim())
      || `page-${crypto.randomUUID().slice(0, 8)}`;
    const conceptId = newPageFolder ? `${newPageFolder}/${baseSlug}` : baseSlug;
    try {
      let template: { body: string; type?: string } | null = null;
      if (newPageTemplate.startsWith('page:')) {
        const templatePath = newPageTemplate.slice('page:'.length);
        template = { body: (await getPage(conceptIdFromPath(templatePath), userBranch, ws.slug, 'wiki')).body };
      }
      const body = newPageBody(newName.trim(), template);
      await writePage(conceptId, body, userBranch, ws.slug, 'wiki', undefined, true);
      const title = newName.trim();
      setShowNewPage(null);
      setNewName('');
      setNewPageFolder(null);
      setNewPageTemplate('');
      await loadSpacePages(ws);
      const newPath = conceptPath(conceptId);
      // The engine may normalize properties; edit what was actually saved.
      const saved = await getPage(conceptId, userBranch, ws.slug, 'wiki')
        .catch(() => ({ slug: conceptId, path: newPath, title, summary: '', body, branch: userBranch, kind: 'page' as const, children: [] }));
      persistedPageBody.current = saved.body;
      setActiveView({ kind: 'page', slug: conceptId, path: newPath, content: saved });
      navigate(pageRoute(ws.slug, conceptId), { replace: true });
    } catch (error) {
      setMessage({ text: error instanceof Error ? `Failed to create page: ${error.message}` : 'Failed to create page', type: 'error' });
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
    const ws = activeWorkspace;
    const apply = () => {
      pageRequestSeq.current += 1;
      sourceRequestSeq.current += 1;
      setEditingPage(false);
      setActiveTab(tab);
      const owner = auth?.name || 'user';
      switch (tab) {
        case 'home':
        case 'journals':
          setJournalJump(null);
          setActiveView({ kind: tab, workspaceSlug: ws.slug });
          navigate('/', { replace: true });
          break;
        case 'reviews':
          setActiveView({ kind: 'review-list', workspaceSlug: ws.slug });
          navigate(reviewRoute(owner, ws.slug));
          break;
        case 'members':
          setActiveView({ kind: 'members', workspaceSlug: ws.slug });
          navigate(`/${owner}/${ws.slug}/members`);
          break;
        case 'history':
          setActiveView({ kind: 'history', workspaceSlug: ws.slug });
          navigate(`/${owner}/${ws.slug}/history`);
          break;
        case 'links':
          setActiveView({ kind: 'links', workspaceSlug: ws.slug });
          navigate('/', { replace: true });
          break;
        case 'flashcards':
          setActiveView({ kind: 'flashcards', workspaceSlug: ws.slug });
          navigate('/', { replace: true });
          break;
        case 'tasks':
          setActiveView({ kind: 'tasks', workspaceSlug: ws.slug });
          navigate('/', { replace: true });
          break;
      }
    };
    if (editingPage && pageEditorRef.current) {
      void (async () => {
        if (!(await flushBeforeLeave())) return;
        apply();
      })();
      return;
    }
    apply();
  };

  const openReviewDetail = (target: ReviewTarget) => {
    if (!activeWorkspace) return;
    const ws = activeWorkspace;
    const apply = () => {
      pageRequestSeq.current += 1;
      sourceRequestSeq.current += 1;
      setEditingPage(false);
      const owner = auth?.name || 'user';
      setActiveView({ kind: 'review-detail', workspaceSlug: ws.slug, target });
      navigate(reviewRoute(owner, ws.slug, target));
    };
    if (editingPage && pageEditorRef.current) {
      void (async () => {
        if (!(await flushBeforeLeave())) return;
        apply();
      })();
      return;
    }
    apply();
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
    const deletedSlug = showDeleteConfirm.slug;
    try {
      await deleteWorkspace(deletedSlug);
      activeAgentChangeIdsRef.current.delete(deletedSlug);
      for (const key of [...closedAgentChangesRef.current]) {
        if (key.startsWith(`${deletedSlug}\0`)) closedAgentChangesRef.current.delete(key);
      }
      setViewHistory((history) => forgetSpace(history, deletedSlug));
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
  const isHtmlPage = !!pageView && isHtmlPath(pageView.path ?? pageView.slug);
  const isDrawingPage = !!pageView?.content && isExcalidrawPage(pageView.path ?? pageView.slug, pageView.content.body);
  const htmlDocumentPath = pageView?.path ?? pageView?.slug ?? '';
  const htmlSpaceSlug = activeWorkspace?.slug ?? '';
  const htmlAssetReader = useCallback((assetPath: string) => {
    if (!htmlSpaceSlug || !htmlDocumentPath) return Promise.reject(new Error('No HTML document selected'));
    return readHtmlAsset(htmlSpaceSlug, htmlDocumentPath, assetPath);
  }, [htmlSpaceSlug, htmlDocumentPath]);
  const commentsActive = !!pageView?.content && !editingPage && !isHtmlPage && !isDrawingPage;
  // Click to edit, as in Logseq: the working version of a Markdown page.
  const canEditInline = !!pageView?.content && !isHtmlPage && !isDrawingPage && versionSelection.kind === 'working';
  const commentPageSlug = commentsActive && pageView
    ? (pageView.path ?? pageView.content?.path ?? '')
    : '';
  const commentSource = commentsActive && pageView?.content ? renderBody(pageView.content.body) : '';
  const commentStore = useMemo(() => {
    if (!activeWorkspace) return null;
    if (desktop) {
      return desktopPageCommentStore(activeWorkspace.slug, commentCloudSpace, cloudClient, cloudSession);
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
        const slugified = (isLocalSpace(ws)
          ? pageFileName(pathOp.value)
          : pathOp.value
            .toLowerCase()
            .replace(/[^a-z0-9\s-]/g, '')
            .trim()
            .replace(/\s+/g, '-')) || `item-${crypto.randomUUID().slice(0, 8)}`;
        const separator = pathOp.path.lastIndexOf('/');
        const parent = separator === -1 ? '' : pathOp.path.slice(0, separator);
        const renamed = pathOp.isFolder ? slugified : `${slugified}.md`;
        const to = parent ? `${parent}/${renamed}` : renamed;
        if (to === pathOp.path) { setPathOp(null); return; }
        const updatedLinks = await renamePath(ws.slug, userBranch, pathOp.path, to);
        setViewHistory((history) => movePath(history, ws.slug, pathOp.path, to));
        setMessage({
          text: updatedLinks
            ? `Renamed in your draft and updated links in ${updatedLinks} ${updatedLinks === 1 ? 'page' : 'pages'}.`
            : 'Renamed in your draft.',
          type: 'success',
        });
      } else {
        await deletePath(ws.slug, userBranch, pathOp.path);
        setViewHistory((history) => forgetPath(history, ws.slug, pathOp.path));
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

  // Leave edit mode; flush any pending save first, then pull the saved page so the read view is fresh.
  const handleDoneEditing = async (options?: { openComments?: boolean }): Promise<boolean> => {
    if (pageEditorRef.current && !(await pageEditorRef.current.flush())) return false;
    if (activeView?.kind === 'page') {
      setReaderScroll({ path: activeView.path ?? '', top: pageEditorRef.current?.scrollTop() ?? 0 });
    }
    if (options?.openComments) setCommentsAfterEdit(true);
    setEditingPage(false);
    if (!activeWorkspace || activeView?.kind !== 'page') return true;
    try {
      const page = await getPage(activeView.slug, userBranch, activeWorkspace.slug, 'wiki');
      persistedPageBody.current = page.body;
      setActiveView((prev) => (prev?.kind === 'page' ? { ...prev, content: page } : prev));
    } catch { /* keep the stale view */ }
    return true;
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
  const activeWorkingDiffs = workingDiffsSpaceSlug === activeWorkspace?.slug ? workingDiffs : [];
  const activeCloudState = desktopCloudStatus && desktopCloudStatus.spaceSlug === activeWorkspace?.slug
    ? desktopCloudStatus.state
    : null;
  const workspaceContext = workspaceContextStatus({
    desktop,
    state: activeCloudState,
    hasLocalChanges: activeWorkingDiffs.length > 0,
    statusKnown: !desktop || !!activeCloudState,
  });
  // Publish or sync this Space: from the status menu and from Settings.
  const spaceCloudAction = {
    label: !cloudSession
      ? 'Sign in to share…'
      : workspaceContext.kind === 'linked' ? 'Cloud sync…' : 'Publish to Cloud…',
    onSelect: () => {
      if (!cloudSession) {
        navigate('/login');
        return;
      }
      setCloudDialogOpen(true);
    },
  };

  // Every Space opens on Home: what needs you, today's journal (when the
  // Journal module is on) and recently edited pages. Journals is the full
  // stream of days. A shared Space (a team Space, or a local Space published
  // to Cloud) also lists your unsent draft among the changes to review.
  const activeLocal = isLocalSpace(activeWorkspace);
  const journalsAvailable = pluginActions.some((action) => action.id === 'journal:today');
  const sharedSpace = !personal
    || (!!activeCloudState && activeCloudState !== 'unlinked' && activeCloudState !== 'unavailable');
  const onStartPage = activeView === null || activeView.kind === 'home';
  const showJournals = !!activeWorkspace && activeView?.kind === 'journals' && journalsAvailable;
  // The top bar names only what the view below does not: a page's name (its
  // first heading may differ) and Journals (below are only dates). Views with
  // their own title, and the Space (the sidebar names it), add nothing here.
  const [topBarStart, setTopBarStart] = useState<HTMLElement | null>(null);
  const [topBarEnd, setTopBarEnd] = useState<HTMLElement | null>(null);
  const topBarSlots = useMemo(() => ({ start: topBarStart, end: topBarEnd }), [topBarStart, topBarEnd]);
  const crumb = showJournals
    ? t('Journals')
    : activeView?.kind === 'page' && activeView.content ? activeView.content.title : null;
  const showOverview = !!activeWorkspace && onStartPage;
  const mainScrolls = !!activeView && ['review-list', 'review-detail', 'members', 'history', 'links', 'notifications'].includes(activeView.kind);
  const journalPaths = useMemo(() => activeFlatPages.map((page) => page.path), [activeFlatPages]);
  const journalSpaceSlug = activeWorkspace?.slug ?? '';
  const readJournal = useCallback(
    async (path: string) => (await getPage(conceptIdFromPath(path), userBranch, journalSpaceSlug, 'wiki')).body,
    [journalSpaceSlug, userBranch],
  );
  const [homeRefreshTick, setHomeRefreshTick] = useState(0);
  const writeJournal = useCallback(
    async (path: string, body: string, expected: string) => {
      await writePage(conceptIdFromPath(path), body, userBranch, journalSpaceSlug, 'wiki', expected);
    },
    [journalSpaceSlug, userBranch],
  );
  const [recent, setRecent] = useState<{ spaceSlug: string; pages: RecentPage[] } | null>(null);
  const pagesVersion = activeWorkspace ? spacePages[activeWorkspace.id] : undefined;
  useEffect(() => {
    if (!showOverview || !activeLocal || !journalSpaceSlug) return;
    let live = true;
    recentPages(journalSpaceSlug, 8)
      .then((pages) => { if (live) setRecent({ spaceSlug: journalSpaceSlug, pages }); })
      .catch(() => { if (live) setRecent({ spaceSlug: journalSpaceSlug, pages: [] }); });
    return () => { live = false; };
  }, [showOverview, activeLocal, journalSpaceSlug, pagesVersion, reviewRefreshKey, homeRefreshTick]);
  const todayKey = useTodayKey();
  const todayDate = useMemo(() => fromDateKey(todayKey) ?? new Date(), [todayKey]);
  const [homeCounts, setHomeCounts] = useState<HomeCounts & { spaceSlug: string } | null>(null);
  const countTasks = activeFeaturesReady && activeFeatures.modules.tasks;
  const countCards = activeFeaturesReady && activeFeatures.modules.flashcards;
  const journalConfig = activeFeatures.journal;
  useEffect(() => {
    if (!showOverview || !activeLocal || !journalSpaceSlug || !activeFeaturesReady) return;
    let live = true;
    loadHomeCounts({ spaceSlug: journalSpaceSlug, journal: journalConfig, tasks: countTasks, cards: countCards, links: desktop })
      .then((counts) => { if (live) setHomeCounts({ spaceSlug: journalSpaceSlug, ...counts }); })
      .catch(() => { if (live) setHomeCounts(null); });
    return () => { live = false; };
  }, [showOverview, activeLocal, journalSpaceSlug, activeFeaturesReady, journalConfig, countTasks, countCards, desktop, pagesVersion, homeRefreshTick, todayKey]);
  const counts = homeCounts?.spaceSlug === journalSpaceSlug ? homeCounts : null;
  const changesToReview = (desktop && activeWorkspace?.localPath
    ? openAgentChanges.length + (sharedSpace && activeWorkingDiffs.length > 0 ? 1 : 0)
    : reviewCount);
  const homeNeeds: HomeNeed[] = [
    { id: 'tasks', count: counts?.tasks ?? 0, label: counts?.tasks === 1 ? 'task due' : 'tasks due', onOpen: () => handleTabChange('tasks') },
    { id: 'cards', count: counts?.cards ?? 0, label: counts?.cards === 1 ? 'card to review' : 'cards to review', onOpen: () => handleTabChange('flashcards') },
    { id: 'reviews', count: changesToReview, label: changesToReview === 1 ? 'change to review' : 'changes to review', onOpen: () => handleTabChange('reviews') },
    // Broken links have no sidebar entry: they show here only when there are some.
    { id: 'links', count: counts?.links ?? 0, label: counts?.links === 1 ? 'broken link' : 'broken links', onOpen: () => handleTabChange('links') },
  ].filter((need) => need.count > 0);
  const todayEntry = useMemo(
    () => (journalsAvailable ? journalTimeline(journalConfig, journalPaths, todayDate)[0] ?? null : null),
    [journalsAvailable, journalConfig, journalPaths, todayDate],
  );
  /** Days with a journal note, for the Journals calendar. */
  const journalDays = useMemo(() => new Set(
    journalTimeline(journalConfig, journalPaths, todayDate).filter((entry) => !entry.missing).map((entry) => entry.key),
  ), [journalConfig, journalPaths, todayDate]);
  const [journalJump, setJournalJump] = useState<{ key: string; nonce: number } | null>(null);

  // Determine active page/source for panel highlight
  const currentActivePage = activeView?.kind === 'page' ? activeView.slug : null;
  const currentActiveSource = activeView?.kind === 'source' ? activeView.filename : null;
  const activeSourceUrl = activeView?.kind === 'source' && activeView.content
    ? sourceUrlFromDocument(activeView.content.content)
    : null;
  const selectedAgentChange = versionSelection.kind === 'agent'
    ? openAgentChanges.find((change) => change.id === versionSelection.changeId)
    : undefined;
  const activePagePath = activeView?.kind === 'page'
    ? (activeView.path || conceptPath(activeView.slug))
    : null;
  const workingPageDiff = activePagePath ? findDiffForPath(activeWorkingDiffs, activePagePath) : undefined;
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
    ? t('Viewing upstream “main”')
    : versionSelection.kind === 'agent'
      ? t('Viewing {title} changes', { title: selectedAgentChange?.title ?? t('Agent change') })
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
          onImportObsidian={() => { setShowCreate(false); setObsidianImportOpen(true); }}
        />
        <ObsidianImportDialog
          open={obsidianImportOpen}
          existingSlugs={workspaces.map((workspace) => workspace.slug)}
          onClose={() => setObsidianImportOpen(false)}
          onImported={handleObsidianImported}
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
            onSettings={() => openSettings('app')}
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
                activeTab={showJournals ? 'journals' : showOverview ? 'home' : activeTab}
                onTabChange={handleTabChange}
                pages={activeWorkspace ? (spacePages[activeWorkspace.id] || []) : []}
                sources={activeWorkspace ? (spaceSources[activeWorkspace.id] || []) : []}
                activePage={currentActivePage}
                activeSource={currentActiveSource}
                reviewCount={reviewCount}
                isPersonal={personal}
                showReviews={desktop || !personal}
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
                onSettings={() => openSettings(isLocalSpace(activeWorkspace) ? 'space' : 'app')}
                modules={activeModules}
                // The journal's own entry is Journals (the start page), not a command.
                pluginActions={pluginActions.filter((action) => action.id !== 'journal:today')}
                onPluginAction={(id) => { void dispatchPluginAction(id); }}
                journals={journalsAvailable}
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
          <main
            // Lists such as History and Reviews scroll in <main>; the other views scroll in their own pane.
            className={mainScrolls ? 'scroll-pane' : undefined}
            style={{ flex: 1, minWidth: 0, overflow: mainScrolls ? undefined : 'auto', display: 'flex', flexDirection: 'column' }}
          >
            <CommentsProvider
              store={commentStore}
              pageSlug={commentPageSlug}
              openPanel={commentsAfterEdit}
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
                    aria-label={t('Expand sidebar')}
                    title={t('Expand sidebar')}
                    onClick={() => setSidebarLayout((current) => ({ ...current, collapsed: false }))}
                    style={{
                      width: 28, height: 28, padding: 0, marginRight: 4,
                      border: 'none', borderRadius: 6, background: 'transparent',
                      color: C.faint, cursor: 'pointer', flexShrink: 0,
                      display: 'inline-flex', alignItems: 'center', justifyContent: 'center',
                    }}
                    onMouseEnter={(event) => { event.currentTarget.style.background = C.rail; event.currentTarget.style.color = C.ink2; }}
                    onMouseLeave={(event) => { event.currentTarget.style.background = 'transparent'; event.currentTarget.style.color = C.faint; }}
                  >
                    <PanelLeft size={16} />
                  </button>
                )}
                {desktop && activeWorkspace && (
                  <HistoryButtons
                    canGoBack={canGoBack(viewHistory)}
                    canGoForward={canGoForward(viewHistory)}
                    onBack={() => goThroughHistory(-1)}
                    onForward={() => goThroughHistory(1)}
                  />
                )}
                {crumb && (
                  <span style={{ color: C.ink, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{crumb}</span>
                )}
                <span ref={setTopBarStart} style={{ display: 'contents' }} />
              </ContentBreadcrumb>

              {/* Right: actions */}
              {/* Version and Cloud state, comments, then the Agent toggle facing its sidebar. */}
              <ContentHeaderActions>
                <span ref={setTopBarEnd} style={{ display: 'contents' }} />
                {/* Flashcards read the working copy whatever version is picked. */}
                {desktop && activeWorkspace?.localPath && activeView?.kind !== 'flashcards' && (
                  <SpaceStatusMenu
                    context={workspaceContext}
                    agentChanges={openAgentChanges}
                    selection={versionSelection}
                    onSelect={(selection) => {
                      void (async () => {
                        if (editingPage && !(await handleDoneEditing())) return;
                        setVersionSelection(selection);
                      })();
                    }}
                    onSeeReviews={() => handleTabChange('reviews')}
                    versions={!isHtmlPage}
                    cloudAction={spaceCloudAction}
                  />
                )}

                {showJournals && (
                  <JournalCalendar
                    days={journalDays}
                    today={todayKey}
                    onGoTo={(key) => setJournalJump((previous) => ({ key, nonce: (previous?.nonce ?? 0) + 1 }))}
                  />
                )}

                {/* Comments on the open page */}
                {!isHtmlPage && versionSelection.kind === 'working' && activeTab === 'wiki' && (activeView?.kind === 'page' || activeView?.kind === 'source') && (
                  <CommentsHeaderToggle style={headerBtnStyle} />
                )}
                {/* While editing, comments wait: the same button saves, returns to reading and opens them. */}
                {editingPage && canEditInline && commentStore && (
                  <button
                    type="button"
                    aria-label={t('Comments')}
                    title={t('Back to reading and show comments')}
                    onClick={() => { void handleDoneEditing({ openComments: true }); }}
                    className="hover:bg-bg-hover"
                    style={{ ...headerBtnStyle, width: 30, height: 28, justifyContent: 'center', padding: 0, color: C.ink2, background: undefined }}
                  >
                    <MessageSquare size={15} />
                  </button>
                )}

                {/* Opens the Agent sidebar; once open, the sidebar's own header closes it from the same corner. */}
                {desktop && activeWorkspace?.localPath && !agentPanelOpen && (
                  <button
                    type="button"
                    aria-label={t('Agent')}
                    onClick={() => setAgentPanelOpen(true)}
                    className="hover:bg-bg-hover"
                    style={{
                      ...headerBtnStyle,
                      width: 28, height: 28, justifyContent: 'center', padding: 0, marginRight: -18,
                      color: C.muted, background: undefined,
                    }}
                    title={t('Open the Agent sidebar')}
                  >
                    <Bot size={16} />
                  </button>
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
                cloudClient && cloudSession ? (
                  <CloudNotificationsPage
                    client={cloudClient}
                    session={cloudSession}
                    onSignOut={handleLogout}
                    embedded
                    onUnreadChange={setNotifUnread}
                  />
                ) : <NotificationsPage onUnreadChange={setNotifUnread} />

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
                    onDraftChanged={() => {
                      if (!activeWorkspace || activeWorkspace.slug !== activeView.workspaceSlug) return;
                      void loadSpacePages(activeWorkspace);
                      void loadSpaceSources(activeWorkspace);
                    }}
                    onReviewsChanged={() => setReviewRefreshKey((key) => key + 1)}
                    onAgentChangeClosed={(changeId) => markAgentChangeClosed(activeView.workspaceSlug, changeId)}
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
                  key={`${activeWorkspace.slug}:${checkpointTick}`}
                  workspaceSlug={activeWorkspace.slug}
                  local={desktop && !!activeWorkspace.localPath}
                />

              /* Link diagnostics */
              ) : activeView?.kind === 'tasks' && activeWorkspace ? (
                <div className="scroll-pane" style={{ position: 'absolute', inset: 0, padding: '36px 56px 56px' }}>
                  <TasksView
                    workspaceSlug={activeWorkspace.slug}
                    journal={activeFeatures.journal}
                    onOpenPage={(path, line) => {
                      void selectPage(activeWorkspace, conceptIdFromPath(path), path, true, undefined, line);
                    }}
                    onToggleTask={toggleTaskInPage}
                    onSetTaskStatus={setTaskStatusInPage}
                    onAddTask={addTaskToToday}
                  />
                </div>
              ) : activeView?.kind === 'flashcards' && activeWorkspace ? (
                <div className="scroll-pane" style={{ position: 'absolute', inset: 0, padding: '36px 56px 56px' }}>
                  <ObsidianReaderContext.Provider value={readerCallbacks}>
                    <TopBarSlots.Provider value={topBarSlots}>
                    <FlashcardsView
                      key={activeWorkspace.slug}
                      workspaceSlug={activeWorkspace.slug}
                      deck={activeView.deck ?? null}
                      studying={!!activeView.studying}
                      onNavigate={({ deck, studying }) => setActiveView({ workspaceSlug: activeWorkspace.slug, ...flashcardsStep(deck, studying) })}
                      onLeaveStudy={() => {
                        // A session is its own step: ending it steps back to where it started.
                        const current = viewHistoryRef.current.entries[viewHistoryRef.current.index];
                        if (desktop && current?.view.kind === 'flashcards' && current.view.studying) goThroughHistory(-1);
                        else setActiveView({ workspaceSlug: activeWorkspace.slug, ...flashcardsStep(activeView.deck) });
                      }}
                      onOpenSource={(path) => { void selectPage(activeWorkspace, conceptIdFromPath(path), path); }}
                    />
                    </TopBarSlots.Provider>
                  </ObsidianReaderContext.Provider>
                </div>
              ) : activeView?.kind === 'links' && activeWorkspace ? (
                <LinksView
                  workspaceSlug={activeWorkspace.slug}
                  onOpenPage={(path) => selectPage(activeWorkspace, conceptIdFromPath(path), path)}
                />

              /* Source view */
              ) : activeView?.kind === 'source' && activeView.content ? (
                <div>
                  <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 8, marginBottom: 16 }}>
                    <span style={{
                      padding: '2px 10px', fontSize: 11, borderRadius: 12,
                      background: C.blueSoft, color: C.blue, fontWeight: 500,
                    }}>
                      {t('Source File')}
                    </span>
                    {activeSourceUrl && (
                      <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        onClick={() => { void openExternalUrl(activeSourceUrl); }}
                      >
                        <ExternalLink className="size-4" />
                        {t('View original')}
                      </Button>
                    )}
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
                isHtmlPage ? (
                  <HtmlFileReader key={(activeWorkspace?.slug ?? '') + ':' + activeView.content.path + ':' + activeView.content.body}
                    path={activeView.content.path} source={activeView.content.body} readAsset={htmlAssetReader} />
                ) : isDrawingPage && !editingPage && versionSelection.kind === 'working' ? (
                  <ObsidianReaderContext.Provider value={readerCallbacks}>
                    <ExcalidrawReader
                      key={(activeWorkspace?.slug ?? '') + ':' + activeView.content.path}
                      title={(activeView.content.path.split('/').at(-1) ?? '').replace(/\.excalidraw(\.md)?$|\.md$/i, '')}
                      body={activeView.content.body}
                    />
                  </ObsidianReaderContext.Provider>
                ) : editingPage ? (
                  <PageEditor
                    ref={pageEditorRef}
                    key={activeView.path ?? activeView.slug}
                    initialBody={activeView.content.body}
                    initialCursor={editStart && editStart.path === (activeView.path ?? '') ? editStart.cursor : undefined}
                    initialCursorTop={editStart && editStart.path === (activeView.path ?? '') ? editStart.top : undefined}
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
                      const deleted = activeView.path ?? conceptPath(activeView.slug);
                      if (activeWorkspace) setViewHistory((history) => forgetPath(history, activeWorkspace.slug, deleted));
                      setEditingPage(false);
                      setActiveView(null);
                      navigate('/', { replace: true });
                      if (activeWorkspace) loadSpacePages(activeWorkspace);
                    }}
                    onDone={() => { void handleDoneEditing(); }}
                    header={versionSelection.kind === 'working'
                      ? <PageByline name={activeView.content.edited_by} editedAt={activeView.content.edited_at} />
                      : undefined}
                    onWikilink={(target) => { void openWikilink(target); }}
                    loadImage={canReadAssets ? loadAssetUrl : undefined}
                    getPageSlugs={() => (activeWorkspace ? collectSlugs(spacePages[activeWorkspace.id] || []) : [])}
                  />
                ) : (
                  // Fill the content box edge-to-edge: the doc scrolls on the left
                  // (left-anchored, same left edge as every other view), the comment
                  // panel is flush to the right edge and full height with its own
                  // scroll. Opening it shrinks the doc from the right only.
                  <ObsidianReaderContext.Provider value={readerCallbacks}>
                  <PageReader
                    key={(activeWorkspace?.slug ?? '') + ':' + (activeView.path ?? activeView.slug) + ':' + versionSelection.kind}
                    articleRef={articleRef}
                    body={obsidianToReaderMarkdown(renderBody(viewedPageBody))}
                    lineage={pageLineage(
                      viewedPageBody,
                      versionSelection.kind === 'working' ? activeView.content.provenance : undefined,
                    )}
                    onOpenSource={(path) => {
                      if (activeWorkspace) void selectSource(activeWorkspace, sourceFilename(path));
                    }}
                    loadSource={versionSelection.kind === 'working' && activeWorkspace
                      ? (path) => getSource(activeWorkspace.slug, sourceFilename(path))
                      : undefined}
                    markdownComponents={READER_COMPONENTS}
                    byline={versionSelection.kind === 'working' ? {
                      name: activeView.content.edited_by,
                      editedAt: activeView.content.edited_at,
                    } : undefined}
                    initialScrollTop={readerScroll && readerScroll.path === (activeView.path ?? '') ? readerScroll.top : undefined}
                    targetLine={readerTargetLine && readerTargetLine.path === (activeView.path ?? '') ? {
                      line: Math.max(1, readerTargetLine.line - renderedLineOffset(viewedPageBody, renderBody(viewedPageBody)) + 1),
                      seq: readerTargetLine.seq,
                    } : undefined}
                    readOnlyLabel={versionSelection.kind !== 'working' ? readonlyVersionLabel : undefined}
                    readOnlyDotColor={versionSelection.kind === 'upstream' ? C.purple : C.blue}
                    missingMessage={viewedPageMissing
                      ? t('This page does not exist in the selected version.')
                      : undefined}
                    aside={<CommentsPanel />}
                    onEditAt={canEditInline ? (click, top) => {
                      const body = splitSystemFrontmatter(activeView.content!.body).body;
                      const cursor = click ? sourceOffsetForClick(body, click) ?? body.length : body.length;
                      setEditStart({ path: activeView.path ?? '', cursor, top: Math.max(0, top) });
                      setCommentsAfterEdit(false);
                      setEditingPage(true);
                    } : undefined}
                  />
                  </ObsidianReaderContext.Provider>
                )

              /* Discover */
              ) : location.pathname === '/discover' ? (
                <DiscoverView />

              /* Journals: the stream of days */
              ) : showJournals && activeWorkspace ? (
                <div className="scroll-pane" style={{ position: 'absolute', inset: 0 }}>
                  <div className="page-column">
                  <JournalsView
                    key={activeWorkspace.slug}
                    config={activeFeatures.journal}
                    paths={journalPaths}
                    jumpTo={journalJump}
                    read={readJournal}
                    write={writeJournal}
                    createToday={async () => (await ensureTodayEntry(activeWorkspace)).path}
                    onOpenPage={(path) => { void selectPage(activeWorkspace, conceptIdFromPath(path), path); }}
                    onWikilink={(target) => { void openWikilink(target); }}
                    loadAsset={canReadAssets ? loadAssetFrom : undefined}
                    getPageSlugs={() => collectSlugs(spacePages[activeWorkspace.id] || [])}
                  />
                  </div>
                </div>

              /* Start page: Home */
              ) : showOverview && activeWorkspace ? (
                <div className="scroll-pane" style={{ position: 'absolute', inset: 0 }}>
                  <div className="page-column">
                    <SpaceHome
                      needs={homeNeeds}
                      today={todayEntry && (
                        <JournalDay
                          key={activeWorkspace.slug + ':' + todayEntry.key}
                          entry={todayEntry}
                          today
                          read={readJournal}
                          write={writeJournal}
                          createToday={async () => (await ensureTodayEntry(activeWorkspace)).path}
                          onOpenPage={(path) => { void selectPage(activeWorkspace, conceptIdFromPath(path), path); }}
                          onWikilink={(target) => { void openWikilink(target); }}
                          onFinishEdit={() => setHomeRefreshTick((tick) => tick + 1)}
                          loadAsset={canReadAssets ? loadAssetFrom : undefined}
                          getPageSlugs={() => collectSlugs(spacePages[activeWorkspace.id] || [])}
                        />
                      )}
                      recent={activeLocal ? (recent?.spaceSlug === activeWorkspace.slug ? recent.pages : []) : null}
                      onOpenPage={(path) => { void selectPage(activeWorkspace, conceptIdFromPath(path), path); }}
                    />
                  </div>
                </div>
              ) : null}
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
                aria-label="Resize agent panel"
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
              {/* Native Agent sessions live in the desktop backend: hiding or
                  remounting this panel never stops or re-sends a task. */}
              <AgentPanel
                key={activeWorkspace.id}
                spaceSlug={activeWorkspace.slug}
                defaultAgent={clientSettings.defaultAgent}
                agentModels={clientSettings.agentModels}
                onAgentUsed={handleAgentUsed}
                openRequest={agentOpenRequest}
                onOpenRequestHandled={(requestId) => {
                  setAgentOpenRequest((current) => (
                    current?.requestId === requestId ? null : current
                  ));
                }}
                onClose={() => setAgentPanelOpen(false)}
                onOpenChanges={(changeId) => openReviewDetail({ kind: 'local-agent', changeId })}
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
        onImportObsidian={desktop ? () => { setShowCreate(false); setObsidianImportOpen(true); } : undefined}
      />

      {/* New page */}
      <Dialog open={!!showNewPage} onOpenChange={(open) => !open && setShowNewPage(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader><DialogTitle>{t('New Page')}</DialogTitle></DialogHeader>
          <form onSubmit={handleCreatePage} className="space-y-4 mt-2">
            <div>
              <label className="mb-1.5 block text-sm text-text-secondary">{t('Title')}</label>
              <Input value={newName} onChange={(e) => setNewName(e.target.value)} placeholder={t('e.g. Meeting Notes')} autoFocus />
            </div>
            <div>
              <label htmlFor="new-page-template" className="mb-1.5 block text-sm text-text-secondary">{t('Template')}</label>
              <Select value={newPageTemplate || 'blank'} onValueChange={(value) => setNewPageTemplate(value === 'blank' ? '' : value)}>
                <SelectTrigger id="new-page-template" className="h-9 w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="blank">{t('Blank page')}</SelectItem>
                  {newPageTemplates.map((path) => (
                    <SelectItem key={path} value={`page:${path}`}>{path.replace(/\.md$/i, '')}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
              {newPageTemplates.length === 0 && showNewPage && (
                <p className="mt-1.5 text-xs text-text-secondary">
                  No templates yet. Pages in the <code>{(spaceFeatures[showNewPage.id] ?? DEFAULT_SPACE_FEATURES).templates.folder || 'Templates'}</code> folder appear here.
                </p>
              )}
            </div>
            <div className="flex justify-end gap-2 pt-2">
              <Button variant="outline" type="button" onClick={() => setShowNewPage(null)}>{t('Cancel')}</Button>
              <Button type="submit" disabled={!newName.trim()}>{t('Create')}</Button>
            </div>
          </form>
        </DialogContent>
      </Dialog>

      {/* Rename workspace */}
      <Dialog open={!!showRename} onOpenChange={(open) => !open && setShowRename(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader><DialogTitle>{t('Rename Space')}</DialogTitle></DialogHeader>
          <form onSubmit={handleRename} className="space-y-4 mt-2">
            <div>
              <label className="mb-1.5 block text-sm text-text-secondary">{t('Name')}</label>
              <Input value={renameValue} onChange={(e) => setRenameValue(e.target.value)} autoFocus />
            </div>
            <div className="flex justify-end gap-2 pt-2">
              <Button variant="outline" type="button" onClick={() => setShowRename(null)}>{t('Cancel')}</Button>
              <Button type="submit" disabled={!renameValue.trim()}>{t('Rename')}</Button>
            </div>
          </form>
        </DialogContent>
      </Dialog>

      {/* New folder */}
      <Dialog open={!!showNewFolder} onOpenChange={(open) => !open && setShowNewFolder(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader><DialogTitle>{t('New Folder')}</DialogTitle></DialogHeader>
          <form onSubmit={handleCreateFolder} className="space-y-4 mt-2">
            <div>
              <label className="mb-1.5 block text-sm text-text-secondary">{t('Name')}</label>
              <Input value={newName} onChange={(e) => setNewName(e.target.value)} placeholder={t('e.g. Research, Projects')} autoFocus />
            </div>
            <div className="flex justify-end gap-2 pt-2">
              <Button variant="outline" type="button" onClick={() => setShowNewFolder(null)}>{t('Cancel')}</Button>
              <Button type="submit" disabled={!newName.trim()}>{t('Create')}</Button>
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
          <DialogHeader><DialogTitle>{pathOp?.isFolder ? t('Rename Folder') : t('Rename Page')}</DialogTitle></DialogHeader>
          <div className="space-y-4 mt-2">
            <Input
              value={pathOp?.kind === 'rename' ? pathOp.value : ''}
              onChange={(e) => setPathOp((p) => (p?.kind === 'rename' ? { ...p, value: e.target.value } : p))}
              onKeyDown={(e) => isSubmitEnter(e) && handlePathOp()}
              autoFocus
            />
            <p className="text-xs text-text-secondary">
              Renames the {pathOp?.isFolder ? 'folder path' : 'page slug'} in your draft; links to the old name will need updating.
            </p>
            <div className="flex justify-end gap-2">
              <Button variant="outline" onClick={() => setPathOp(null)}>{t('Cancel')}</Button>
              <Button onClick={handlePathOp}>{t('Rename')}</Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>

      {/* Delete page/folder confirm */}
      <Dialog open={pathOp?.kind === 'delete'} onOpenChange={(open) => !open && setPathOp(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader><DialogTitle>{pathOp?.isFolder ? t('Delete Folder') : t('Delete Page')}</DialogTitle></DialogHeader>
          <div className="space-y-4 mt-2">
            <p className="text-sm text-text-secondary">
              Delete <strong>{pathOp?.title}</strong>
              {pathOp?.isFolder ? ' and everything inside it' : ''}?
              {desktop
                ? ' The deletion remains a local Git change until you keep it from Reviews.'
                : ' The change lands on the shared wiki when your submission is merged.'}
            </p>
            <div className="flex justify-end gap-2">
              <Button variant="outline" onClick={() => setPathOp(null)}>{t('Cancel')}</Button>
              <Button variant="destructive" onClick={handlePathOp}>{t('Delete')}</Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>

      <ObsidianImportDialog
        open={obsidianImportOpen}
        existingSlugs={workspaces.map((workspace) => workspace.slug)}
        onClose={() => setObsidianImportOpen(false)}
        onImported={handleObsidianImported}
      />
      <SettingsDialog
        open={settingsOpen}
        onOpenChange={setSettingsOpen}
        group={settingsGroup}
        space={activeWorkspace && isLocalSpace(activeWorkspace) ? {
          name: activeWorkspace.name,
          features: activeFeatures,
          plugins: BUILTIN_PLUGINS,
          ready: activeFeaturesReady,
          configurationError: featureLoadErrors[activeWorkspace.id],
          place: { label: workspaceContext.label, detail: workspaceContext.detail, action: desktop ? spaceCloudAction : null },
          onReload: async () => { await loadSpaceFeatures(activeWorkspace); },
          onSave: (features) => saveModules(features, preferences),
        } : null}
        preferences={preferences}
        onPreferencesChange={savePreferences}
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
          hasLocalChanges={activeWorkingDiffs.length > 0}
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
  onImportObsidian,
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
  onImportObsidian?: () => void;
}) {
  const choice = (
    <div className="grid gap-2 mt-2">
      <SpaceChoice icon={<HardDrive size={19} />} title="Create local Space" description="Start a new OKF knowledge space on this Mac." onClick={() => onModeChange('local')} />
      {onImportObsidian && (
        <SpaceChoice icon={<BookMarked size={19} />} title="Import Obsidian vault" description="Copy a vault into a new Space with a compatibility report. The vault is not changed." onClick={onImportObsidian} />
      )}
      <SpaceChoice icon={<FolderInput size={19} />} title="Open a folder in place" description="Use an existing Markdown folder or Git repository directly. CoWiki adds folder indexes and page properties to it." onClick={() => onModeChange('import')} />
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
        <span className="flex items-center gap-2 text-sm font-semibold">{title}{badge && <span className="rounded-full bg-muted px-2 py-0.5 text-caption font-medium text-muted-foreground">{badge}</span>}</span>
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
        <p style={{ margin: '0 auto 34px', maxWidth: 490, color: C.ink2, fontSize: 16, lineHeight: 1.65 }}>
          Create a new local knowledge space or import an existing folder.<br />
          Cloud can be enabled later without changing its local-first foundation.
        </p>
        <button
          type="button"
          onClick={onOpenFolder}
          disabled={creating}
          style={{
            minWidth: 220, height: 58, padding: '0 24px', border: 'none', borderRadius: 12,
            display: 'inline-flex', alignItems: 'center', justifyContent: 'center', gap: 18,
            background: C.accent, color: C.onAccent, fontSize: 16, fontWeight: 700,
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
  const { t } = useI18n();
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
      <h1 className="view-title mb-1">{t('Discover')}</h1>
      <p style={{ color: C.muted, fontSize: 13, marginBottom: 32 }}>{t('Public knowledge spaces you can browse and join.')}</p>
      {loading ? (
        <div style={{ padding: '32px 0', textAlign: 'center', color: C.muted, fontSize: 13 }}>{t('Loading…')}</div>
      ) : spaces.length === 0 ? (
        <div style={{ padding: '48px 0', textAlign: 'center' }}>
          <Compass size={24} style={{ margin: '0 auto 12px', display: 'block', color: C.faint }} />
          <p style={{ color: C.muted, fontSize: 13 }}>{t('No public spaces yet.')}</p>
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
                  fontSize: 16, fontWeight: 500, color: C.ink2,
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
                {joining === ws.slug ? t('Joining...') : t('Join')}
              </button>
            </div>
          ))}
        </div>
      )}
    </>
  );
}
