import { useState } from 'react';
import { Type, Link2, FileUp, X } from 'lucide-react';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Textarea } from '@/components/ui/textarea';
import { ingest } from '../api';
import { isDesktopClient } from '../runtime';
import { chooseSourceFiles, ingestFiles } from '../local-api';

interface AddSourceDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  branch: string;
  workspaceName: string;
  workspaceSlug: string;
  onDone: () => void;
}

type SourceTab = 'text' | 'url' | 'file';

export function AddSourceDialog({
  open,
  onOpenChange,
  branch,
  workspaceName,
  workspaceSlug,
  onDone,
}: AddSourceDialogProps) {
  const desktop = isDesktopClient();
  const [activeTab, setActiveTab] = useState<SourceTab>('url');
  const [content, setContent] = useState('');
  const [selectedFiles, setSelectedFiles] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');

  const reset = () => {
    setContent('');
    setSelectedFiles([]);
    setError('');
  };

  const handleOpenChange = (isOpen: boolean) => {
    if (!isOpen) reset();
    onOpenChange(isOpen);
  };

  const handleChooseFiles = async () => {
    const picked = await chooseSourceFiles();
    if (picked.length === 0) return;
    setSelectedFiles((current) => [...new Set([...current, ...picked])]);
    setError('');
  };

  const fileName = (path: string) => path.split(/[\\/]/).pop() || path;

  const handleSubmitFiles = async () => {
    setLoading(true);
    setError('');
    try {
      const outcomes = await ingestFiles(workspaceSlug, selectedFiles);
      const failed = outcomes.filter((outcome) => outcome.error);
      if (failed.length === outcomes.length) {
        setError(failed.map((outcome) => `${fileName(outcome.sourcePath)}: ${outcome.error}`).join('; '));
        return;
      }
      if (failed.length > 0) {
        setError(`${failed.length} of ${outcomes.length} file(s) failed: ${failed
          .map((outcome) => `${fileName(outcome.sourcePath)} (${outcome.error})`)
          .join(', ')}`);
      }
      reset();
      onDone();
      if (failed.length === 0) onOpenChange(false);
    } catch (err) {
      setError(`Failed to add sources: ${err instanceof Error ? err.message : 'Unknown error'}`);
    } finally {
      setLoading(false);
    }
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (loading) return;
    if (activeTab === 'file') {
      if (selectedFiles.length === 0) return;
      await handleSubmitFiles();
      return;
    }
    if (!content.trim()) return;
    setLoading(true);
    setError('');
    try {
      await ingest(activeTab, content, branch, undefined, workspaceSlug);
      reset();
      onDone();
      onOpenChange(false);
    } catch (err) {
      setError(`Failed to add source: ${err instanceof Error ? err.message : 'Unknown error'}`);
    } finally {
      setLoading(false);
    }
  };

  const canSubmit = activeTab === 'file' ? selectedFiles.length > 0 : !!content.trim();

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
              setActiveTab(v as SourceTab);
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
              {desktop && (
                <TabsTrigger value="file">
                  <FileUp className="h-4 w-4" />
                  File
                </TabsTrigger>
              )}
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
                The URL is kept as an OKF source. Codex or Claude can read it and organize durable knowledge.
              </p>
            </TabsContent>

            {desktop && (
              <TabsContent value="file" className="mt-3 space-y-3">
                <Button type="button" variant="outline" onClick={() => { void handleChooseFiles(); }} className="w-full">
                  <FileUp className="h-4 w-4" />
                  Choose files…
                </Button>
                {selectedFiles.length > 0 && (
                  <ul className="max-h-40 space-y-1 overflow-y-auto rounded-md border p-1.5">
                    {selectedFiles.map((path) => (
                      <li key={path} className="flex items-center justify-between gap-2 rounded px-2 py-1 text-sm hover:bg-muted/50">
                        <span className="truncate">{fileName(path)}</span>
                        <button
                          type="button"
                          onClick={() => setSelectedFiles((current) => current.filter((p) => p !== path))}
                          className="shrink-0 text-muted-foreground hover:text-foreground"
                          aria-label={`Remove ${fileName(path)}`}
                        >
                          <X className="h-3.5 w-3.5" />
                        </button>
                      </li>
                    ))}
                  </ul>
                )}
                <p className="text-xs text-muted-foreground">
                  PDF, Word, PowerPoint, and spreadsheet files are converted to text on import. Unsupported formats are reported per file.
                </p>
              </TabsContent>
            )}
          </Tabs>

          {error && <p className="text-sm text-red-600">{error}</p>}

          <Button type="submit" disabled={!canSubmit || loading}>
            {loading
              ? 'Adding...'
              : activeTab === 'file' && selectedFiles.length > 1
                ? `Add ${selectedFiles.length} Sources`
                : 'Add Source'}
          </Button>
        </form>
      </DialogContent>
    </Dialog>
  );
}
