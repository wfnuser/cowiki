import { useMemo } from 'react';
import { Navigate, useLocation, useNavigate } from 'react-router-dom';
import { clearAuth, getStoredAuth } from '../auth';
import { apiOrigin } from '../runtime';
import { createCloudClient } from './client';
import { CloudHome } from './CloudHome';
import { CloudSpaceView } from './CloudSpaceView';
import { parseCloudRoute } from './routes';
import { normalizeCloudSession, type CloudSession } from './session';

interface CloudAppProps {
  /** Explicit seam for tests and preview harnesses. Production uses stored OAuth auth. */
  session?: CloudSession;
}

export function CloudApp({ session: injectedSession }: CloudAppProps) {
  const location = useLocation();
  const navigate = useNavigate();
  const session = useMemo(
    () => injectedSession ? normalizeCloudSession(injectedSession) : storedCloudSession(),
    [injectedSession],
  );
  const client = useMemo(() => session ? createCloudClient(session) : null, [session]);

  if (!session || !client) return <Navigate to="/login" replace />;

  const route = parseCloudRoute(location.pathname);
  const signOut = async () => {
    try {
      await client.logout();
    } catch {
      // Local sign-out must still complete if the server is offline.
    } finally {
      clearAuth();
      navigate('/login', { replace: true });
    }
  };
  if (location.pathname === '/cloud' || location.pathname === '/cloud/') {
    return <CloudHome client={client} session={session} onSignOut={() => void signOut()} />;
  }
  if (route) {
    return (
      <CloudSpaceView
        client={client}
        session={session}
        route={route}
        onSignOut={() => void signOut()}
      />
    );
  }
  return <Navigate to="/cloud" replace />;
}

function storedCloudSession(): CloudSession | null {
  const auth = getStoredAuth();
  if (!auth?.api_key || auth.mode !== 'remote') return null;
  const baseUrl = apiOrigin() || window.location.origin;
  try {
    return normalizeCloudSession({
      baseUrl,
      apiKey: auth.api_key,
      userId: auth.id,
      userName: auth.name,
    });
  } catch {
    return null;
  }
}
