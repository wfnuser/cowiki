import { BrowserRouter, Routes, Route, Navigate, useSearchParams, useNavigate } from 'react-router-dom';
import { useEffect } from 'react';
import { MainLayout } from './pages/MainLayout';
import { LoginPage } from './pages/LoginPage';
import { getStoredAuth, storeAuth } from './auth';

function OAuthInterceptor({ children }: { children: React.ReactNode }) {
  const [searchParams] = useSearchParams();
  const navigate = useNavigate();

  useEffect(() => {
    const apiKey = searchParams.get('api_key');
    const userName = searchParams.get('user_name');
    const userId = searchParams.get('user_id');

    if (apiKey && userName && userId) {
      storeAuth(apiKey, userName, userId);
      navigate('/', { replace: true });
    }
  }, [searchParams, navigate]);

  return <>{children}</>;
}

function ProtectedRoute({ children }: { children: React.ReactNode }) {
  const [searchParams] = useSearchParams();
  const auth = getStoredAuth();
  // Don't redirect if we're in the middle of OAuth callback (api_key in URL)
  const hasOAuthParams = searchParams.has('api_key');
  if (!auth && !hasOAuthParams) return <Navigate to="/login" replace />;
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
