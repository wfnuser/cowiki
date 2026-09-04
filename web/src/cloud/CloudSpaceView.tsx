import { useEffect, useMemo, useState } from 'react';
import { PanelLeft } from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import type { PageMeta, Workspace } from '../api';
import {
  ContentBreadcrumb,
  ContentHeader,
} from '../components/layout/ContentHeader';
import { SpacePanel, type NavTab } from '../components/layout/SpacePanel';
import { SpaceRail } from '../components/layout/SpaceRail';
import { TooltipProvider } from '../components/ui/tooltip';
import { conceptIdFromPath } from '../lib/okf-pages';
import {
  CloudApiError,
  type CloudClient,
  type CloudSpace,
  type CloudTree,
} from './client';
import { CloudNotice } from './CloudHome';
import { CloudMembersView } from './CloudMembersView';
import { CloudReviewsView } from './CloudReviewsView';
import { CloudWikiView } from './CloudWikiView';
import { routeScopedValue, type SpaceScopedResource } from './cloud-shell-model';
import { cloudSpaceRoute, type ParsedCloudRoute } from './routes';
import type { CloudSession } from './session';

interface CloudSpaceViewProps {
  client: CloudClient;
  session: CloudSession;
  route: ParsedCloudRoute;
  onSignOut: () => void;
}

const EMPTY_MAIN: CloudTree = { ref: 'main', oid: '', entries: [] };

export function CloudSpaceView({ client, session, route, onSignOut }: CloudSpaceViewProps) {
  const navigate = useNavigate();
  const [spaces, setSpaces] = useState<CloudSpace[]>([]);
  const [loadedSpace, setLoadedSpace] = useState<SpaceScopedResource<CloudSpace> | null>(null);
  const [loadedTree, setLoadedTree] = useState<SpaceScopedResource<CloudTree> | null>(null);
  const [spaceErrorState, setSpaceErrorState] = useState<SpaceScopedResource<string> | null>(null);
  const [treeErrorState, setTreeErrorState] = useState<SpaceScopedResource<string> | null>(null);
  const [unpublishedState, setUnpublishedState] = useState<SpaceScopedResource<boolean> | null>(null);
  const [panelCollapsed, setPanelCollapsed] = useState(false);
  const [unread, setUnread] = useState(0);
  const space = routeScopedValue(route.spaceId, loadedSpace);
  const tree = routeScopedValue(route.spaceId, loadedTree);
  const spaceError = routeScopedValue(route.spaceId, spaceErrorState) ?? '';
  const treeError = routeScopedValue(route.spaceId, treeErrorState) ?? '';
  const unpublished = routeScopedValue(route.spaceId, unpublishedState) ?? false;
  const pages = useMemo(() => cloudTreePages(tree), [tree]);
  const workspace = useMemo(() => space ? cloudWorkspace(space) : null, [space]);
  const headerLabel = cloudHeaderLabel(route);

  useEffect(() => {
    let active = true;
    void client.getSpace(route.spaceId)
      .then((value) => {
        if (!active) return;
        setLoadedSpace({ spaceId: route.spaceId, value });
        setSpaceErrorState(null);
      })
      .catch((cause) => {
        if (active) {
          setSpaceErrorState({
            spaceId: route.spaceId,
            value: cause instanceof Error ? cause.message : 'Could not load this Cloud Space.',
          });
        }
      });
    return () => { active = false; };
  }, [client, route.spaceId]);

  useEffect(() => {
    let active = true;
    void client.getTree(route.spaceId)
      .then((value) => {
        if (!active) return;
        setLoadedTree({ spaceId: route.spaceId, value });
        setTreeErrorState(null);
        setUnpublishedState({ spaceId: route.spaceId, value: false });
      })
      .catch((cause) => {
        if (!active) return;
        if (cause instanceof CloudApiError && cause.status === 404) {
          setLoadedTree({ spaceId: route.spaceId, value: EMPTY_MAIN });
          setTreeErrorState(null);
          setUnpublishedState({ spaceId: route.spaceId, value: true });
          return;
        }
        setTreeErrorState({
          spaceId: route.spaceId,
          value: cause instanceof Error ? cause.message : 'Could not load Cloud main.',
        });
      });
    return () => { active = false; };
  }, [client, route.spaceId]);

  useEffect(() => {
    let active = true;
    void client.listSpaces()
      .then((value) => { if (active) setSpaces(value); })
      .catch(() => undefined);
    return () => { active = false; };
  }, [client]);

  useEffect(() => {
    void client.notificationUnreadCount().then((value) => setUnread(value.count)).catch(() => undefined);
  }, [client]);

  const changeTab = (tab: NavTab) => {
    if (tab === 'wiki' || tab === 'reviews' || tab === 'members') {
      navigate(cloudSpaceRoute(route.spaceId, tab));
    }
  };

  return (
    <TooltipProvider>
      <div className="flex h-screen overflow-hidden bg-bg text-text">
        <SpaceRail
          workspaces={spaces}
          activeWorkspaceId={route.spaceId}
          userName={session.userName}
          onSelectWorkspace={(nextSpace) => navigate(cloudSpaceRoute(nextSpace.id))}
          onCreateWorkspace={() => navigate('/cloud?action=create')}
          onSettings={() => undefined}
          onDiscover={() => undefined}
          onLogout={onSignOut}
          onConnectCloud={() => undefined}
          notifUnread={unread}
          onShowNotifications={() => navigate('/cloud/notifications')}
          showBell
          showCloudActions
          showSettings={false}
          showDiscover={false}
          titlebarInset={false}
          createLabel="New shared Space"
        />

        {!panelCollapsed && (
          <div className="w-[292px] min-w-[292px]">
            <SpacePanel
              workspace={workspace}
              activeTab={route.view}
              onTabChange={changeTab}
              pages={pages}
              sources={[]}
              activePage={route.documentPath ? conceptIdFromPath(route.documentPath) : null}
              activeSource={null}
              reviewCount={0}
              isPersonal={false}
              showReviews
              showLinkDiagnostics={false}
              showHistory={false}
              isOwner={space?.role === 'owner'}
              readOnly
              onSelectPage={(_conceptId, path) => {
                if (path) navigate(cloudSpaceRoute(route.spaceId, 'wiki', path));
              }}
              onSelectSource={() => undefined}
              onNewPage={() => undefined}
              onNewFolder={() => undefined}
              onAddPageInFolder={() => undefined}
              onAddFolderInFolder={() => undefined}
              onRenamePath={() => undefined}
              onDeletePath={() => undefined}
              onShowIngest={() => undefined}
              onCollapse={() => setPanelCollapsed(true)}
            />
          </div>
        )}

        <main className="relative flex min-w-0 flex-1 flex-col overflow-hidden">
          {panelCollapsed && (
            <button
              type="button"
              aria-label="Expand sidebar"
              title="Expand sidebar"
              onClick={() => setPanelCollapsed(false)}
              className="absolute left-3 top-2.5 z-20 grid size-7 place-items-center rounded-md text-text-tertiary hover:bg-bg-hover hover:text-text-secondary"
            >
              <PanelLeft size={16} />
            </button>
          )}
          <ContentHeader>
            <ContentBreadcrumb>
              {panelCollapsed && <span style={{ width: 28 }} />}
              <span className="text-text-secondary">{space?.name ?? 'Cloud'}</span>
              {headerLabel && (
                <>
                  <span className="text-border-hover">/</span>
                  <span className="truncate text-text">{headerLabel}</span>
                </>
              )}
            </ContentBreadcrumb>
          </ContentHeader>

          <div className="relative min-h-0 flex-1">
            {spaceError ? (
              <div className="p-8"><CloudNotice tone="error">{spaceError}</CloudNotice></div>
            ) : !space ? (
              <div className="p-10 text-sm text-text-tertiary">Loading Space…</div>
            ) : route.view === 'wiki' ? (
              <CloudWikiView
                client={client}
                space={space}
                tree={tree}
                treeError={treeError}
                unpublished={unpublished}
                documentPath={route.documentPath}
                currentUserId={session.userId}
                currentUserName={session.userName}
              />
            ) : route.view === 'reviews' ? (
              <CloudReviewsView client={client} space={space} pullRequestId={route.documentPath} />
            ) : (
              <CloudMembersView client={client} space={space} currentUserId={session.userId} />
            )}
          </div>
        </main>
      </div>
    </TooltipProvider>
  );
}

function cloudHeaderLabel(route: ParsedCloudRoute): string {
  if (route.view === 'reviews') return route.documentPath ? 'Review' : 'Reviews';
  if (route.view === 'members') return 'Members';
  if (!route.documentPath) return '';
  return route.documentPath
    .split('/')
    .at(-1)
    ?.replace(/\.md$/i, '')
    .replace(/[-_]/g, ' ') || '';
}

function cloudWorkspace(space: CloudSpace): Workspace {
  return {
    id: space.id,
    name: space.name,
    slug: space.slug,
    role: space.role,
    visibility: 'shared',
  };
}

function cloudTreePages(tree: CloudTree | null): PageMeta[] {
  if (!tree) return [];
  const nodes = new Map<string, PageMeta>();
  for (const entry of tree.entries) {
    const title = entry.path.split('/').at(-1)?.replace(/\.md$/i, '').replace(/[-_]/g, ' ') || entry.path;
    nodes.set(entry.path, {
      slug: entry.kind === 'page' ? conceptIdFromPath(entry.path) : entry.path,
      path: entry.path,
      title,
      summary: '',
      branch: 'main',
      kind: entry.kind,
      children: [],
    });
  }
  const roots: PageMeta[] = [];
  for (const node of nodes.values()) {
    const parentPath = node.path.includes('/') ? node.path.slice(0, node.path.lastIndexOf('/')) : '';
    const parent = parentPath ? nodes.get(parentPath) : null;
    if (parent?.kind === 'folder') parent.children.push(node);
    else roots.push(node);
  }
  return roots;
}
