import { useCallback, useEffect, useState } from 'react';
import { ArrowRight, Cloud, LogOut, RefreshCw } from 'lucide-react';
import { Link } from 'react-router-dom';
import { AvatarBadge } from '../components/ui/avatar-badge';
import { Button } from '../components/ui/button';
import type { CloudClient, CloudSpace } from './client';
import { cloudSpaceRoute } from './routes';
import type { CloudSession } from './session';

interface CloudHomeProps {
  client: CloudClient;
  session: CloudSession;
  onSignOut: () => void;
}

export function CloudHome({ client, session, onSignOut }: CloudHomeProps) {
  const [spaces, setSpaces] = useState<CloudSpace[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const loadSpaces = useCallback(async () => {
    setLoading(true);
    setError('');
    try {
      setSpaces(await client.listSpaces());
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Could not load Cloud Spaces.');
    } finally {
      setLoading(false);
    }
  }, [client]);

  useEffect(() => { void loadSpaces(); }, [loadSpaces]);

  return (
    <div className="min-h-screen bg-bg text-text">
      <CloudHeader userName={session.userName} onSignOut={onSignOut} />
      <main className="mx-auto w-full max-w-6xl px-8 py-14">
        <div className="mb-10 flex items-end justify-between gap-8">
          <div>
            <div className="mb-3 flex items-center gap-2 text-xs font-semibold uppercase tracking-[0.16em] text-accent">
              <Cloud size={15} /> CoWiki Cloud
            </div>
            <h1 className="font-serif text-5xl font-bold tracking-[-0.025em]">Shared Spaces</h1>
            <p className="mt-3 max-w-xl text-[15px] leading-7 text-text-tertiary">
              Browse knowledge that has reached Cloud main. Local drafts stay on their authors&apos; devices until submitted and merged.
            </p>
          </div>
          <Button variant="outline" onClick={() => void loadSpaces()} disabled={loading}>
            <RefreshCw className={loading ? 'animate-spin' : ''} /> Refresh
          </Button>
        </div>

        {error && <CloudNotice tone="error">{error}</CloudNotice>}
        {loading ? (
          <div className="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3">
            {[0, 1, 2].map((key) => <div key={key} className="h-44 animate-pulse rounded-xl border bg-panel" />)}
          </div>
        ) : spaces.length === 0 ? (
          <section className="rounded-2xl border border-dashed bg-panel px-8 py-16 text-center">
            <div className="mx-auto mb-5 grid size-12 place-items-center rounded-xl bg-accent-soft text-accent"><Cloud /></div>
            <h2 className="font-serif text-2xl font-semibold">No shared Space yet</h2>
            <p className="mx-auto mt-2 max-w-md text-sm leading-6 text-text-tertiary">
              Open a local Space in the CoWiki desktop app and choose Publish to Cloud. Publishing is explicit and never uploads a Space in the background.
            </p>
          </section>
        ) : (
          <div className="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3">
            {spaces.map((space) => (
              <Link
                key={space.id}
                to={cloudSpaceRoute(space.id)}
                className="group flex min-h-44 flex-col rounded-xl border bg-panel p-6 text-inherit no-underline transition hover:-translate-y-0.5 hover:border-border-hover hover:shadow-[0_12px_36px_rgba(29,28,26,0.08)]"
              >
                <div className="mb-8 flex items-start justify-between">
                  <SpaceMonogram name={space.name} />
                  <span className="rounded-full bg-secondary px-2.5 py-1 text-[11px] font-semibold capitalize text-text-secondary">{space.role}</span>
                </div>
                <h2 className="text-[17px] font-semibold">{space.name}</h2>
                <div className="mt-1 flex items-center justify-between text-xs text-text-tertiary">
                  <span>{space.slug}</span>
                  <ArrowRight className="transition-transform group-hover:translate-x-0.5" size={15} />
                </div>
              </Link>
            ))}
          </div>
        )}
      </main>
    </div>
  );
}

export function CloudHeader({ userName, onSignOut }: { userName: string; onSignOut: () => void }) {
  return (
    <header className="flex h-14 items-center border-b bg-panel px-6">
      <Link to="/cloud" className="flex items-center gap-2.5 text-text no-underline">
        <img src="/cowiki-logo.svg" alt="" className="size-7" />
        <span className="font-serif text-xl font-bold">CoWiki</span>
        <span className="rounded-full bg-accent-soft px-2 py-0.5 text-[10px] font-bold uppercase tracking-wider text-accent">Cloud</span>
      </Link>
      <div className="ml-auto flex items-center gap-3">
        <AvatarBadge name={userName} size={28} />
        <span className="hidden text-sm font-medium text-text-secondary sm:inline">{userName}</span>
        <Button variant="ghost" size="icon-sm" aria-label="Sign out" onClick={onSignOut}><LogOut /></Button>
      </div>
    </header>
  );
}

export function SpaceMonogram({ name }: { name: string }) {
  return <div className="grid size-10 place-items-center rounded-[11px] bg-[#5d8a6c] text-sm font-bold text-white">{name[0]?.toUpperCase() || 'S'}</div>;
}

export function CloudNotice({ children, tone = 'neutral' }: { children: React.ReactNode; tone?: 'neutral' | 'error' | 'success' }) {
  const styles = tone === 'error'
    ? 'border-red/20 bg-red-soft text-red'
    : tone === 'success'
      ? 'border-green/20 bg-green-soft text-green'
      : 'border-border bg-secondary text-text-secondary';
  return <div className={`mb-5 rounded-lg border px-4 py-3 text-sm ${styles}`}>{children}</div>;
}

