import { BrowserRouter, Routes, Route, Navigate, useSearchParams, useNavigate } from 'react-router-dom';
import { useEffect } from 'react';
import { Layout } from './components/Layout';
import { HomePage } from './pages/HomePage';
import { WikiPage } from './pages/WikiPage';
import { PageViewPage } from './pages/PageViewPage';
import { ReviewPage } from './pages/ReviewPage';
import { SearchPage } from './pages/SearchPage';
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
          {/* Home — workspace list */}
          <Route path="/" element={<ProtectedRoute><HomePage /></ProtectedRoute>} />
          {/* Inside a workspace */}
          <Route path="/w/:workspaceSlug" element={<ProtectedRoute><Layout /></ProtectedRoute>}>
            <Route index element={<WikiPage />} />
            <Route path="page/:slug" element={<PageViewPage />} />
            <Route path="reviews" element={<ReviewPage />} />
            <Route path="search" element={<SearchPage />} />
          </Route>
        </Routes>
      </OAuthInterceptor>
    </BrowserRouter>
  );
}
