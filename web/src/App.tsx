import { BrowserRouter, Routes, Route } from 'react-router-dom';
import { Layout } from './components/Layout';
import { WikiPage } from './pages/WikiPage';
import { PersonalPage } from './pages/PersonalPage';
import { PageViewPage } from './pages/PageViewPage';
import { ReviewPage } from './pages/ReviewPage';
import { SearchPage } from './pages/SearchPage';

export default function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route element={<Layout />}>
          <Route path="/" element={<WikiPage />} />
          <Route path="/personal" element={<PersonalPage />} />
          <Route path="/page/:slug" element={<PageViewPage />} />
          <Route path="/reviews" element={<ReviewPage />} />
          <Route path="/search" element={<SearchPage />} />
        </Route>
      </Routes>
    </BrowserRouter>
  );
}
