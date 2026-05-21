import { useEffect, useState } from 'react';
import { useParams, useSearchParams, Link } from 'react-router-dom';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { ChevronLeft } from 'lucide-react';
import { getPage, type PageFull } from '../api';

export function PageViewPage() {
  const { slug, workspaceSlug } = useParams<{ slug: string; workspaceSlug: string }>();
  const [searchParams] = useSearchParams();
  const branch = searchParams.get('branch') || 'main';
  const [page, setPage] = useState<PageFull | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    if (!slug) return;
    setLoading(true);
    getPage(slug, branch).then(setPage).finally(() => setLoading(false));
  }, [slug, branch]);

  if (loading) {
    return <div className="py-12 text-center text-[var(--color-text-tertiary)] text-sm">Loading...</div>;
  }
  if (!page) {
    return <div className="py-12 text-center text-[var(--color-text-tertiary)] text-sm">Page not found</div>;
  }

  // Strip frontmatter
  let body = page.body;
  if (body.startsWith('---')) {
    const parts = body.split('---');
    if (parts.length >= 3) {
      body = parts.slice(2).join('---').trim();
    }
  }

  return (
    <div>
      <Link
        to={`/w/${workspaceSlug}`}
        className="inline-flex items-center gap-1 text-sm text-[var(--color-text-tertiary)] hover:text-[var(--color-text-secondary)] mb-6 transition-colors"
      >
        <ChevronLeft size={14} />
        Wiki
      </Link>

      <article>
        <h1
          className="text-4xl font-bold mb-2 leading-tight"
          style={{ fontFamily: 'var(--font-serif)' }}
        >
          {page.title}
        </h1>
        {page.summary && (
          <p className="text-[var(--color-text-secondary)] mb-8">{page.summary}</p>
        )}
        <div className="prose">
          <ReactMarkdown remarkPlugins={[remarkGfm]}>{body}</ReactMarkdown>
        </div>
      </article>
    </div>
  );
}
