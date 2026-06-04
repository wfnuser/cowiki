import { useState } from 'react';
import { Type, Link2 } from 'lucide-react';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Textarea } from '@/components/ui/textarea';
import { ingest } from '../api';

interface AddSourceDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  branch: string;
  workspaceName: string;
  workspaceSlug: string;
  onDone: () => void;
}

export function AddSourceDialog({
  open,
  onOpenChange,
  branch,
  workspaceName,
  workspaceSlug,
  onDone,
}: AddSourceDialogProps) {
  const [activeTab, setActiveTab] = useState<'text' | 'url'>('url');
  const [content, setContent] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');

  const handleOpenChange = (isOpen: boolean) => {
    if (!isOpen) {
      setContent('');
      setError('');
    }
    onOpenChange(isOpen);
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!content.trim() || loading) return;
    setLoading(true);
    setError('');
    try {
      await ingest(activeTab, content, branch, undefined, workspaceSlug);
      setContent('');
      setError('');
      onDone();
      onOpenChange(false);
    } catch (err) {
      setError(`Failed to add source: ${err instanceof Error ? err.message : 'Unknown error'}`);
    } finally {
      setLoading(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>
            Add Source to{' '}
            <span className="text-foreground/70 italic">{workspaceName}</span>
          </DialogTitle>
        </DialogHeader>

        <form onSubmit={handleSubmit} className="space-y-4">
          <Tabs
            value={activeTab}
            onValueChange={(v) => {
              setActiveTab(v as 'text' | 'url');
              setContent('');
              setError('');
            }}
          >
            <TabsList>
              <TabsTrigger value="text">
                <Type className="h-4 w-4" />
                Text
              </TabsTrigger>
              <TabsTrigger value="url">
                <Link2 className="h-4 w-4" />
                URL
              </TabsTrigger>
            </TabsList>

            <TabsContent value="text" className="mt-3">
              <Textarea
                value={content}
                onChange={(e) => setContent(e.target.value)}
                placeholder="Paste or type your content here..."
                rows={8}
              />
            </TabsContent>

            <TabsContent value="url" className="mt-3 space-y-1.5">
              <Input
                type="url"
                value={content}
                onChange={(e) => setContent(e.target.value)}
                placeholder="https://example.com/article"
              />
              <p className="text-xs text-muted-foreground">
                The page content will be automatically extracted and cleaned.
              </p>
            </TabsContent>
          </Tabs>

          {error && <p className="text-sm text-red-600">{error}</p>}

          <Button type="submit" disabled={!content.trim() || loading}>
            {loading ? 'Adding...' : 'Add Source'}
          </Button>
        </form>
      </DialogContent>
    </Dialog>
  );
}
