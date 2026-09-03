import type { Ref, ReactNode } from 'react';
import ReactMarkdown, { type Components } from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { C } from '@/lib/design';
import { PageByline } from './PageByline';
import { PageLineage } from './PageLineage';
import type { PageLineage as PageLineageModel } from '@/lib/page-lineage';
import { htmlMarkdownComponents } from './HtmlView';

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
  lineage?: PageLineageModel;
  onOpenSource?: (path: string) => void;
  onOpenReview?: (id: string) => void;
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
  lineage,
  onOpenSource,
  onOpenReview,
}: PageReaderProps) {
  return (
    <div style={{ position: 'absolute', inset: 0, display: 'flex', alignItems: 'stretch' }}>
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
            {lineage && (
              <PageLineage
                lineage={lineage}
                onOpenSource={onOpenSource}
                onOpenReview={onOpenReview}
              />
            )}
            <ReactMarkdown
              remarkPlugins={[remarkGfm]}
              components={{ ...htmlMarkdownComponents, ...markdownComponents }}
            >
              {body}
            </ReactMarkdown>
          </>
        )}
      </article>
      {aside}
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
