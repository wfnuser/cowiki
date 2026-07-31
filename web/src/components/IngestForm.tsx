import { useState } from 'react';
import { Plus, Link2, Type } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Textarea } from '@/components/ui/textarea';
import { ingest } from '../api';

export function IngestForm({ branch, onDone, workspaceSlug }: { branch: string; onDone: () => void; workspaceSlug: string }) {
  const [sourceType, setSourceType] = useState<'text' | 'url'>('text');
  const [content, setContent] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!content.trim()) return;
    setLoading(true);
    setError('');
    try {
      await ingest(sourceType, content, branch, undefined, workspaceSlug);
      setContent('');
      onDone();
    } catch (err) {
      setError(`Failed to add source: ${err instanceof Error ? err.message : 'Unknown error'}`);
    } finally {
      setLoading(false);
    }
  };

  return (
    <form onSubmit={handleSubmit} className="space-y-3">
      <div className="flex gap-1">
        <Button
          type="button"
          variant={sourceType === 'text' ? 'secondary' : 'ghost'}
          size="sm"
          onClick={() => setSourceType('text')}
        >
          <Type className="mr-1.5 h-3 w-3" /> Text
        </Button>
        <Button
          type="button"
          variant={sourceType === 'url' ? 'secondary' : 'ghost'}
          size="sm"
          onClick={() => setSourceType('url')}
        >
          <Link2 className="mr-1.5 h-3 w-3" /> URL
        </Button>
      </div>

      {sourceType === 'url' ? (
        <Input
          type="url"
          value={content}
          onChange={(e) => setContent(e.target.value)}
          placeholder="Paste a URL..."
        />
      ) : (
        <Textarea
          value={content}
          onChange={(e) => setContent(e.target.value)}
          placeholder="Paste or type your content here..."
          rows={5}
        />
      )}

      <Button type="submit" disabled={loading || !content.trim()} size="sm">
        <Plus className="mr-1.5 h-3.5 w-3.5" />
        {loading ? 'Adding...' : 'Add source'}
      </Button>
      {error && (
        <p className="text-xs text-red-600">{error}</p>
      )}
    </form>
  );
}
