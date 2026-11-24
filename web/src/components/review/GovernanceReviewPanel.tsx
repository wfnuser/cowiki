import { useEffect, useRef, useState } from 'react';
import { ArrowLeft, ChevronRight, GitPullRequest, RefreshCw } from 'lucide-react';

import type { AgentChange } from '@/api';
import { Button } from '@/components/ui/button';
import { InlineFeedback } from '@/components/ui/inline-feedback';
import { ViewEmpty } from '@/components/ui/view-header';
import { reviewGovernance, stageGovernanceAsNewChange } from '@/local-api';
import { DiffView } from './DiffView';
import {
  formatCitation, toggleRepair, type GovernanceCitation, type GovernanceFinding, type GovernanceReview,
} from './governance-model';

const severityStyle = {
  critical: 'bg-red-soft text-red',
  warning: 'bg-amber-soft text-amber',
  info: 'bg-secondary text-text-secondary',
};

function Severity({ value }: { value: GovernanceFinding['severity'] }) {
  return <span className={`rounded px-2 py-0.5 text-xs font-medium capitalize ${severityStyle[value]}`}>{value}</span>;
}

function CitationRow({ label, citation, excerpt, fallback }: {
  label: string;
  citation: GovernanceCitation | null;
  excerpt: string | null;
  fallback: string;
}) {
  return (
    <>
      <dt className="text-text-tertiary">{label}</dt>
      <dd className="min-w-0">
        <span className="break-all font-mono text-xs">{citation ? formatCitation(citation) : fallback}</span>
        {excerpt && <p className="mt-0.5 whitespace-pre-wrap break-words text-text-secondary">{excerpt}</p>}
      </dd>
    </>
  );
}

export function GovernanceReviewPanel({ spaceSlug, changeId, onStaged, onOpenChange }: {
  spaceSlug: string;
  changeId: string | null;
  onStaged?: () => void;
  onOpenChange: (id: string) => void;
}) {
  const [revision, setRevision] = useState(0);
  const [report, setReport] = useState<GovernanceReview | null>(null);
  const [findingId, setFindingId] = useState<string | null>(null);
  const [selected, setSelected] = useState<string[]>([]);
  const [stagedIds, setStagedIds] = useState<string[]>([]);
  const [staging, setStaging] = useState(false);
  const stageLock = useRef(false);
  const [staged, setStaged] = useState<AgentChange | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let current = true;
    reviewGovernance(spaceSlug, changeId)
      .then(value => { if (current) setReport(value); })
      .catch(cause => { if (current) setError(String(cause)); });
    return () => { current = false; };
  }, [spaceSlug, changeId, revision]);

  function refresh() {
    setReport(null);
    setFindingId(null);
    setSelected([]);
    setStaged(null);
    setStagedIds([]);
    setError(null);
    setRevision(n => n + 1);
  }

  async function stage() {
    if (!report?.canStage || !selected.length || stageLock.current) return;
    stageLock.current = true;
    setStaging(true);
    setError(null);
    try {
      const change = await stageGovernanceAsNewChange(spaceSlug, changeId, report.asOf, selected);
      setStaged(change);
      setStagedIds(ids => [...ids, ...selected]);
      setSelected([]);
      setFindingId(null);
      onStaged?.();
    } catch (cause) {
      setError(String(cause));
    } finally {
      stageLock.current = false;
      setStaging(false);
    }
  }

  const finding = report?.findings.find(f => f.id === findingId);
  const choices = report?.proposals.filter(p => finding ? p.findingIds.includes(finding.id) : !p.findingIds.length) ?? [];
  return (
    <section aria-label="Governance audit" className="mt-4 space-y-4">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold">Governance audit</h2>
          {report && <p className="mt-1 text-xs text-text-tertiary">{report.findings.length} finding{report.findings.length !== 1 && 's'} · {report.proposals.length} proposed repair{report.proposals.length !== 1 && 's'} · {report.asOf}</p>}
        </div>
        <Button size="sm" variant="ghost" disabled={staging} onClick={refresh}><RefreshCw size={14} />Refresh audit</Button>
      </header>
      {error && <InlineFeedback title="Governance review could not complete" description={error} />}
      {report && report.warnings.length > 0 && <InlineFeedback tone="warning" title="Some checks need human review" details={report.warnings.join('\n')} />}
      {staged && <InlineFeedback tone="success" title="Repairs staged in a new Agent Change"
        description={`${staged.title} · ${staged.id.slice(0, 8)} · Ready for review`}
        action={<Button size="sm" variant="outline" onClick={() => onOpenChange(staged.id)}>Open change</Button>} />}
      {!report && !error && <ViewEmpty>Reading the local snapshot…</ViewEmpty>}
      {report && !finding && (
        report.findings.length ? <ul aria-label="Findings" className="divide-y divide-border border-y border-border">
          {report.findings.map(item => {
            const repairs = report.proposals.filter(p => p.findingIds.includes(item.id));
            const label = repairs.some(p => selected.includes(p.id)) ? 'Selected'
              : repairs.some(p => stagedIds.includes(p.id)) ? 'Staged' : repairs.length ? 'Repair available' : null;
            return (
              <li key={item.id}>
                <button
                  type="button"
                  onClick={() => setFindingId(item.id)}
                  className="flex w-full items-center gap-3 px-1 py-3 text-left hover:bg-bg-hover/60"
                >
                  <span className="min-w-0 flex-1">
                    <span className="flex flex-wrap items-center gap-2">
                      <Severity value={item.severity} />
                      <code className="text-xs">{item.code}</code>
                      {item.verdict && <span className="text-xs text-text-tertiary">Offline rules · {item.verdict.replaceAll('_', ' ')}</span>}
                    </span>
                    <span className="mt-1 block text-sm">{item.message}</span>
                    <span className="block break-all text-xs text-text-tertiary">{item.location ? formatCitation(item.location) : item.path}</span>
                  </span>
                  {label && <span className="text-xs text-text-secondary">{label}</span>}
                  <ChevronRight size={16} className="shrink-0 text-text-faint" />
                </button>
              </li>
            );
          })}
        </ul> : <ViewEmpty>No findings in this snapshot.</ViewEmpty>
      )}
      {finding && (
        <div className="space-y-3">
          <Button variant="ghost" size="sm" onClick={() => setFindingId(null)}><ArrowLeft size={14} />All findings</Button>
          <div className="rounded-lg border border-border p-4">
            <div className="flex items-center gap-2"><Severity value={finding.severity} /><code className="text-xs">{finding.code}</code></div>
            <h3 className="my-2 text-base font-medium">{finding.message}</h3>
            <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-sm">
              <CitationRow label="Candidate" citation={finding.location} excerpt={finding.candidate_excerpt} fallback={finding.path} />
              <CitationRow label="Evidence" citation={finding.evidence} excerpt={finding.evidence_excerpt} fallback="No separate source citation" />
              <dt className="text-text-tertiary">Verifier</dt><dd>{finding.verdict ? `Offline rules · ${finding.verdict.replaceAll('_', ' ')}` : 'Structural audit'}</dd>
            </dl>
          </div>
          {!choices.length && <p className="text-sm text-text-secondary">This finding needs human review.</p>}
        </div>
      )}
      {choices.map(p => (
        <section key={p.id} aria-label={`Proposed repair for ${p.diff.path}`} className="space-y-2">
          <div className="flex flex-wrap items-start justify-between gap-2">
            <div><h3 className="text-sm font-semibold">Proposed repair</h3><p className="text-sm text-text-secondary">{p.rationale}</p></div>
            {report?.canStage && <label className="flex cursor-pointer items-center gap-2 text-sm">
              <input type="checkbox" disabled={staging || stagedIds.includes(p.id)} checked={selected.includes(p.id) || stagedIds.includes(p.id)}
                onChange={() => setSelected(ids => toggleRepair(ids, p.id))} />{stagedIds.includes(p.id) ? 'Staged' : 'Select repair'}
            </label>}
          </div>
          <DiffView diffs={[p.diff]} />
        </section>
      ))}
      {report && (selected.length > 0 || finding) && <footer className="flex flex-wrap items-center justify-between gap-2 border-t border-border py-3">
        {finding && <Button variant="ghost" size="sm" disabled={staging} onClick={() => {
          setSelected(ids => ids.filter(id => !choices.some(p => p.id === id)));
          setFindingId(null);
        }}>Leave for human review</Button>}
        {selected.length > 0 && <Button className="ml-auto" size="sm" disabled={staging} onClick={() => void stage()}>
          <GitPullRequest size={14} />{staging ? 'Staging…' : `Stage ${selected.length} repair${selected.length === 1 ? '' : 's'} as new change`}
        </Button>}
      </footer>}
    </section>
  );
}
