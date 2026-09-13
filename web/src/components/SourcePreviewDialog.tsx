import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { ExternalLink } from 'lucide-react';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { splitSystemFrontmatter } from '@/lib/page-frontmatter';
import { sourceOriginalUrl } from '@/lib/page-lineage';

export function SourcePreviewDialog({
  open,
  path,
  content,
  error,
  onOpenChange,
}: {
  open: boolean;
  path: string;
  content: string | null;
  error: string;
  onOpenChange: (open: boolean) => void;
}) {
  const originalUrl = content ? sourceOriginalUrl(content) : null;
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[72vh] overflow-hidden sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle className="truncate pr-8">{path.replace(/^\.cowiki\/sources\//, '')}</DialogTitle>
          <DialogDescription className="sr-only">Captured Source content and original URL</DialogDescription>
        </DialogHeader>
        {originalUrl && (
          <a
            href={originalUrl}
            target="_blank"
            rel="noreferrer"
            className="inline-flex w-fit items-center gap-1.5 text-xs font-semibold text-accent hover:underline"
          >
            View original <ExternalLink size={12} />
          </a>
        )}
        <div className="min-h-32 overflow-auto rounded-lg border border-border bg-bg p-5">
          {error ? (
            <p className="text-sm text-red">{error}</p>
          ) : content ? (
            <article className="prose max-w-none">
              <ReactMarkdown remarkPlugins={[remarkGfm]}>
                {splitSystemFrontmatter(content).body}
              </ReactMarkdown>
            </article>
          ) : (
            <p className="text-sm text-text-tertiary">Loading Source…</p>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
