import { useEffect, useId, useRef, useState, type Ref } from 'react';
import { Bot, ChevronRight, ExternalLink, FileText, GitCommitHorizontal, GitPullRequest, Link2, X } from 'lucide-react';
import type { PageLineage as PageLineageModel } from '@/lib/page-lineage';
import { sourceFilename } from '@/lib/page-lineage';
import { sourceUrlFromDocument, splitSystemFrontmatter } from '@/lib/page-frontmatter';
import { openExternalUrl } from '@/external-links';
import './PageLineage.css';

export interface LineageSource { title?: string; content: string }
export type LoadLineageSource = (path: string) => Promise<LineageSource>;

export function PageLineage({ lineage, open, onToggle, panelId, triggerRef }: {
  lineage: PageLineageModel; open: boolean; onToggle: () => void; panelId: string; triggerRef?: Ref<HTMLButtonElement>;
}) {
  const who = lineage.agents.map(a => a.name).join(', ') || lineage.commit?.author;
  return <div className="page-lineage-entry not-prose">
    <button type="button" ref={triggerRef} onClick={onToggle} aria-expanded={open} aria-controls={panelId}>
      <Link2 size={17} aria-hidden />
      <span><strong>Sources &amp; records</strong><small>{lineage.sources.length} {lineage.sources.length === 1 ? 'source' : 'sources'}{who ? ` · Last contribution by ${who}` : ''}</small></span>
      <ChevronRight size={15} aria-hidden />
    </button>
  </div>;
}

export function PageLineagePanel({ lineage, panelId, onClose, onOpenSource, onOpenReview, loadSource }: {
  lineage: PageLineageModel; panelId: string; onClose: () => void;
  onOpenSource?: (path: string) => void; onOpenReview?: (id: string) => void; loadSource?: LoadLineageSource;
}) {
  const [tab, setTab] = useState<'sources' | 'records'>('sources');
  const [limit, setLimit] = useState(20);
  const closeRef = useRef<HTMLButtonElement>(null);
  const sourcesRef = useRef<HTMLButtonElement>(null);
  const recordsRef = useRef<HTMLButtonElement>(null);
  const id = useId();
  useEffect(() => { closeRef.current?.focus(); }, []);
  const records = !!lineage.commit || !!lineage.review || lineage.agents.length > 0;
  return <aside id={panelId} aria-label="Sources & records" className="page-lineage-panel not-prose"
    onKeyDown={event => { if (event.key === 'Escape') { event.stopPropagation(); onClose(); } }}>
    <header><h2>Sources &amp; records</h2><button ref={closeRef} type="button" aria-label="Close sources and records" onClick={onClose}><X size={18} /></button></header>
    <div role="tablist" aria-label="Source details" className="page-lineage-tabs" onKeyDown={event => {
      if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
      event.preventDefault();
      const next = event.key === 'Home' ? 'sources' : event.key === 'End' ? 'records' : tab === 'sources' ? 'records' : 'sources';
      setTab(next); (next === 'sources' ? sourcesRef : recordsRef).current?.focus();
    }}>
      <button ref={sourcesRef} type="button" role="tab" id={id + '-sources-tab'} aria-controls={id + '-sources'} aria-selected={tab === 'sources'} tabIndex={tab === 'sources' ? 0 : -1} onClick={() => setTab('sources')}>Sources <small>{lineage.sources.length}</small></button>
      <button ref={recordsRef} type="button" role="tab" id={id + '-records-tab'} aria-controls={id + '-records'} aria-selected={tab === 'records'} tabIndex={tab === 'records' ? 0 : -1} onClick={() => setTab('records')}>Records</button>
    </div>
    <section role="tabpanel" id={id + '-sources'} aria-labelledby={id + '-sources-tab'} hidden={tab !== 'sources'} className="page-lineage-body">
      <p className="lineage-caption">{lineage.sources.length ? 'Materials referenced by this page.' : 'No sources linked. Add Source references to the page frontmatter to keep its evidence connected.'}</p>
      {lineage.sources.slice(0, limit).map(path => <SourceRow key={path} path={path} loadSource={loadSource} onOpenSource={onOpenSource} />)}
      {lineage.sources.length > limit && <button type="button" className="lineage-text-action" onClick={() => setLimit(limit + 20)}>Show more sources</button>}
    </section>
    <section role="tabpanel" id={id + '-records'} aria-labelledby={id + '-records-tab'} hidden={tab !== 'records'} className="page-lineage-body">
      <p className="lineage-caption">{records ? 'Records attached to this version, not a complete page history.' : 'No recorded changes are available for this version.'}</p>
      <ol className="lineage-records">
        {lineage.review && <li><GitPullRequest size={17} aria-hidden /><div><h3>Reviewed change</h3>
          {onOpenReview ? <button type="button" className="lineage-text-action" onClick={() => onOpenReview(lineage.review!.id)}>Review: {lineage.review.title || `#${lineage.review.number}`} <ChevronRight size={13} /></button> : <p>{lineage.review.title || `Review #${lineage.review.number}`}</p>}
        </div></li>}
        {lineage.agents.map((agent, index) => <li key={agent.changeId + ':' + index}><Bot size={17} aria-hidden /><div><h3>{agent.name}</h3>{agent.task && <p>{agent.task}</p>}</div></li>)}
        {lineage.commit && <li><GitCommitHorizontal size={17} aria-hidden /><div><h3>{lineage.commit.author || 'Recorded contribution'}</h3><p>{lineage.commit.summary}</p><time>{formatTime(lineage.commit.committedAt)}</time></div></li>}
      </ol>
      {records && <details className="lineage-technical"><summary>Technical details</summary><dl>
        {lineage.commit && <><dt>Commit</dt><dd>{lineage.commit.oid}</dd></>}
        {lineage.review && <><dt>Review ID</dt><dd>{lineage.review.id}</dd></>}
        {lineage.agents.map((agent, index) => <div key={agent.changeId + ':' + index}><dt>{agent.name} change ID</dt><dd>{agent.changeId}</dd></div>)}
      </dl></details>}
    </section>
    <footer>Source references stay with the Markdown file.</footer>
  </aside>;
}

function formatTime(seconds: number) {
  const date = new Date(seconds * 1000);
  return Number.isFinite(date.getTime()) ? date.toLocaleString() : '';
}
function scalar(document: string, field: string): string {
  const line = splitSystemFrontmatter(document).systemFrontmatter.split(/\r?\n/).find(line => line.startsWith(field + ':'));
  const raw = line?.slice(field.length + 1).trim() ?? '';
  if (raw.startsWith('"')) { try { const value: unknown = JSON.parse(raw); return typeof value === 'string' ? value : ''; } catch { return ''; } }
  return raw.replace(/^'|'$/g, '');
}
function SourceRow({ path, loadSource, onOpenSource }: { path: string; loadSource?: LoadLineageSource; onOpenSource?: (path: string) => void }) {
  const [result, setResult] = useState<LineageSource | null>(null);
  const [error, setError] = useState('');
  const [attempt, setAttempt] = useState(0);
  // Callers may recreate callbacks when comment state changes. Only a mounted
  // source identity or an explicit retry starts another request.
  const loader = useRef(loadSource);
  useEffect(() => {
    let active = true;
    if (!loader.current) return;
    void loader.current(path).then(value => { if (active) { setResult(value); setError(''); } })
      .catch(cause => { if (active) setError(cause instanceof Error ? cause.message : 'Could not load this Source.'); });
    return () => { active = false; };
  }, [path, attempt]);
  const document = result?.content ?? '';
  const body = splitSystemFrontmatter(document).body;
  const title = result?.title || scalar(document, 'title') || body.match(/^#\s+(.+)$/m)?.[1] || sourceFilename(path).replace(/\.md$/i, '').replace(/[_-]+/g, ' ');
  const url = sourceUrlFromDocument(document);
  const capturedAt = scalar(document, 'captured_at');
  return <div className="lineage-source">
    <FileText aria-hidden size={17} />
    <div>
      <details><summary>{title}<ChevronRight size={14} aria-hidden /></summary>
        <div className="lineage-source-excerpt">{result ? body.slice(0, 600) || 'This Source has no text content.' : !loadSource ? 'Source content is not available in this view.' : error ? 'Source content could not be loaded.' : 'Loading Source…'}</div>
      </details>
      <p className="lineage-source-meta">{url ? new URL(url).hostname : 'Markdown Source'}{capturedAt ? ` · Captured ${capturedAt}` : ''}</p>
      {error && <div role="status" className="lineage-source-error">{error} <button type="button" onClick={() => { setError(''); setAttempt(attempt + 1); }}>Retry</button></div>}
      <div className="lineage-source-actions">
        {onOpenSource && <button type="button" onClick={() => onOpenSource(path)}>Open Source</button>}
        {url && <a href={url} target="_blank" rel="noreferrer" onClick={event => {
          event.preventDefault(); void openExternalUrl(url).catch(() => setError('Could not open the original URL.'));
        }}>View original <ExternalLink size={12} /></a>}
      </div>
      <details className="lineage-technical"><summary>Technical details</summary><dl><dt>Source path</dt><dd>{path}</dd>
        {scalar(document, 'content_hash') && <><dt>Content hash</dt><dd>{scalar(document, 'content_hash')}</dd></>}
        {scalar(document, 'extractor') && <><dt>Extractor</dt><dd>{scalar(document, 'extractor')}</dd></>}
      </dl></details>
    </div>
  </div>;
}
