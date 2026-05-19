import { useEffect, useState } from 'react';
import { useParams, useSearchParams, Link } from 'react-router-dom';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { getPage, type PageFull } from '../api';

export function PageViewPage() {
  const { slug } = useParams<{ slug: string }>();
  const [searchParams] = useSearchParams();
  const branch = searchParams.get('branch') || 'main';
  const [page, setPage] = useState<PageFull | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    if (!slug) return;
    setLoading(true);
    getPage(slug, branch).then(setPage).finally(() => setLoading(false));
  }, [slug, branch]);

  if (loading) return <div className="text-center py-12 text-stone-400">Loading...</div>;
  if (!page) return <div className="text-center py-12 text-stone-400">Page not found</div>;

  // Strip frontmatter from body for rendering
  let body = page.body;
  if (body.startsWith('---')) {
    const parts = body.split('---');
    if (parts.length >= 3) {
      body = parts.slice(2).join('---').trim();
    }
  }

  return (
    <div>
      <div className="mb-6">
        <Link
          to={branch === 'main' ? '/' : '/personal'}
          className="text-sm text-stone-400 hover:text-stone-600"
        >
          &larr; Back
        </Link>
      </div>
      <article className="bg-white rounded-lg border border-stone-200 p-8">
        <h1 className="text-3xl font-bold text-stone-800 mb-2">{page.title}</h1>
        {page.summary && (
          <p className="text-stone-500 mb-6">{page.summary}</p>
        )}
        <div className="prose prose-stone max-w-none">
          <ReactMarkdown remarkPlugins={[remarkGfm]}>{body}</ReactMarkdown>
        </div>
      </article>
    </div>
  );
}
