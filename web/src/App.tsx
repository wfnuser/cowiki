import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom';
import { useEffect, useState } from 'react';
import { MainLayout } from './pages/MainLayout';
import { LoginPage } from './pages/LoginPage';
import { authHeaders, clearAuth, getCurrentAuth, getStoredAuth, storeAuth } from './auth';
import { apiBase, isDesktopClient } from './runtime';
import { CloudApp } from './cloud/CloudApp';
import { CloudInvitationPage } from './cloud/CloudInvitationPage';
import { PublicCloudSpacePage } from './cloud/PublicCloudSpacePage';
import {
  AUTH_RETURN_PATH_STORAGE,
  createWebAuthBootstrap,
  exchangeOAuthCode,
  parseWebOAuthFragment,
  safeAuthReturnPath,
  validateWebCredential,
} from './auth-flow';

const runWebAuthBootstrap = createWebAuthBootstrap({
  readOAuthCode: () => parseWebOAuthFragment(window.location.hash),
  exchangeOAuthCode: (code) => exchangeOAuthCode(apiBase(), code),
  storeCredential: (credential) => {
    storeAuth(credential.apiKey, credential.userName, credential.userId);
  },
  hasStoredCredential: () => Boolean(getStoredAuth()),
  validateStoredCredential: () => validateWebCredential(apiBase(), authHeaders()),
  clearCredential: clearAuth,
  finishOAuth: () => {
    const returnPath = safeAuthReturnPath(
      window.sessionStorage.getItem(AUTH_RETURN_PATH_STORAGE),
    );
    window.sessionStorage.removeItem(AUTH_RETURN_PATH_STORAGE);
    window.history.replaceState(null, '', returnPath);
  },
  failOAuth: () => {
    window.history.replaceState(null, '', '/login?error=oauth');
  },
});

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
      // Desktop local mode talks straight to the Tauri local engine. It has no
      // account and must never wait for, or redirect through, a sign-in flow.
      if (isDesktopClient()) {
        setReady(true);
        return;
      }
      await runWebAuthBootstrap();
      setReady(true);
    };
    void bootstrap();
  }, []);

  if (!ready) return null;

  return (
    <BrowserRouter>
      <Routes>
        <Route path="/login" element={<LoginPage />} />
        <Route path="/invite/:token" element={<CloudInvitationPage />} />
        <Route path="/spaces/:slug/*" element={<PublicCloudSpacePage />} />
        <Route path="/auth/callback" element={<Navigate to="/login" replace />} />
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
