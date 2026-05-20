import { useEffect, useState } from 'react';
import { Check, X, ChevronLeft, GitPullRequest } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Separator } from '@/components/ui/separator';
import { listReviews, getReview, reviewAction, type Submission, type ReviewDetail } from '../api';

export function ReviewPage() {
  const [reviews, setReviews] = useState<Submission[]>([]);
  const [selected, setSelected] = useState<ReviewDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [actionLoading, setActionLoading] = useState(false);

  const load = () => {
    setLoading(true);
    listReviews().then(setReviews).finally(() => setLoading(false));
  };

  useEffect(() => { load(); }, []);

  const handleSelect = async (id: string) => {
    const detail = await getReview(id);
    setSelected(detail);
  };

  const handleAction = async (action: string) => {
    if (!selected) return;
    setActionLoading(true);
    try {
      await reviewAction(selected.submission.id, action);
      setSelected(null);
      load();
    } finally {
      setActionLoading(false);
    }
  };

  if (selected) {
    return (
      <div>
        <button
          onClick={() => setSelected(null)}
          className="inline-flex items-center gap-1 text-sm text-muted-foreground hover:text-foreground mb-6 transition-colors"
        >
          <ChevronLeft className="h-4 w-4" /> All reviews
        </button>

        <h1 className="text-2xl font-bold mb-1" style={{ fontFamily: 'var(--font-serif)' }}>
          Review #{selected.submission.id.slice(0, 8)}
        </h1>
        <p className="text-sm text-muted-foreground mb-1">{selected.submission.summary}</p>
        <div className="flex items-center gap-2 text-xs text-muted-foreground mb-6">
          <Badge variant="secondary">{selected.submission.page_slugs.length} file(s)</Badge>
          <span>{new Date(selected.submission.created_at).toLocaleString()}</span>
        </div>

        {/* Diffs */}
        <div className="space-y-4 mb-6">
          {selected.diffs.map((diff) => (
            <Card key={diff.path} className="overflow-hidden">
              <CardHeader className="py-2 px-4 bg-muted/50">
                <div className="flex items-center gap-2 text-xs font-mono">
                  <Badge variant={diff.old_content === null ? 'default' : 'secondary'} className="text-[10px] px-1.5 py-0">
                    {diff.old_content === null ? 'NEW' : 'MOD'}
                  </Badge>
                  {diff.path}
                </div>
              </CardHeader>
              <ScrollArea className="max-h-96">
                <pre className="p-4 text-xs leading-relaxed font-mono">
                  {diff.new_content?.split('\n').map((line, i) => {
                    const oldLines = diff.old_content?.split('\n') || [];
                    const isNew = diff.old_content === null;
                    const isChanged = !isNew && (i >= oldLines.length || oldLines[i] !== line);
                    return (
                      <div
                        key={i}
                        className={
                          isNew || isChanged
                            ? 'text-green-700 bg-green-50 dark:text-green-400 dark:bg-green-950/30'
                            : 'text-muted-foreground'
                        }
                      >
                        <span className="inline-block w-8 text-right pr-3 text-muted-foreground/50 select-none">
                          {i + 1}
                        </span>
                        {isNew || isChanged ? '+ ' : '  '}
                        {line}
                      </div>
                    );
                  })}
                </pre>
              </ScrollArea>
            </Card>
          ))}
        </div>

        <Separator className="mb-4" />

        <div className="flex gap-2">
          <Button onClick={() => handleAction('approve')} disabled={actionLoading} size="sm" className="bg-green-600 hover:bg-green-700">
            <Check className="mr-1.5 h-3.5 w-3.5" />
            {actionLoading ? 'Processing...' : 'Approve'}
          </Button>
          <Button variant="destructive" onClick={() => handleAction('reject')} disabled={actionLoading} size="sm">
            <X className="mr-1.5 h-3.5 w-3.5" /> Reject
          </Button>
        </div>
      </div>
    );
  }

  return (
    <div>
      <h1 className="text-4xl font-bold mb-1" style={{ fontFamily: 'var(--font-serif)' }}>
        Reviews
      </h1>
      <p className="text-muted-foreground text-sm mb-6">
        Submissions waiting for review before entering the shared wiki.
      </p>

      {loading ? (
        <div className="py-8 text-center text-muted-foreground text-sm">Loading...</div>
      ) : reviews.length === 0 ? (
        <div className="py-16 text-center">
          <GitPullRequest className="mx-auto h-6 w-6 text-muted-foreground mb-3" />
          <p className="text-muted-foreground text-sm">No pending reviews</p>
        </div>
      ) : (
        <div>
          {reviews.map((r) => (
            <button
              key={r.id}
              onClick={() => handleSelect(r.id)}
              className="w-full text-left flex items-start gap-3 px-2 py-3 -mx-2 rounded-md hover:bg-accent transition-colors"
            >
              <GitPullRequest className="mt-0.5 h-4 w-4 shrink-0 text-orange-500" />
              <div className="min-w-0 flex-1">
                <div className="text-sm">{r.page_slugs.length} page(s) from {r.source_branch}</div>
                <div className="text-xs text-muted-foreground mt-0.5">{r.summary}</div>
              </div>
              <Badge variant="outline" className="shrink-0 text-[10px]">{r.status}</Badge>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
