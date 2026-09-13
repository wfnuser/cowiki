import { useId, useRef, type Ref, type ReactNode } from 'react';
import ReactMarkdown, { type Components } from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { C } from '@/lib/design';
import { PageByline } from './PageByline';
import { PageLineage, PageLineagePanel, type LoadLineageSource } from './PageLineage';
import { useReaderLineagePanel } from './PageCommentsLayer';
import type { PageLineage as PageLineageModel } from '@/lib/page-lineage';
import { withHtmlMarkdownComponents } from './HtmlView';
import { useMemo } from 'react';

interface PageReaderProps {
  body: string;
  articleRef?: Ref<HTMLElement>;
  markdownComponents?: Components;
  byline?: {
    name?: string | null;
    editedAt?: number | null;
  };
  readOnlyLabel?: string;
  readOnlyDotColor?: string;
  missingMessage?: string;
  aside?: ReactNode;
  toolbar?: ReactNode;
  lineage?: PageLineageModel;
  onOpenSource?: (path: string) => void;
  onOpenReview?: (id: string) => void;
  loadSource?: LoadLineageSource;
}

export function PageReader({
  body,
  articleRef,
  markdownComponents,
  byline,
  readOnlyLabel,
  readOnlyDotColor = C.blue,
  missingMessage,
  aside,
  toolbar,
  lineage,
  onOpenSource,
  onOpenReview,
  loadSource,
}: PageReaderProps) {
  const [lineageOpen, setLineageOpen] = useReaderLineagePanel();
  const lineageId = useId();
  const lineageTrigger = useRef<HTMLButtonElement>(null);
  const closeLineage = () => { setLineageOpen(false); lineageTrigger.current?.focus(); };
  const components = useMemo(() => withHtmlMarkdownComponents(markdownComponents), [markdownComponents]);
  return (
    <div className="page-reader" style={{ position: 'absolute', inset: 0, display: 'flex', flexDirection: 'column' }}>
      {toolbar && <div style={{ height: 44, flexShrink: 0, display: 'flex', alignItems: 'center', justifyContent: 'flex-end', padding: '0 20px' }}>{toolbar}</div>}
      <div style={{ position: 'relative', display: 'flex', alignItems: 'stretch', flex: 1, minHeight: 0 }}>
      <article
        ref={articleRef}
        className="prose"
        style={{ flex: 1, minWidth: 0, overflow: 'auto', padding: '36px 48px 56px 56px' }}
      >
        {readOnlyLabel && (
          <div style={readOnlyBannerStyle}>
            <span
              aria-hidden
              style={{
                width: 8,
                height: 8,
                borderRadius: '50%',
                background: readOnlyDotColor,
                flexShrink: 0,
              }}
            />
            <span>{readOnlyLabel} · <span style={{ color: C.muted }}>read-only</span></span>
          </div>
        )}
        {missingMessage ? (
          <div style={missingStyle}>{missingMessage}</div>
        ) : (
          <>
            {byline && <PageByline name={byline.name} editedAt={byline.editedAt} />}
            <ReactMarkdown
              remarkPlugins={[remarkGfm]}
              components={components}
            >
              {body}
            </ReactMarkdown>
            {lineage && (
              <PageLineage
                lineage={lineage}
                open={lineageOpen}
                onToggle={() => setLineageOpen(!lineageOpen)}
                panelId={lineageId}
                triggerRef={lineageTrigger}
              />
            )}
          </>
        )}
      </article>
      {lineage && lineageOpen && !missingMessage && <PageLineagePanel
        lineage={lineage} panelId={lineageId} onClose={closeLineage}
        onOpenSource={onOpenSource} onOpenReview={onOpenReview} loadSource={loadSource}
      />}
      {aside}
      </div>
    </div>
  );
}

const readOnlyBannerStyle: React.CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  gap: 9,
  marginBottom: 24,
  padding: '10px 14px',
  border: `1px solid ${C.line}`,
  borderRadius: 9,
  background: C.sidebar,
  color: C.ink2,
  fontSize: 12.5,
};

const missingStyle: React.CSSProperties = {
  padding: '34px 18px',
  border: `1px dashed ${C.line}`,
  borderRadius: 10,
  background: C.panel,
  color: C.muted,
  textAlign: 'center',
  fontSize: 13,
};

export default PageReader;
