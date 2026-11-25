import { useEffect, useId, useLayoutEffect, useMemo, useRef, type Ref, type ReactNode } from 'react';
import ReactMarkdown, { type Components } from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { C } from '@/lib/design';
import { PageByline } from './PageByline';
import { PageLineage, PageLineagePanel, type LoadLineageSource } from './PageLineage';
import { useReaderLineagePanel } from './PageCommentsLayer';
import type { PageLineage as PageLineageModel } from '@/lib/page-lineage';
import { withHtmlMarkdownComponents } from './HtmlView';
import { renderedClick, type RenderedClick } from '@/lib/click-to-edit';
import { remarkSoftBreaks } from '@/lib/remark-soft-breaks';

const REMARK = [remarkGfm, remarkSoftBreaks];


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
  /**
   * Click to edit: called with the clicked text (null for blank space) and how
   * far below the top of the visible page the clicked character sits, so the
   * editor can open with it in the same place. Links and controls never edit.
   */
  onEditAt?: (click: RenderedClick | null, top: number) => void;
  /** Scroll position to open at, e.g. where the editor was when editing stopped. */
  initialScrollTop?: number;
  /** 1-based line inside `body` to scroll to and briefly highlight (e.g. jumping from Tasks). */
  targetLine?: { line: number; seq: number };
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
  onEditAt,
  initialScrollTop,
  targetLine,
}: PageReaderProps) {
  const column = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const scroller = column.current?.parentElement;
    if (initialScrollTop && scroller) scroller.scrollTop = initialScrollTop;
    // Only where the page opens.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  const targetLineNum = targetLine?.line;
  const targetSeq = targetLine?.seq;
  useEffect(() => {
    if (targetLineNum === undefined || !column.current) return;
    const nodes = [...column.current.querySelectorAll<HTMLElement>('[data-source-line]')];
    if (nodes.length === 0) return;
    const exact = nodes.find((el) => Number(el.dataset.sourceLine) === targetLineNum);
    const containing = exact ?? nodes.find((el) => {
      const start = Number(el.dataset.sourceLine);
      const end = Number(el.dataset.endLine ?? start);
      return start <= targetLineNum && targetLineNum <= end;
    });
    const chosen = containing ?? nodes.reduce((best, el) => (
      Math.abs(Number(el.dataset.sourceLine) - targetLineNum)
        < Math.abs(Number(best.dataset.sourceLine) - targetLineNum) ? el : best
    ));
    chosen.scrollIntoView({ block: 'center' });
    chosen.classList.add('reader-target-line');
    const timer = window.setTimeout(() => chosen.classList.remove('reader-target-line'), 1600);
    return () => {
      window.clearTimeout(timer);
      chosen.classList.remove('reader-target-line');
    };
  }, [targetLineNum, targetSeq, body]);
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
        className={onEditAt ? 'prose page-reader-editable' : 'prose'}
        style={{ flex: 1, minWidth: 0, overflow: 'auto' }}
        onClick={onEditAt ? (event) => {
          const click = renderedClick(event.nativeEvent, event.currentTarget);
          if (click === undefined) return;
          const top = (click?.clientTop ?? event.clientY) - event.currentTarget.getBoundingClientRect().top;
          onEditAt(click, top);
        } : undefined}
      >
        <div ref={column} className="page-column">
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
            <div className="doc-body">
              <ReactMarkdown remarkPlugins={REMARK} components={components}>
                {body}
              </ReactMarkdown>
            </div>
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
        </div>
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
  borderRadius: 8,
  background: C.sidebar,
  color: C.ink2,
  fontSize: 13,
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
