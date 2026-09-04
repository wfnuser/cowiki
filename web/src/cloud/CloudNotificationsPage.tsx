import { useEffect, useMemo, useState } from 'react';
import { Bell, CheckCheck, MessageSquare } from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import { SpaceRail } from '../components/layout/SpaceRail';
import { TooltipProvider } from '../components/ui/tooltip';
import { timeAgo } from '../lib/time';
import type { CloudClient, CloudNotification, CloudSpace } from './client';
import { cloudSpaceRoute } from './routes';
import type { CloudSession } from './session';

export function CloudNotificationsPage({
  client,
  session,
  onSignOut,
}: {
  client: CloudClient;
  session: CloudSession;
  onSignOut: () => void;
}) {
  const navigate = useNavigate();
  const [spaces, setSpaces] = useState<CloudSpace[]>([]);
  const [notifications, setNotifications] = useState<CloudNotification[] | null>(null);
  const [error, setError] = useState('');
  const unread = useMemo(
    () => notifications?.filter((notification) => !notification.read).length ?? 0,
    [notifications],
  );

  const reload = () => {
    void client.listNotifications()
      .then(setNotifications)
      .catch((cause) => setError(cause instanceof Error ? cause.message : 'Could not load notifications.'));
  };

  useEffect(() => {
    reload();
    void client.listSpaces().then(setSpaces).catch(() => undefined);
    // client identity is stable for the signed-in session.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [client]);

  const open = async (notification: CloudNotification) => {
    if (!notification.read) {
      setNotifications((current) => current?.map((item) => (
        item.id === notification.id ? { ...item, read: true } : item
      )) ?? null);
      await client.setNotificationRead(notification.id, true).catch(reload);
    }
    navigate(cloudSpaceRoute(notification.spaceId, 'wiki', notification.pagePath));
  };

  const markAll = async () => {
    setNotifications((current) => current?.map((item) => ({ ...item, read: true })) ?? null);
    await client.markAllNotificationsRead().catch(reload);
  };

  return (
    <TooltipProvider>
      <div className="flex h-screen overflow-hidden bg-bg text-text">
        <SpaceRail
          workspaces={spaces}
          activeWorkspaceId={null}
          userName={session.userName}
          onSelectWorkspace={(space) => navigate(cloudSpaceRoute(space.id))}
          onCreateWorkspace={() => navigate('/cloud?action=create')}
          onSettings={() => undefined}
          onDiscover={() => undefined}
          onLogout={onSignOut}
          onConnectCloud={() => undefined}
          notifUnread={unread}
          onShowNotifications={() => undefined}
          showBell
          showCloudActions
          showSettings={false}
          showDiscover={false}
          titlebarInset={false}
          createLabel="New shared Space"
        />
        <main className="min-w-0 flex-1 overflow-auto p-10">
          <div className="mx-auto max-w-3xl">
            <div className="mb-6 flex items-center justify-between gap-4">
              <div>
                <h1 className="font-serif text-2xl font-semibold">Notifications</h1>
                <p className="mt-1 text-sm text-text-tertiary">Mentions shared through Cloud Spaces.</p>
              </div>
              <button
                type="button"
                disabled={unread === 0}
                onClick={() => void markAll()}
                className="inline-flex items-center gap-2 rounded-lg border px-3 py-2 text-sm text-text-secondary disabled:opacity-40"
              >
                <CheckCheck size={15} /> Mark all read
              </button>
            </div>
            {error ? (
              <p className="text-sm text-red">{error}</p>
            ) : notifications == null ? (
              <p className="text-sm text-text-tertiary">Loading…</p>
            ) : notifications.length === 0 ? (
              <div className="rounded-xl border bg-panel px-6 py-12 text-center">
                <Bell className="mx-auto mb-3 text-text-tertiary" size={22} />
                <p className="text-sm text-text-secondary">No mentions yet.</p>
              </div>
            ) : (
              <div className="overflow-hidden rounded-xl border bg-panel">
                {notifications.map((notification) => (
                  <button
                    key={notification.id}
                    type="button"
                    onClick={() => void open(notification)}
                    className="flex w-full items-start gap-3 border-b px-4 py-4 text-left last:border-b-0 hover:bg-bg-hover"
                  >
                    <span className="mt-0.5 grid size-8 shrink-0 place-items-center rounded-lg bg-accent-soft text-accent">
                      <MessageSquare size={15} />
                    </span>
                    <span className="min-w-0 flex-1">
                      <span className="block text-sm text-text">
                        <strong>@{notification.actorHandle}</strong> mentioned you in {notification.spaceName}
                      </span>
                      <span className="mt-1 block truncate text-sm text-text-secondary">{notification.commentBody}</span>
                      <span className="mt-1 block text-xs text-text-tertiary">{notification.pagePath} · {timeAgo(notification.createdAt)}</span>
                    </span>
                    {!notification.read && <span className="mt-2 size-2 rounded-full bg-accent" aria-label="Unread" />}
                  </button>
                ))}
              </div>
            )}
          </div>
        </main>
      </div>
    </TooltipProvider>
  );
}
