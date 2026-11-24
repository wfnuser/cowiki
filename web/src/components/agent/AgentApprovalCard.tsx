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
      className="rounded-xl border border-border bg-panel p-3 shadow-[var(--shadow-card)]"
    >
      <div className="flex items-start gap-2">
        <span className="mt-px flex size-5 shrink-0 items-center justify-center rounded-md bg-amber-soft text-amber">
          <Icon className="size-3" aria-hidden />
        </span>
        <div className="min-w-0 flex-1">
          <p className="truncate text-small font-medium leading-5 text-text">{approval.title}</p>
          {approval.description && (
            <p className="text-meta leading-[1.5] text-text-secondary">{approval.description}</p>
          )}
        </div>
      </div>
      {approval.command && (
        <pre
          className="mt-2.5 max-h-28 overflow-auto whitespace-pre-wrap break-all rounded-lg bg-bg-secondary px-2.5 py-2 text-meta leading-relaxed text-text"
          style={{ fontFamily: fonts.mono }}
        >
          {approval.command}
        </pre>
      )}
      {approval.paths.length > 0 && (
        <ul className="mt-2.5 space-y-0.5 rounded-lg bg-bg-secondary px-2.5 py-2 text-meta text-text-secondary" style={{ fontFamily: fonts.mono }}>
          {approval.paths.slice(0, 8).map((path) => <li key={path} className="truncate">{path}</li>)}
          {approval.paths.length > 8 && <li>…and {approval.paths.length - 8} more</li>}
        </ul>
      )}
      <div className="mt-3 flex items-center gap-2">
        {approval.cwd && approval.kind === 'command' && (
          <p className="min-w-0 flex-1 truncate text-caption text-text-tertiary" title={approval.cwd}>in {approval.cwd}</p>
        )}
        <div className="ml-auto flex shrink-0 gap-1.5">
          <Button
            size="xs"
            variant="outline"
            className="h-7 px-3"
            disabled={pending !== null}
            aria-label={`Deny: ${approval.title}`}
            onClick={() => void answer('deny')}
          >
            Deny
          </Button>
          <Button
            size="xs"
            className="h-7 bg-accent px-3 text-on-accent hover:bg-accent-hover"
            disabled={pending !== null}
            aria-label={`Allow once: ${approval.title}`}
            onClick={() => void answer('allow')}
          >
            Allow once
          </Button>
        </div>
      </div>
    </section>
  );
}
