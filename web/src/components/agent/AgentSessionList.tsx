import { Bot, GitBranch, LoaderCircle } from 'lucide-react';

import { isBusy, sessionGroups, shortAgo, statusLabel, type AgentSnapshot } from '@/lib/agent-session';
import { cn } from '@/lib/utils';

/** One row per session: what it was about, and when or what state it is in. */
export function AgentSessionRow({ session, active, onOpen }: { session: AgentSnapshot; active?: boolean; onOpen: () => void }) {
  const busy = isBusy(session.status);
  const notable = busy || session.status === 'waiting' || session.status === 'failed' || session.status === 'interrupted';
  return (
    <button
      type="button"
      onClick={onOpen}
      aria-current={active ? 'true' : undefined}
      className={cn(
        'flex w-full min-w-0 items-center gap-2 rounded-md px-2 py-1.5 text-left text-small transition-colors',
        active ? 'bg-bg-active/70 text-text' : 'text-text-secondary hover:bg-bg-hover hover:text-text',
      )}
    >
      {session.mode === 'background'
        ? <GitBranch className="size-3.5 shrink-0 text-text-faint" aria-hidden />
        : <Bot className="size-3.5 shrink-0 text-text-faint" aria-hidden />}
      <span className="min-w-0 flex-1 truncate">{session.title}</span>
      {notable ? (
        <span className={cn('flex shrink-0 items-center gap-1 text-caption', session.status === 'waiting' ? 'text-amber' : busy ? 'text-accent' : 'text-red')}>
          {busy && session.status !== 'waiting' && <LoaderCircle className="size-3 animate-spin" aria-hidden />}
          {statusLabel(session)}
        </span>
      ) : (
        <span className="shrink-0 text-caption tabular-nums text-text-faint">{shortAgo(session.updatedAt)}</span>
      )}
    </button>
  );
}

/** Every session of this Space, newest first, grouped by day. */
export function AgentSessionList({ sessions, activeId, onOpen }: {
  sessions: readonly AgentSnapshot[];
  activeId: string | null;
  onOpen: (sessionId: string) => void;
}) {
  const groups = sessionGroups(sessions);
  if (groups.length === 0) {
    return <p className="px-2 py-10 text-center text-small text-text-tertiary">No sessions yet.</p>;
  }
  return (
    <div className="flex flex-col gap-4">
      {groups.map((group) => (
        <section key={group.label} aria-label={group.label}>
          <h3 className="mb-0.5 px-2 text-meta font-medium text-text-tertiary">{group.label}</h3>
          {group.sessions.map((session) => (
            <AgentSessionRow
              key={session.sessionId}
              session={session}
              active={session.sessionId === activeId}
              onOpen={() => onOpen(session.sessionId)}
            />
          ))}
        </section>
      ))}
    </div>
  );
}
