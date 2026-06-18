import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom';
import { useEffect, useState } from 'react';
import { MainLayout } from './pages/MainLayout';
import { LoginPage } from './pages/LoginPage';
import { getStoredAuth, storeAuth } from './auth';

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
  if (!getStoredAuth()) return <Navigate to="/login" replace />;
  return <>{children}</>;
}

export default function App() {
  // Consume any OAuth fragment exactly once, before routing decisions are made.
  // `ready` gates the first paint so ProtectedRoute never sees a transient
  // "no auth yet, fragment still present" state.
  const [ready, setReady] = useState(false);
  useEffect(() => {
    consumeOAuthFragment();
    setReady(true);
  }, []);

  if (!ready) return null;

  return (
    <BrowserRouter>
      <Routes>
        <Route path="/login" element={<LoginPage />} />
        {/* All content in one layout — no page transitions */}
        <Route path="/*" element={<ProtectedRoute><MainLayout /></ProtectedRoute>} />
      </Routes>
    </BrowserRouter>
  );
}
