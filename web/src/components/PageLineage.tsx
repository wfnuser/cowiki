import { Bot, Database, GitCommitHorizontal, GitPullRequest, MoveRight } from 'lucide-react';
import type { ReactNode } from 'react';
import { C, fonts } from '@/lib/design';
import type { PageLineage as PageLineageModel } from '@/lib/page-lineage';

interface LineageStep {
  key: string;
  icon: ReactNode;
  eyebrow: string;
  label: string;
  title?: string;
  onClick?: () => void;
}

export function PageLineage({
  lineage,
  onOpenSource,
  onOpenReview,
}: {
  lineage: PageLineageModel;
  onOpenSource?: (path: string) => void;
  onOpenReview?: (id: string) => void;
}) {
  const steps: LineageStep[] = [
    ...lineage.sources.map((path) => ({
      key: `source:${path}`,
      icon: <Database size={13} />,
      eyebrow: 'Source',
      label: path.replace(/^\.cowiki\/sources\//, ''),
      onClick: onOpenSource ? () => onOpenSource(path) : undefined,
    })),
    ...lineage.agents.map((agent) => ({
      key: `agent:${agent.changeId}`,
      icon: <Bot size={13} />,
      eyebrow: 'Agent',
      label: agent.name,
      title: agent.task,
    })),
    ...(lineage.commit ? [{
      key: `commit:${lineage.commit.oid}`,
      icon: <GitCommitHorizontal size={13} />,
      eyebrow: 'Commit',
      label: lineage.commit.oid.slice(0, 7),
      title: `${lineage.commit.summary} · ${lineage.commit.author}`,
    }] : []),
    ...(lineage.review ? [{
      key: `review:${lineage.review.id}`,
      icon: <GitPullRequest size={13} />,
      eyebrow: 'Review',
      label: `#${lineage.review.number}`,
      title: lineage.review.title,
      onClick: onOpenReview ? () => onOpenReview(lineage.review!.id) : undefined,
    }] : []),
  ];
  if (!steps.length) return null;

  return (
    <section aria-label="Knowledge lineage" style={{ marginBottom: 24 }}>
      <div style={{ marginBottom: 8, color: C.faint, fontSize: 10, fontWeight: 700, letterSpacing: '0.08em', textTransform: 'uppercase' }}>
        Knowledge lineage
      </div>
      <div style={{ display: 'flex', alignItems: 'stretch', gap: 7, overflowX: 'auto', paddingBottom: 2 }}>
        {steps.map((step, index) => (
          <div key={step.key} style={{ display: 'flex', alignItems: 'center', gap: 7 }}>
            {index > 0 && <MoveRight aria-hidden size={13} color={C.faint} />}
            <button
              type="button"
              disabled={!step.onClick}
              onClick={step.onClick}
              title={step.title}
              style={{
                display: 'grid',
                gridTemplateColumns: 'auto minmax(0, 1fr)',
                columnGap: 7,
                alignItems: 'center',
                minWidth: 110,
                maxWidth: 210,
                padding: '7px 9px',
                border: `1px solid ${C.line}`,
                borderRadius: 8,
                background: C.panel,
                color: C.ink2,
                cursor: step.onClick ? 'pointer' : 'default',
                textAlign: 'left',
              }}
            >
              <span style={{ gridRow: '1 / span 2', display: 'grid', placeItems: 'center', color: C.accent }}>{step.icon}</span>
              <span style={{ color: C.faint, fontSize: 9, fontWeight: 700, letterSpacing: '0.06em', textTransform: 'uppercase' }}>{step.eyebrow}</span>
              <span style={{ overflow: 'hidden', color: C.ink2, fontFamily: step.eyebrow === 'Commit' ? fonts.mono : undefined, fontSize: 11.5, fontWeight: 600, textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{step.label}</span>
            </button>
          </div>
        ))}
      </div>
    </section>
  );
}
