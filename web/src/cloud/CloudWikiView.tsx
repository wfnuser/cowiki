import { useEffect, useMemo, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { PageReader } from '../components/PageReader';
import { splitSystemFrontmatter } from '../lib/page-frontmatter';
import type { CloudClient, CloudContent, CloudSpace, CloudTree } from './client';
import { resolveInitialCloudPage } from './cloud-shell-model';
import { CloudNotice } from './CloudHome';
import { CloudQuickSetup } from './CloudQuickSetup';
import { cloudSpaceRoute } from './routes';
import { pageLineage } from '../lib/page-lineage';
import { SourcePreviewDialog } from '../components/SourcePreviewDialog';

export function CloudWikiView({
  client,
  space,
  tree,
  treeError,
  unpublished,
  documentPath,
}: {
  client: CloudClient;
  space: CloudSpace;
  tree: CloudTree | null;
  treeError: string;
  unpublished: boolean;
  documentPath?: string;
}) {
  const navigate = useNavigate();
  const [content, setContent] = useState<CloudContent | null>(null);
  const [contentError, setContentError] = useState<{ path: string; message: string } | null>(null);
  const [sourcePath, setSourcePath] = useState('');
  const [sourceContent, setSourceContent] = useState<CloudContent | null>(null);
  const [sourceError, setSourceError] = useState('');
  const pages = useMemo(() => tree?.entries.filter((entry) => entry.kind === 'page') ?? [], [tree]);
  const currentContent = content?.path === documentPath ? content : null;
  const currentError = contentError?.path === documentPath ? contentError?.message ?? '' : '';

  useEffect(() => {
    if (!tree || documentPath || unpublished) return;
    const initial = resolveInitialCloudPage(tree.entries);
    if (initial) navigate(cloudSpaceRoute(space.id, 'wiki', initial), { replace: true });
  }, [documentPath, navigate, space.id, tree, unpublished]);

  useEffect(() => {
    if (!documentPath) return;
    let active = true;
    void client.getContent(space.id, documentPath)
      .then((next) => {
        if (!active) return;
        setContent(next);
        setContentError(null);
      })
      .catch((cause) => {
        if (active) {
          setContentError({
            path: documentPath,
            message: cause instanceof Error ? cause.message : 'Could not load this page.',
          });
        }
      });
    return () => { active = false; };
  }, [client, documentPath, space.id]);

  useEffect(() => {
    if (!sourcePath) return;
    let active = true;
    setSourceContent(null);
    setSourceError('');
    void client.getSourceContent(space.id, sourcePath)
      .then((next) => { if (active) setSourceContent(next); })
      .catch((cause) => {
        if (active) setSourceError(cause instanceof Error ? cause.message : 'Could not load this Source.');
      });
    return () => { active = false; };
  }, [client, sourcePath, space.id]);

  return (
    <div className="relative h-full min-h-0">
        {treeError ? (
          <div className="p-8"><CloudNotice tone="error">{treeError}</CloudNotice></div>
        ) : currentError ? (
          <div className="p-8"><CloudNotice tone="error">{currentError}</CloudNotice></div>
        ) : documentPath && !currentContent ? (
          <div className="p-10 text-sm text-text-tertiary">Loading page…</div>
        ) : currentContent ? (
          <PageReader
            body={splitSystemFrontmatter(currentContent.content).body}
            lineage={pageLineage(currentContent.content, currentContent.provenance)}
            onOpenSource={setSourcePath}
            onOpenReview={(id) => navigate(cloudSpaceRoute(space.id, 'reviews', id))}
          />
        ) : tree && pages.length === 0 ? (
          <CloudQuickSetup space={space} canPublish={space.role === 'owner'} />
        ) : null}
      <SourcePreviewDialog
        open={!!sourcePath}
        path={sourcePath}
        content={sourceContent?.path === sourcePath ? sourceContent.content : null}
        error={sourceError}
        onOpenChange={(open) => { if (!open) setSourcePath(''); }}
      />
    </div>
  );
}
