import { useCallback, useEffect, useState } from 'react';
import { BookOpen, ChevronRight, GitPullRequest, Users } from 'lucide-react';
import { Link } from 'react-router-dom';
import type { CloudClient, CloudSpace } from './client';
import { cloudNavigation } from './cloud-shell-model';
import { CloudHeader, CloudNotice, SpaceMonogram } from './CloudHome';
import { CloudMembersView } from './CloudMembersView';
import { CloudReviewsView } from './CloudReviewsView';
import { CloudWikiView } from './CloudWikiView';
import { cloudSpaceRoute, type ParsedCloudRoute } from './routes';
import type { CloudSession } from './session';

interface CloudSpaceViewProps {
  client: CloudClient;
  session: CloudSession;
  route: ParsedCloudRoute;
  onSignOut: () => void;
}

const icons = { wiki: BookOpen, reviews: GitPullRequest, members: Users };

export function CloudSpaceView({ client, session, route, onSignOut }: CloudSpaceViewProps) {
  const [space, setSpace] = useState<CloudSpace | null>(null);
  const [error, setError] = useState('');
  const loadSpace = useCallback(async () => {
    setError('');
    try {
      setSpace(await client.getSpace(route.spaceId));
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Could not load this Cloud Space.');
    }
  }, [client, route.spaceId]);

  useEffect(() => { void loadSpace(); }, [loadSpace]);

  return (
    <div className="flex min-h-screen flex-col bg-bg text-text">
      <CloudHeader userName={session.userName} onSignOut={onSignOut} />
      <div className="flex min-h-0 flex-1">
        <aside className="w-64 shrink-0 border-r bg-bg-secondary">
          <div className="flex h-[76px] items-center gap-3 border-b px-5">
            <SpaceMonogram name={space?.name || 'Space'} />
            <div className="min-w-0">
              <div className="truncate text-[15px] font-semibold">{space?.name || 'Loading…'}</div>
              <div className="mt-0.5 text-[11px] font-semibold uppercase tracking-[0.1em] text-text-tertiary">Cloud main</div>
            </div>
          </div>
          <nav className="p-3">
            {(space ? cloudNavigation(space.role) : []).map((item) => {
              const Icon = icons[item.id];
              const active = route.view === item.id;
              return (
                <Link
                  key={item.id}
                  to={cloudSpaceRoute(route.spaceId, item.id)}
                  className={`mb-1 flex items-center gap-3 rounded-lg px-3 py-2.5 text-sm no-underline transition ${active ? 'bg-accent-soft font-semibold text-accent' : 'text-text-secondary hover:bg-bg-hover'}`}
                >
                  <Icon size={17} />
                  <span className="flex-1">{item.label}</span>
                  {active && <ChevronRight size={14} />}
                </Link>
              );
            })}
          </nav>
          {space && (
            <div className="mx-5 mt-3 border-t pt-4 text-xs leading-5 text-text-tertiary">
              Your role
              <div className="mt-1 font-semibold capitalize text-text-secondary">{space.role}</div>
            </div>
          )}
        </aside>

        <main className="min-w-0 flex-1">
          {error && <div className="p-8"><CloudNotice tone="error">{error}</CloudNotice></div>}
          {!error && !space && <div className="p-10 text-sm text-text-tertiary">Loading Cloud Space…</div>}
          {space && route.view === 'wiki' && (
            <CloudWikiView client={client} space={space} documentPath={route.documentPath} />
          )}
          {space && route.view === 'reviews' && (
            <CloudReviewsView client={client} space={space} pullRequestId={route.documentPath} />
          )}
          {space && route.view === 'members' && (
            <CloudMembersView client={client} space={space} currentUserId={session.userId} />
          )}
        </main>
      </div>
    </div>
  );
}
