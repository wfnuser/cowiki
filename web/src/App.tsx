import { BrowserRouter, Routes, Route, Navigate, useSearchParams, useNavigate } from 'react-router-dom';
import { useEffect } from 'react';
import { MainLayout } from './pages/MainLayout';
import { LoginPage } from './pages/LoginPage';
import { getStoredAuth, storeAuth } from './auth';

function OAuthInterceptor({ children }: { children: React.ReactNode }) {
  const [searchParams, setSearchParams] = useSearchParams();
  const navigate = useNavigate();

  useEffect(() => {
    const apiKey = searchParams.get('api_key');
    const userName = searchParams.get('user_name');
    const userId = searchParams.get('user_id');

    if (apiKey && userName && userId) {
      storeAuth(apiKey, userName, userId);
      setSearchParams({}, { replace: true });
      navigate('/', { replace: true });
    }
  }, [searchParams, setSearchParams, navigate]);

  return <>{children}</>;
}

function ProtectedRoute({ children }: { children: React.ReactNode }) {
  const auth = getStoredAuth();
  if (!auth) return <Navigate to="/login" replace />;
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
