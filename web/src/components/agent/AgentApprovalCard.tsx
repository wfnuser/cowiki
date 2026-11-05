import { FileDiff, ShieldQuestion, SquareTerminal, Wrench } from 'lucide-react';
import { useState } from 'react';

import { Button } from '@/components/ui/button';
import { fonts } from '@/lib/design';
import type { AgentApproval } from '@/lib/agent-session';

const icons = { command: SquareTerminal, file: FileDiff, tool: Wrench };

/** One permission request. The command is shown for review only; CoWiki
 * never runs it itself. Each decision applies to this one action. */
export function AgentApprovalCard({
  approval,
  onAnswer,
}: {
  approval: AgentApproval;
  onAnswer: (decision: 'allow' | 'deny') => Promise<void>;
}) {
  const [pending, setPending] = useState<'allow' | 'deny' | null>(null);
  const Icon = icons[approval.kind] ?? ShieldQuestion;
  const answer = async (decision: 'allow' | 'deny') => {
    if (pending) return;
    setPending(decision);
    try {
      await onAnswer(decision);
    } finally {
      setPending(null);
    }
  };
  return (
    <section
      role="group"
      aria-label={`Approval needed: ${approval.title}`}
      className="rounded-lg border border-amber/30 bg-amber-soft/60 p-2.5"
    >
      <div className="flex items-center gap-1.5 text-xs font-semibold text-text">
        <Icon className="size-3.5 shrink-0 text-amber" aria-hidden />
        <span className="min-w-0 truncate">{approval.title}</span>
        <span className="ml-auto shrink-0 text-[10px] font-medium uppercase tracking-wide text-amber">Needs approval</span>
      </div>
      {approval.command && (
        <pre
          className="mt-2 max-h-28 overflow-auto whitespace-pre-wrap break-all rounded-md bg-bg px-2 py-1.5 text-[11px] text-text"
          style={{ fontFamily: fonts.mono }}
        >
          {approval.command}
        </pre>
      )}
      {approval.paths.length > 0 && (
        <ul className="mt-2 space-y-0.5 text-[11px] text-text-secondary" style={{ fontFamily: fonts.mono }}>
          {approval.paths.slice(0, 8).map((path) => <li key={path} className="truncate">{path}</li>)}
          {approval.paths.length > 8 && <li>…and {approval.paths.length - 8} more</li>}
        </ul>
      )}
      {approval.cwd && approval.kind === 'command' && (
        <p className="mt-1 truncate text-[10px] text-text-tertiary">in {approval.cwd}</p>
      )}
      {approval.description && (
        <p className="mt-1.5 text-[11px] leading-relaxed text-text-secondary">{approval.description}</p>
      )}
      <div className="mt-2 flex gap-1.5">
        <Button
          size="xs"
          className="bg-accent text-on-accent hover:bg-accent-hover"
          disabled={pending !== null}
          aria-label={`Allow once: ${approval.title}`}
          onClick={() => void answer('allow')}
        >
          Allow once
        </Button>
        <Button
          size="xs"
          variant="outline"
          disabled={pending !== null}
          aria-label={`Deny: ${approval.title}`}
          onClick={() => void answer('deny')}
        >
          Deny
        </Button>
      </div>
    </section>
  );
}
