import { useState, useCallback } from 'react';
import { Wand2, ArrowUpRight, RefreshCw } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { PageList } from '../components/PageList';
import { IngestForm } from '../components/IngestForm';
import { compile, submit, listPages } from '../api';

const BRANCH = 'user/default';

export function PersonalPage() {
  const [refreshKey, setRefreshKey] = useState(0);
  const [compiling, setCompiling] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [message, setMessage] = useState<{ text: string; type: 'info' | 'success' | 'error' } | null>(null);
  const [showIngest, setShowIngest] = useState(false);

  const refresh = useCallback(() => setRefreshKey((k) => k + 1), []);

  const handleCompile = async () => {
    setCompiling(true);
    setMessage(null);
    try {
      const res = await compile(BRANCH);
      setMessage({ text: `Compiled ${res.pages?.length || 0} page(s)`, type: 'success' });
      refresh();
    } catch {
      setMessage({ text: 'Compilation failed', type: 'error' });
    } finally {
      setCompiling(false);
    }
  };

  const handleSubmit = async () => {
    setSubmitting(true);
    setMessage(null);
    try {
      const pages = await listPages(BRANCH);
      if (pages.length === 0) {
        setMessage({ text: 'No pages to submit', type: 'info' });
        return;
      }
      const slugs = pages.map((p) => p.slug);
      const res = await submit(BRANCH, slugs);
      const dupCount = res.duplicates?.length || 0;
      setMessage({
        text: `Submitted for review.${dupCount > 0 ? ` ${dupCount} possible duplicate(s).` : ''}`,
        type: 'success',
      });
    } catch {
      setMessage({ text: 'Submit failed', type: 'error' });
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div>
      <h1 className="text-4xl font-bold mb-1" style={{ fontFamily: 'var(--font-serif)' }}>
        My Space
      </h1>
      <p className="text-muted-foreground text-sm mb-6">
        Draft pages and sources. Only you can see this.
      </p>

      <div className="flex items-center gap-2 mb-6">
        <Button variant="outline" size="sm" onClick={() => setShowIngest(!showIngest)}>
          + Add source
        </Button>
        <Button size="sm" onClick={handleCompile} disabled={compiling}>
          {compiling ? <RefreshCw className="mr-1.5 h-3.5 w-3.5 animate-spin" /> : <Wand2 className="mr-1.5 h-3.5 w-3.5" />}
          {compiling ? 'Compiling...' : 'Compile'}
        </Button>
        <Button variant="outline" size="sm" onClick={handleSubmit} disabled={submitting}>
          <ArrowUpRight className="mr-1.5 h-3.5 w-3.5" />
          {submitting ? 'Submitting...' : 'Submit to shared'}
        </Button>
      </div>

      {message && (
        <div className="mb-4">
          <Badge variant={message.type === 'error' ? 'destructive' : message.type === 'success' ? 'default' : 'secondary'}>
            {message.text}
          </Badge>
        </div>
      )}

      {showIngest && (
        <Card className="mb-6">
          <CardHeader className="pb-3">
            <CardTitle className="text-sm font-medium text-muted-foreground">Add source</CardTitle>
          </CardHeader>
          <CardContent>
            <IngestForm branch={BRANCH} onDone={() => { refresh(); setShowIngest(false); }} />
          </CardContent>
        </Card>
      )}

      <div className="border-t pt-4">
        <p className="text-xs font-medium text-muted-foreground uppercase tracking-wider mb-3">
          Draft pages
        </p>
        <PageList key={refreshKey} branch={BRANCH} />
      </div>
    </div>
  );
}
