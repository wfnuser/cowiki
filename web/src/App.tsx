import { BrowserRouter, Routes, Route, Navigate, useNavigate } from 'react-router-dom';
import { useEffect } from 'react';
import { MainLayout } from './pages/MainLayout';
import { LoginPage } from './pages/LoginPage';
import { getStoredAuth, storeAuth } from './auth';

/** OAuth hands the credential over in the URL *fragment* (never sent to servers,
 *  logs, or Referer). Parse #api_key=...&user_name=...&user_id=..., store, then
 *  scrub the fragment from the address bar/history. */
function readOAuthFragment(): { apiKey: string; userName: string; userId: string } | null {
  const hash = window.location.hash.replace(/^#/, '');
  if (!hash.includes('api_key=')) return null;
  const params = new URLSearchParams(hash);
  const apiKey = params.get('api_key');
  const userName = params.get('user_name');
  const userId = params.get('user_id');
  if (!apiKey || !userName || !userId) return null;
  return { apiKey, userName, userId };
}

function OAuthInterceptor({ children }: { children: React.ReactNode }) {
  const navigate = useNavigate();

  useEffect(() => {
    const creds = readOAuthFragment();
    if (creds) {
      storeAuth(creds.apiKey, creds.userName, creds.userId);
      // Remove the fragment so the key doesn't linger in the URL/history entry.
      window.history.replaceState(null, '', window.location.pathname);
      navigate('/', { replace: true });
    }
  }, [navigate]);

  return <>{children}</>;
}

function ProtectedRoute({ children }: { children: React.ReactNode }) {
  const auth = getStoredAuth();
  // Don't redirect while the OAuth fragment is still being processed.
  if (!auth && !readOAuthFragment()) return <Navigate to="/login" replace />;
  return <>{children}</>;
}

export default function App() {
  return (
    <BrowserRouter>
      <OAuthInterceptor>
        <Routes>
          <Route path="/login" element={<LoginPage />} />
          {/* All content in one layout — no page transitions */}
          <Route path="/*" element={<ProtectedRoute><MainLayout /></ProtectedRoute>} />
        </Routes>
      </OAuthInterceptor>
    </BrowserRouter>
  );
}
