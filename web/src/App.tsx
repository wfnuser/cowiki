import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom';
import { useEffect, useState } from 'react';
import { MainLayout } from './pages/MainLayout';
import { LoginPage } from './pages/LoginPage';
import { authHeaders, clearAuth, getCurrentAuth, getStoredAuth, storeAuth, tryLocalLogin } from './auth';
import { apiBase, isDesktopClient } from './runtime';
import { CloudApp } from './cloud/CloudApp';

/** OAuth hands the credential over in the URL *fragment* (never sent to servers,
 *  logs, or Referer). Parse #api_key=...&user_name=...&user_id=..., store, then
 *  scrub the fragment from the address bar/history. */
function consumeOAuthFragment(): boolean {
  const hash = window.location.hash.replace(/^#/, '');
  if (!hash.includes('api_key=')) return false;
  const params = new URLSearchParams(hash);
  const apiKey = params.get('api_key');
  const userName = params.get('user_name');
  const userId = params.get('user_id');
  if (!apiKey || !userName || !userId) return false;
  storeAuth(apiKey, userName, userId);
  // Scrub the fragment so the key doesn't linger in the URL/history entry.
  window.history.replaceState(null, '', window.location.pathname);
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
      consumeOAuthFragment();
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
