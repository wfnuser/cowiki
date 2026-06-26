import { useState, useEffect, useCallback, useRef } from 'react';
import { useNavigate, useLocation } from 'react-router-dom';
import {
  Compass,
  Upload, Zap, ArrowUpRight, MoreHorizontal, RefreshCw,
  CheckCircle2, Clock, Pencil,
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
  deleteWorkspace,
  listPublicWorkspaces, joinWorkspace,
  listSources, getSource, listReviews, syncBranch, renamePath, deletePath,
  type Workspace, type PageMeta, type PageFull, type SourceItem, type SourceContent,
} from '../api';
import { AddSourceDialog } from '@/components/AddSourceDialog';
import { SettingsDialog } from '../components/SettingsDialog';
import { getStoredAuth, clearAuth } from '../auth';
import { SpaceRail } from '../components/layout/SpaceRail';
import { SpacePanel, type NavTab } from '../components/layout/SpacePanel';
type ContentDir = 'wiki' | 'entities' | 'concepts';
import { ReviewList } from '../components/review/ReviewList';
import { ReviewDetail } from '../components/review/ReviewDetail';
import { MembersView } from '../components/views/MembersView';
import { InviteDialog } from '../components/InviteDialog';
import { PageEditor } from '../components/PageEditor';
import { PageByline } from '../components/PageByline';
import { TransferDialog } from '../components/TransferDialog';
import { NotificationsPage } from '../components/notifications/NotificationsPage';
import { notificationUnreadCount } from '../api';
import { CommentsProvider, CommentsPanel, CommentsHeaderToggle, commentMarkdownComponents } from '../components/PageCommentsLayer';
import { C } from '@/lib/design';

type ActiveView =
  | { kind: 'page'; slug: string; path?: string; content: PageFull | null }
  | { kind: 'source'; filename: string; content: SourceContent | null }
  | { kind: 'review-list'; workspaceSlug: string }
  | { kind: 'review-detail'; workspaceSlug: string; submissionId: string }
  | { kind: 'members'; workspaceSlug: string }
  | { kind: 'activity'; workspaceSlug: string }
  | { kind: 'notifications' }
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
  const [notifUnread, setNotifUnread] = useState(0);

  // Cross-space unread badge for the rail; refreshed when opening the inbox.
  useEffect(() => {
    notificationUnreadCount().then(setNotifUnread).catch(() => {});
  }, []);
  const articleRef = useRef<HTMLElement>(null);
  const [reviewRefreshKey, setReviewRefreshKey] = useState(0);
  const [activeTab, setActiveTab] = useState<NavTab>('wiki');
  const [reviewCount, setReviewCount] = useState(0);

  // Modals
  const [showCreate, setShowCreate] = useState(false);
  const [showNewPage, setShowNewPage] = useState<Workspace | null>(null);
  const [showNewFolder, setShowNewFolder] = useState<Workspace | null>(null);
  const [newPageFolder, setNewPageFolder] = useState<string | null>(null);
  const [newFolderParent, setNewFolderParent] = useState<string | null>(null);
  const [newPageDir, setNewPageDir] = useState<ContentDir>('wiki');
  const [newFolderDir, setNewFolderDir] = useState<ContentDir>('wiki');
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
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [syncing, setSyncing] = useState(false);
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
        // Find the page in the tree to get its path
        const findPath = (items: PageMeta[], targetSlug: string): string | undefined => {
          for (const p of items) {
            if (p.kind !== 'folder' && p.slug === targetSlug) return p.path;
            if (p.children) {
              const found = findPath(p.children, targetSlug);
              if (found) return found;
            }
          }
          return undefined;
        };
        const pagePath = findPath(spacePages[targetWs.id] || [], pageSlug);
        const dir = pagePath?.split('/')[0] || 'wiki';
        try {
          const page = await getPage(pageSlug, branch, targetWs.slug, dir);
          setActiveView({ kind: 'page', slug: pageSlug, path: pagePath, content: page });
        } catch {
          // Page not found, just show home
        }
      }
    } else if (personal) {
      const personalPages = await listPages(userBranch, personal.slug, 'all');
      const firstPage = personalPages.find((p) => p.kind === 'page');
      if (firstPage) {
        setActiveView({ kind: 'page', slug: firstPage.slug, path: firstPage.path, content: null });
        navigate(`/${personal.slug}/${firstPage.slug}`, { replace: true });
        const firstDir = firstPage.path?.split('/')[0] || 'wiki';
        try {
          const page = await getPage(firstPage.slug, userBranch, personal.slug, firstDir);
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

  /** Walk the page tree and recursively merge draft pages into main pages by path.
   *  When both trees have a folder at the same path, their children are merged
   *  recursively so that draft-only pages under existing section nodes (wiki,
   *  entities, concepts) are not silently dropped. */
  function mergePageTrees(main: PageMeta[], draft: PageMeta[]): PageMeta[] {
    const merged: PageMeta[] = [...main];
    const pathToIndex = new Map(merged.map((p, i) => [p.path, i] as const));
    for (const dp of draft) {
      const idx = pathToIndex.get(dp.path);
      if (idx === undefined) {
        // Draft-only node — add to merged
        pathToIndex.set(dp.path, merged.length);
        merged.push(dp);
      } else if (merged[idx].kind === 'folder' && dp.kind === 'folder') {
        // Both are folders — recursively merge their children
        merged[idx] = {
          ...merged[idx],
          children: mergePageTrees(merged[idx].children || [], dp.children || []),
        };
      }
      // else: main has a page at this path — main wins, skip draft
    }
    return merged;
  }

  // Load pages for a space
  const loadSpacePages = async (ws: Workspace) => {
    if (ws.visibility === 'private') {
      const pages = await listPages(userBranch, ws.slug, 'all');
      setSpacePages((prev) => ({ ...prev, [ws.id]: pages }));
    } else {
      const [mainPages, draftPages] = await Promise.all([
        listPages('main', ws.slug, 'all'),
        listPages(userBranch, ws.slug, 'all').catch(() => [] as PageMeta[]),
      ]);
      const merged = mergePageTrees(mainPages, draftPages);
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
      selectPage(ws, first.slug, first.path);
    } else {
      setActiveView(null);
    }
  };

  // Select a page
  const selectPage = async (ws: Workspace, slug: string, path?: string) => {
    setActiveWorkspace(ws);
    setActiveTab('wiki');
    setEditingPage(false);
    setActiveView({ kind: 'page', slug, path, content: null });
    navigate(`/${ws.slug}/${slug}`, { replace: true });

    const dir = path?.split('/')[0] || 'wiki';
    const setContent = (content: PageFull | null) =>
      setActiveView(prev => prev?.kind === 'page' ? { ...prev, content } : prev);

    if (ws.visibility === 'private') {
      try {
        const page = await getPage(slug, userBranch, ws.slug, dir);
        setContent(page);
      } catch {
        setContent(null);
        setMessage({ text: `Page "${slug}" not found`, type: 'error' });
      }
    } else {
      try {
        const page = await getPage(slug, userBranch, ws.slug, dir);
        setContent(page);
      } catch {
        try {
          const page = await getPage(slug, 'main', ws.slug, dir);
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
    // Strip dir prefix from folder path if creating inside a folder
    const folderRelative = newPageFolder
      ? newPageFolder.slice(newPageDir.length + 1) // e.g. "wiki/some/path" → "some/path"
      : null;
    const slug = folderRelative ? `${folderRelative}/${baseSlug}` : baseSlug;
    const body = `---\ntitle: "${newName.trim()}"\nsummary: ""\nkind: concept\n---\n\n`;
    try {
      await writePage(slug, body, userBranch, ws.slug, newPageDir);
      const title = newName.trim();
      setShowNewPage(null);
      setNewName('');
      setNewPageFolder(null);
      setNewPageDir('wiki');
      await loadSpacePages(ws);
      const newPath = `${newPageDir}/${slug}`;
      setActiveView({ kind: 'page', slug, path: newPath, content: { slug, path: newPath, title, summary: '', body, branch: userBranch, kind: 'page', children: [] } });
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
      await createFolder(newName.trim(), userBranch, newFolderParent || newFolderDir, ws.slug);
      setShowNewFolder(null);
      setNewName('');
      setNewFolderParent(null);
      setNewFolderDir('wiki');
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
      // Flatten page tree to paths, skipping folders
      const flatten = (items: PageMeta[]): string[] => {
        const result: string[] = [];
        for (const p of items) {
          if (p.kind === 'folder' && p.children && p.children.length > 0) {
            result.push(...flatten(p.children));
          } else if (p.kind !== 'folder') {
            result.push(p.path);
          }
        }
        return result;
      };
      const paths = flatten(pages);
      const personal = isPersonalSpace(ws);
      await submit(userBranch, paths, personal, ws.slug);
      setMessage({ text: personal ? 'Committed.' : 'Submitted for review.', type: 'success' });
    } catch {
      setMessage({ text: 'Submit failed', type: 'error' });
    } finally {
      setSubmitting(false);
    }
  };

  // Sync the user's draft branch with main (rebase). Conflicts are surfaced for now;
  // proper in-app resolution is a follow-up.
  const handleSync = async () => {
    if (!activeWorkspace) return;
    setSyncing(true);
    setMessage(null);
    try {
      const res = await syncBranch(activeWorkspace.slug, userBranch);
      if (res.status === 'conflict') {
        setMessage({ text: `Conflict with main — resolve: ${res.conflicts.join(', ')}`, type: 'error' });
      } else {
        setMessage({
          text: res.status === 'updated' ? 'Synced with main.' : 'Already up to date.',
          type: 'success',
        });
        if (res.status === 'updated') loadWorkspaces();
      }
    } catch (e) {
      setMessage({ text: e instanceof Error ? e.message : 'Sync failed', type: 'error' });
    } finally {
      setSyncing(false);
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
          selectPage(activeWorkspace, first.slug, first.path);
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
  const commentPageSlug = commentsActive && pageView ? pageView.slug : '';
  const commentSource = commentsActive && pageView?.content ? renderBody(pageView.content.body) : '';

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
          .replace(/\s+/g, '-');
        if (!slugified) { setMessage({ text: 'Name cannot be empty', type: 'error' }); return; }
        const parent = pathOp.path.slice(0, pathOp.path.lastIndexOf('/'));
        const to = pathOp.isFolder ? `${parent}/${slugified}` : `${parent}/${slugified}.md`;
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
        const dir = activeView.path?.split('/')[0] || 'wiki';
        const viewPath = `${dir}/${activeView.slug}.md`;
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

  // Save an in-page edit to the user's draft branch, then refresh the page + tree.
  const handleSavePage = async (body: string) => {
    if (!activeWorkspace || activeView?.kind !== 'page') return;
    const ws = activeWorkspace;
    const slug = activeView.slug;
    const dir = activeView.path?.split('/')[0] || 'wiki';
    await writePage(slug, body, userBranch, ws.slug, dir);
    setEditingPage(false);
    setMessage({ text: 'Saved to your draft.', type: 'success' });
    try {
      const page = await getPage(slug, userBranch, ws.slug, dir);
      setActiveView((prev) => (prev?.kind === 'page' ? { ...prev, content: page } : prev));
    } catch { /* keep the stale view; tree reload below still runs */ }
    loadSpacePages(ws);
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
            notifUnread={notifUnread}
            onShowNotifications={() => setActiveView({ kind: 'notifications' })}
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
            onSelectPage={(slug, path) => activeWorkspace && selectPage(activeWorkspace, slug, path)}
            onSelectSource={(filename) => activeWorkspace && selectSource(activeWorkspace, filename)}
            onNewPage={(dir?: ContentDir) => { if (activeWorkspace) { setShowNewPage(activeWorkspace); setNewName(''); setNewPageFolder(null); setNewPageDir(dir || 'wiki'); } }}
            onNewFolder={(dir?: ContentDir) => { if (activeWorkspace) { setShowNewFolder(activeWorkspace); setNewName(''); setNewFolderParent(null); setNewFolderDir(dir || 'wiki'); } }}
            onAddPageInFolder={(folderPath, dir) => { if (activeWorkspace) { setShowNewPage(activeWorkspace); setNewName(''); setNewPageFolder(folderPath); setNewPageDir(dir); } }}
            onAddFolderInFolder={(parentPath, dir) => { if (activeWorkspace) { setShowNewFolder(activeWorkspace); setNewName(''); setNewFolderParent(parentPath); setNewFolderDir(dir); } }}
            onRenamePath={(path, isFolder, title) => setPathOp({ kind: 'rename', path, isFolder, title, value: title })}
            onDeletePath={(path, isFolder, title) => setPathOp({ kind: 'delete', path, isFolder, title })}
            onShowIngest={() => setShowIngest(true)}
            onCompile={handleCompile}
            onSettings={() => setSettingsOpen(true)}
          />

          {/* Main Content Area */}
          <main style={{ flex: 1, overflow: 'auto', display: 'flex', flexDirection: 'column' }}>
            <CommentsProvider
              workspaceSlug={activeWorkspace?.slug ?? ''}
              pageSlug={commentPageSlug}
              source={commentSource}
              articleRef={articleRef}
              currentUserId={auth?.id}
            >
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
                {activeView?.kind === 'notifications' && (
                  <span style={{ color: C.ink }}>Notifications</span>
                )}
              </div>

              {/* Right: actions */}
              <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>

                {/* Sync draft branch with main */}
                {activeWorkspace && (
                  <button
                    onClick={handleSync}
                    disabled={syncing}
                    style={{ ...headerBtnStyle, opacity: syncing ? 0.4 : 1 }}
                    title="Sync your draft with the latest main"
                  >
                    <RefreshCw size={13} className={syncing ? 'animate-spin' : ''} /> Sync
                  </button>
                )}

                {/* Wiki-specific actions */}
                {activeTab === 'wiki' && (activeView?.kind === 'page' || activeView?.kind === 'source') && (
                  <>
                    {activeView?.kind === 'page' && activeView.content && !editingPage && (
                      <button
                        onClick={() => setEditingPage(true)}
                        style={headerBtnStyle}
                        title="Edit this page (saved to your draft branch)"
                      >
                        <Pencil size={13} /> Edit
                      </button>
                    )}
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
                    <CommentsHeaderToggle style={{ ...headerBtnStyle, marginLeft: 2 }} />
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
              onDone={handleIngestDone}
            />

            {/* Content */}
            <div style={{ flex: 1, padding: '36px 56px 56px', position: 'relative' }}>
              {/* Notifications (cross-space inbox) */}
              {activeView?.kind === 'notifications' ? (
                <NotificationsPage onUnreadChange={setNotifUnread} />

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
                <MembersView
                  workspaceSlug={activeWorkspace.slug}
                  canManage={isOwner || activeWorkspace.role === 'manager'}
                  currentUserRole={activeWorkspace.role}
                  currentUserId={auth?.id || ''}
                  isOwner={isOwner}
                  onInvite={() => setShowInviteDialog(activeWorkspace)}
                  onTransfer={() => setShowTransferDialog(activeWorkspace)}
                />

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
                      background: C.blueSoft, color: C.blue, fontWeight: 500,
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
                            style={{ fontSize: 12, color: C.blue, background: 'none', border: 'none', cursor: 'pointer', textDecoration: 'underline' }}
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
                editingPage ? (
                  <PageEditor
                    key={activeView.slug}
                    initialBody={activeView.content.body}
                    stripFrontmatter={renderBody}
                    onSave={handleSavePage}
                    onCancel={() => setEditingPage(false)}
                  />
                ) : (
                  // Fill the content box edge-to-edge: the doc scrolls on the left
                  // (left-anchored, same left edge as every other view), the comment
                  // panel is flush to the right edge and full height with its own
                  // scroll. Opening it shrinks the doc from the right only.
                  <div style={{ position: 'absolute', inset: 0, display: 'flex', alignItems: 'stretch' }}>
                    <article ref={articleRef} className="prose" style={{ flex: 1, minWidth: 0, overflow: 'auto', padding: '36px 48px 56px 56px' }}>
                      <PageByline name={activeView.content.edited_by} editedAt={activeView.content.edited_at} />
                      <ReactMarkdown remarkPlugins={[remarkGfm]} components={commentMarkdownComponents}>
                        {renderBody(activeView.content.body)}
                      </ReactMarkdown>
                    </article>
                    <CommentsPanel />
                  </div>
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
            <p className="text-xs text-[var(--color-text-secondary)]">
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
            <p className="text-sm text-[var(--color-text-secondary)]">
              Delete <strong>{pathOp?.title}</strong>
              {pathOp?.isFolder ? ' and everything inside it' : ''} from your draft?
              The change lands on the shared wiki when your submission is merged.
            </p>
            <div className="flex justify-end gap-2">
              <Button variant="outline" onClick={() => setPathOp(null)}>Cancel</Button>
              <Button variant="destructive" onClick={handlePathOp}>Delete</Button>
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
