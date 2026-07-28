import { useEffect, useMemo, useState } from 'react';
import { ArrowRight, CalendarClock, ShieldCheck } from 'lucide-react';
import { Link, useNavigate, useParams } from 'react-router-dom';
import { getStoredAuth } from '../auth';
import { Button } from '../components/ui/button';
import { apiOrigin } from '../runtime';
import {
  CloudApiError,
  createCloudClient,
  previewCloudInvitation,
  type CloudInvitationPreview,
} from './client';
import { CloudNotice, SpaceMonogram } from './CloudHome';
import { cloudSpaceRoute } from './routes';

export function CloudInvitationPage() {
  const { token = '' } = useParams();
  const navigate = useNavigate();
  const [invitation, setInvitation] = useState<CloudInvitationPreview | null>(null);
  const [loading, setLoading] = useState(true);
  const [joining, setJoining] = useState(false);
  const [error, setError] = useState('');
  const baseUrl = apiOrigin() || window.location.origin;
  const auth = getStoredAuth();
  const client = useMemo(() => {
    if (!auth?.api_key || auth.mode !== 'remote') return null;
    return createCloudClient({
      baseUrl,
      apiKey: auth.api_key,
      userId: auth.id,
      userName: auth.name,
    });
  }, [auth?.api_key, auth?.id, auth?.mode, auth?.name, baseUrl]);

  useEffect(() => {
    let active = true;
    setLoading(true);
    setError('');
    void previewCloudInvitation(baseUrl, token)
      .then((value) => { if (active) setInvitation(value); })
      .catch((cause) => {
        if (!active) return;
        setError(
          cause instanceof CloudApiError && cause.status === 404
            ? 'This invitation is invalid, expired, or has been revoked.'
            : cause instanceof Error ? cause.message : 'Could not load this invitation.',
        );
      })
      .finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [baseUrl, token]);

  const accept = async () => {
    if (!client || !invitation) return;
    setJoining(true);
    setError('');
    try {
      const space = await client.acceptInvitation(token);
      navigate(cloudSpaceRoute(space.id, 'wiki'), { replace: true });
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Could not join this Space.');
      setJoining(false);
    }
  };

  const returnTo = `/invite/${encodeURIComponent(token)}`;

  return (
    <main className="grid min-h-screen place-items-center bg-bg-secondary px-6 py-12 text-text">
      <section className="w-full max-w-lg rounded-2xl border bg-panel p-8 shadow-lg">
        <div className="mb-8 flex items-center gap-3">
          <img src="/cowiki-logo.svg" alt="CoWiki" width={32} height={32} />
          <span className="font-serif text-xl font-bold">CoWiki</span>
        </div>
        {loading && <p className="text-sm text-text-tertiary">Loading invitation…</p>}
        {error && <CloudNotice tone="error">{error}</CloudNotice>}
        {!loading && invitation && (
          <>
            <div className="mb-6 flex items-center gap-4">
              <SpaceMonogram name={invitation.spaceName} />
              <div>
                <div className="text-xs font-semibold uppercase tracking-[0.1em] text-accent">
                  Space invitation
                </div>
                <h1 className="mt-1 font-serif text-3xl font-bold">{invitation.spaceName}</h1>
              </div>
            </div>
            <p className="text-sm leading-6 text-text-secondary">
              Join this shared Space to read its accepted knowledge in the browser and submit
              local work for review.
            </p>
            <div className="my-6 grid gap-3 rounded-xl bg-secondary p-4 text-sm">
              <div className="flex items-center gap-2">
                <ShieldCheck size={16} className="text-accent" />
                Role: <strong className="capitalize">{invitation.role}</strong>
              </div>
              <div className="flex items-center gap-2 text-text-secondary">
                <CalendarClock size={16} />
                Expires {new Date(invitation.expiresAt).toLocaleString()}
              </div>
            </div>
            {client ? (
              <Button className="w-full" disabled={joining} onClick={() => void accept()}>
                {joining ? 'Joining…' : 'Join Space'} <ArrowRight />
              </Button>
            ) : (
              <Button asChild className="w-full">
                <Link to={`/login?returnTo=${encodeURIComponent(returnTo)}`}>
                  Sign in with GitHub <ArrowRight />
                </Link>
              </Button>
            )}
          </>
        )}
      </section>
    </main>
  );
}
