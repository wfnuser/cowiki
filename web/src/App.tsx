import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom';
import { useEffect, useState } from 'react';
import { MainLayout } from './pages/MainLayout';
import { LoginPage } from './pages/LoginPage';
import { authHeaders, clearAuth, getCurrentAuth, getStoredAuth, storeAuth, tryLocalLogin } from './auth';
import { apiBase, isDesktopClient } from './runtime';
import { CloudApp } from './cloud/CloudApp';
import {
  AUTH_RETURN_PATH_STORAGE,
  exchangeOAuthCode,
  parseWebOAuthFragment,
  safeAuthReturnPath,
} from './auth-flow';

/** OAuth returns a short-lived one-time code in the fragment. Exchange it
 * before routing, persist only the resulting credential, and scrub the URL. */
async function consumeOAuthFragment(): Promise<boolean> {
  const code = parseWebOAuthFragment(window.location.hash);
  if (!code) return false;
  const credential = await exchangeOAuthCode(apiBase(), code);
  storeAuth(credential.apiKey, credential.userName, credential.userId);
  const returnPath = safeAuthReturnPath(
    window.sessionStorage.getItem(AUTH_RETURN_PATH_STORAGE),
  );
  window.sessionStorage.removeItem(AUTH_RETURN_PATH_STORAGE);
  window.history.replaceState(null, '', returnPath);
  return true;
}

function ProtectedRoute({ children }: { children: React.ReactNode }) {
  // No hash read here — the OAuth fragment is consumed once at startup (below),
  // so a stored session is the single source of truth on every render.
  if (!getCurrentAuth()) return <Navigate to="/login" replace />;
  return <>{children}</>;
}

export default function App() {
  // Consume any OAuth fragment exactly once, before routing decisions are made.
  // `ready` gates the first paint so ProtectedRoute never sees a transient
  // "no auth yet, fragment still present" state.
  const [ready, setReady] = useState(false);
  useEffect(() => {
    const bootstrap = async () => {
      try {
        await consumeOAuthFragment();
      } catch {
        clearAuth();
        window.history.replaceState(null, '', '/login?error=oauth');
      }
      // Desktop local mode talks straight to the Tauri local engine. It has no
      // account and must never wait for, or redirect through, a sign-in flow.
      if (isDesktopClient()) {
        setReady(true);
        return;
      }
      // Local-first: without a stored session, try the backend's local-mode
      // sign-in (single-user installs / the desktop app's local server).
      // Hosted deploys disable the endpoint and fall through to login.
      if (!getStoredAuth()) {
        await tryLocalLogin();
      } else {
        // A stored session can go stale (e.g. the local metadata store was
        // recreated). Validate once at boot; on a definite 401 re-mint via
        // local login. Network errors keep the session — being offline must
        // not log anyone out.
        try {
          const res = await fetch(`${apiBase()}/auth/me`, { headers: authHeaders() });
          if (res.status === 401) {
            clearAuth();
            await tryLocalLogin();
          }
        } catch { /* offline — keep session */ }
      }
      setReady(true);
    };
    void bootstrap();
  }, []);

  if (!ready) return null;

  return (
    <BrowserRouter>
      <Routes>
        <Route path="/login" element={<LoginPage />} />
        <Route path="/cloud/*" element={<CloudApp />} />
        <Route path="/" element={isDesktopClient()
          ? <ProtectedRoute><MainLayout /></ProtectedRoute>
          : <Navigate to="/cloud" replace />} />
        {/* All content in one layout — no page transitions */}
        <Route path="/*" element={<ProtectedRoute><MainLayout /></ProtectedRoute>} />
      </Routes>
    </BrowserRouter>
  );
}
