import {
  Bot,
  ChevronDown,
  GitBranch,
  GitCompare,
  LoaderCircle,
  LogIn,
  MoreHorizontal,
  PanelRightClose,
  Plus,
  RefreshCw,
  Trash2,
} from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';

import { agentApi } from '@/agent-api';
import { createLocalAgentChange } from '@/api';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { InlineFeedback } from '@/components/ui/inline-feedback';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { agentDefinition, SUPPORTED_AGENTS, type AgentKind } from '@/lib/agents';
import {
  canResume,
  canSend,
  isBusy,
  shortVersion,
  statusLabel,
  type AgentMode,
  type AgentSnapshot,
  type NativeReadiness,
} from '@/lib/agent-session';
import { APP_HEADER_HEIGHT } from '@/lib/design';
import { cn } from '@/lib/utils';

import { AgentApprovalCard } from './AgentApprovalCard';
import { AgentComposer } from './AgentComposer';
import { AgentTranscript } from './AgentTranscript';
import { useAgentSessions, useNativeReadiness, type ProbeState } from './useAgentSessions';

export type AgentPanelOpenRequest =
  | {
    requestId: string;
    kind: 'new-change';
    agent: AgentKind;
    title: string;
    initialTask?: string;
  }
  | {
    requestId: string;
    kind: 'existing-change';
    agent: AgentKind;
    changeId: string;
    worktreePath: string;
  };

type Feedback = { title: string; description?: string; details?: string; tone?: 'error' | 'warning' | 'neutral' };

// Requests are handled once per app run, whichever Space panel sees them.
const handledRequests = new Set<string>();

const agentName = (agent: AgentKind) => agentDefinition(agent).displayName;

function errorText(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}

function titleFor(text: string): string {
  const line = text.trim().split('\n')[0] ?? '';
  return line.length > 60 ? `${line.slice(0, 57)}…` : line || 'New session';
}

export function readinessFeedback(agent: AgentKind, readiness: NativeReadiness): Feedback | null {
  const name = agentName(agent);
  switch (readiness.status) {
    case 'available':
      return null;
    case 'signedOut':
      return { title: `${name} is not signed in`, description: readiness.canSignIn ? 'Sign in, then send your message.' : readiness.message ?? undefined, tone: 'warning' };
    case 'notInstalled':
      return { title: `${name} isn’t available`, description: readiness.message ?? undefined };
    case 'incompatible':
      return { title: `${name} can’t run in the sidebar`, description: readiness.message ?? undefined, details: readiness.detail ?? undefined };
    case 'unsupported':
      return { title: `${name} isn’t supported in the sidebar yet`, description: 'Choose another Agent.', tone: 'neutral' };
    case 'broken':
      return { title: `${name} needs attention`, description: readiness.message ?? undefined, details: readiness.detail ?? undefined };
  }
}

function StatusPill({ snapshot }: { snapshot: AgentSnapshot }) {
  const busy = isBusy(snapshot.status);
  // A ready session needs no label; only states worth noticing show.
  if (snapshot.status === 'idle' && !snapshot.closed) return null;
  const tone = snapshot.closed
    ? 'text-text-tertiary'
    : snapshot.status === 'waiting'
      ? 'text-amber'
      : snapshot.status === 'failed' || snapshot.status === 'interrupted'
        ? 'text-red'
        : busy
          ? 'text-accent'
          : 'text-text-tertiary';
  return (
    <span className={cn('flex shrink-0 items-center gap-1.5 px-1 text-[11px] font-medium', tone)} role="status">
      {busy
        ? <LoaderCircle className="size-3 animate-spin" aria-hidden />
        : <span className="size-1.5 rounded-full bg-current" aria-hidden />}
      {statusLabel(snapshot)}
    </span>
  );
}

function ReadinessLine({ agent, state, onCheck, onSignIn, onCancelSignIn }: {
  agent: AgentKind;
  state: ProbeState;
  onCheck: () => void;
  onSignIn: () => void;
  onCancelSignIn: () => void;
}) {
  if (state.phase !== 'settled' || !state.readiness) {
    return (
      <p className="flex items-center gap-1.5 text-[11.5px] text-text-tertiary">
        <LoaderCircle className="size-3 animate-spin" aria-hidden /> Checking {agentName(agent)}…
      </p>
    );
  }
  const readiness = state.readiness;
  const feedback = readinessFeedback(agent, readiness);
  if (!feedback) {
    const version = shortVersion(readiness.version);
    return (
      <p className="flex min-w-0 items-center gap-1.5 text-[11.5px] text-text-tertiary" title={readiness.version ?? undefined}>
        <span className="size-1.5 shrink-0 rounded-full bg-green" aria-hidden />
        <span className="truncate">
          {[`${agentName(agent)}${version ? ` ${version}` : ''} is ready`, readiness.account].filter(Boolean).join(' · ')}
        </span>
      </p>
    );
  }
  return (
    <InlineFeedback
      compact
      tone={feedback.tone ?? 'error'}
      title={feedback.title}
      description={state.loginMessage ?? feedback.description}
      details={feedback.details}
      action={(
        <div className="flex gap-1">
          {readiness.status === 'signedOut' && readiness.canSignIn && (
            state.signingIn ? (
              <Button size="xs" variant="outline" onClick={onCancelSignIn}>Cancel sign-in</Button>
            ) : (
              <Button size="xs" variant="outline" onClick={onSignIn}><LogIn /> Sign in</Button>
            )
          )}
          <Button size="xs" variant="outline" onClick={onCheck}>Check again</Button>
        </div>
      )}
    />
  );
}

export function AgentPanel({
  spaceSlug,
  defaultAgent,
  openRequest,
  onOpenRequestHandled,
  onClose,
  onOpenChanges,
  className,
}: {
  spaceSlug: string;
  defaultAgent: AgentKind;
  openRequest?: AgentPanelOpenRequest | null;
  onOpenRequestHandled?: (requestId: string) => void;
  onClose?: () => void;
  onOpenChanges?: (changeId: string) => void;
  className?: string;
}) {
  const { sessions, ordered, ready, error: listError, forget } = useAgentSessions(spaceSlug);
  const readiness = useNativeReadiness();
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [composingNew, setComposingNew] = useState(false);
  const [draftAgent, setDraftAgent] = useState<AgentKind>(defaultAgent);
  const [draftMode, setDraftMode] = useState<AgentMode>('live');
  const [feedback, setFeedback] = useState<Feedback | null>(null);
  const [busyAction, setBusyAction] = useState(false);
  const busyRef = useRef(false);

  const shownId = selectedId && sessions.has(selectedId)
    ? selectedId
    : composingNew
      ? null
      : ordered[0]?.sessionId ?? null;
  const active = shownId ? sessions.get(shownId) ?? null : null;
  const draftState = readiness.stateFor(draftAgent);

  useEffect(() => {
    if (!active && draftState.phase === 'idle') void readiness.check(draftAgent);
  }, [active, draftAgent, draftState.phase, readiness]);

  const guard = useCallback(async <T,>(work: () => Promise<T>): Promise<T | undefined> => {
    if (busyRef.current) return undefined;
    busyRef.current = true;
    setBusyAction(true);
    setFeedback(null);
    try {
      return await work();
    } catch (cause) {
      setFeedback({ title: 'The Agent could not continue', description: errorText(cause) });
      return undefined;
    } finally {
      busyRef.current = false;
      setBusyAction(false);
    }
  }, []);

  /** Creates a session (and its Agent Change for background work). */
  const createSession = useCallback(async (
    agent: AgentKind,
    mode: AgentMode,
    title: string,
    changeId?: string,
  ): Promise<AgentSnapshot | null> => {
    const state = await readiness.check(agent);
    const problem = readinessFeedback(agent, state);
    if (problem) {
      setFeedback(problem);
      return null;
    }
    let boundChange = changeId ?? null;
    if (mode === 'background' && !boundChange) {
      const change = await createLocalAgentChange(spaceSlug, title, agentName(agent));
      boundChange = change.id;
    }
    const snapshot = await agentApi.create({ spaceSlug, mode, changeId: boundChange, agent, title });
    setSelectedId(snapshot.sessionId);
    setComposingNew(false);
    return snapshot;
  }, [readiness, spaceSlug]);

  const startTask = useCallback(async (text: string) => {
    const done = await guard(async () => {
      const snapshot = await createSession(draftAgent, draftMode, titleFor(text));
      if (!snapshot) return false;
      await agentApi.send(snapshot.sessionId, text);
      return true;
    });
    return done === true;
  }, [createSession, draftAgent, draftMode, guard]);

  const sendToActive = useCallback(async (text: string) => {
    if (!active) return false;
    const sessionId = active.sessionId;
    const done = await guard(async () => {
      await agentApi.send(sessionId, text);
      return true;
    });
    return done === true;
  }, [active, guard]);

  // Ingest and Review open the panel with a request; each is handled once.
  useEffect(() => {
    if (!openRequest || !ready) return;
    if (handledRequests.has(openRequest.requestId)) {
      onOpenRequestHandled?.(openRequest.requestId);
      return;
    }
    const request = openRequest;
    const task = window.setTimeout(() => {
      if (handledRequests.has(request.requestId)) return;
      handledRequests.add(request.requestId);
      void (async () => {
        try {
          if (request.kind === 'existing-change') {
            const existing = ordered.find((session) => session.changeId === request.changeId && !session.closed);
            if (existing) {
              setSelectedId(existing.sessionId);
              setComposingNew(false);
              return;
            }
            await guard(async () => {
              await createSession(request.agent, 'background', 'Continue Agent Change', request.changeId);
            });
            return;
          }
          await guard(async () => {
            const snapshot = await createSession(request.agent, 'background', request.title);
            if (snapshot && request.initialTask) await agentApi.send(snapshot.sessionId, request.initialTask);
          });
        } finally {
          onOpenRequestHandled?.(request.requestId);
        }
      })();
    }, 0);
    return () => window.clearTimeout(task);
  }, [createSession, guard, onOpenRequestHandled, openRequest, ordered, ready]);

  const answer = async (approvalId: string, decision: 'allow' | 'deny') => {
    if (!active) return;
    try {
      await agentApi.answer(active.sessionId, approvalId, decision);
    } catch (cause) {
      setFeedback({ title: 'The approval was not applied', description: errorText(cause) });
    }
  };

  const stop = () => {
    if (!active) return;
    void agentApi.stop(active.sessionId).catch((cause) => setFeedback({ title: 'Stop failed', description: errorText(cause) }));
  };

  const resume = () => {
    if (!active) return;
    void agentApi.resume(active.sessionId).catch((cause) => setFeedback({ title: 'Could not continue', description: errorText(cause) }));
  };

  const removeActive = () => {
    if (!active) return;
    const sessionId = active.sessionId;
    void agentApi.remove(sessionId)
      .then(() => {
        forget(sessionId);
        setSelectedId(null);
      })
      .catch((cause) => setFeedback({ title: 'Could not remove the session', description: errorText(cause) }));
  };

  const newSession = (mode: AgentMode) => {
    setDraftMode(mode);
    setSelectedId(null);
    setComposingNew(true);
    setFeedback(null);
  };

  const activeBusy = active ? isBusy(active.status) : false;
  const visibleSessions = ordered;

  return (
    <aside className={cn('flex h-full min-w-0 flex-col border-l border-border bg-bg', className)} aria-label="Agent">
      <header
        className="flex shrink-0 items-center gap-1 border-b border-border bg-bg-secondary pl-2 pr-1"
        style={{ height: APP_HEADER_HEIGHT, minHeight: APP_HEADER_HEIGHT }}
      >
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button
              type="button"
              className="flex min-w-0 max-w-full items-center gap-1.5 rounded-md px-1.5 py-1 text-left transition-colors hover:bg-bg-hover data-[state=open]:bg-bg-hover"
              aria-label="Choose Agent session"
            >
              <span className="min-w-0 truncate text-[13px] font-semibold text-text">
                {active ? active.title : 'New session'}
              </span>
              <ChevronDown className="size-3.5 shrink-0 text-text-tertiary" aria-hidden />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="w-72">
            <DropdownMenuItem onSelect={() => newSession('live')}><Plus /> New session</DropdownMenuItem>
            {visibleSessions.length > 0 && <DropdownMenuSeparator />}
            {visibleSessions.length > 0 && <DropdownMenuLabel className="text-[11px] font-medium text-text-tertiary">Recent in this Space</DropdownMenuLabel>}
            {visibleSessions.slice(0, 30).map((session) => (
              <DropdownMenuItem
                key={session.sessionId}
                className={cn(session.sessionId === active?.sessionId && 'bg-bg-hover')}
                onSelect={() => {
                  setSelectedId(session.sessionId);
                  setComposingNew(false);
                }}
              >
                {session.mode === 'background' ? <GitBranch className="text-text-tertiary" /> : <Bot className="text-text-tertiary" />}
                <span className="min-w-0 flex-1 truncate">{session.title}</span>
                {!(session.status === 'idle' && !session.closed) && (
                  <span className="shrink-0 text-[10.5px] text-text-tertiary">{statusLabel(session)}</span>
                )}
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
        <div className="flex-1" />
        {active && <StatusPill snapshot={active} />}
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button type="button" variant="ghost" size="icon-xs" className="text-text-tertiary" aria-label="Agent options">
              <MoreHorizontal />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-60">
            <DropdownMenuItem onSelect={() => newSession('live')}><Bot /> New session in Current Draft</DropdownMenuItem>
            <DropdownMenuItem onSelect={() => newSession('background')}><GitBranch /> New Agent Change</DropdownMenuItem>
            {active?.changeId && onOpenChanges && (
              <DropdownMenuItem onSelect={() => onOpenChanges(active.changeId!)}><GitCompare /> View changes</DropdownMenuItem>
            )}
            <DropdownMenuSeparator />
            <DropdownMenuItem onSelect={() => void readiness.check(draftAgent, true)}><RefreshCw /> Check {agentName(draftAgent)} again</DropdownMenuItem>
            {active && (
              <DropdownMenuItem disabled={activeBusy} onSelect={removeActive}><Trash2 /> Remove from history</DropdownMenuItem>
            )}
          </DropdownMenuContent>
        </DropdownMenu>
        {onClose && (
          <Button type="button" variant="ghost" size="icon-xs" className="text-text-tertiary" aria-label="Close agent panel" onClick={onClose}>
            <PanelRightClose />
          </Button>
        )}
      </header>

      {listError && (
        <InlineFeedback compact className="m-2" title="Agent sessions could not be loaded" description={listError} />
      )}

      {active ? (
        <>
          <AgentTranscript snapshot={active} />
          <div className="shrink-0 space-y-2 px-3 empty:hidden">
            {active.error && (
              <InlineFeedback compact title="The Agent stopped with an error" details={active.error} className="mt-2" />
            )}
            {active.notice && !active.error && (
              <InlineFeedback compact tone="neutral" title={active.notice} className="mt-2" />
            )}
            {feedback && (
              <InlineFeedback compact tone={feedback.tone ?? 'error'} title={feedback.title} description={feedback.description} details={feedback.details} className="mt-2" />
            )}
            {active.approvals.map((approval) => (
              <AgentApprovalCard key={approval.id} approval={approval} onAnswer={(decision) => answer(approval.id, decision)} />
            ))}
          </div>
          <AgentComposer
            placeholder={`Ask ${agentName(active.provider)}…`}
            disabledReason={active.closed ? 'This Agent Change is closed. Start a new session to keep working.' : null}
            busy={activeBusy || busyAction}
            stopping={active.status === 'stopping'}
            resumable={canResume(active)}
            meta={(
              <span className="flex min-w-0 items-center gap-1.5">
                <span className="shrink-0 text-text-secondary">{agentName(active.provider)}</span>
                {active.model && <span className="min-w-0 truncate" title={active.model}>{active.model}</span>}
                <span aria-hidden>·</span>
                <span className="shrink-0">{active.mode === 'background' ? 'Agent Change' : 'Current Draft'}</span>
                {active.changeId && onOpenChanges && (
                  <button type="button" className="shrink-0 text-accent hover:underline" onClick={() => onOpenChanges(active.changeId!)}>
                    View changes
                  </button>
                )}
              </span>
            )}
            onSend={(text) => (canSend(active) ? sendToActive(text) : Promise.resolve(false))}
            onStop={activeBusy ? stop : undefined}
            onResume={resume}
          />
        </>
      ) : (
        <>
          <div className="min-h-0 flex-1 overflow-y-auto px-4 pb-4 pt-5">
            <div className="flex flex-col gap-4">
              <div className="flex flex-col gap-1.5">
                <span className="text-[12px] font-medium text-text-secondary" id="agent-for-new-sessions">Agent</span>
                <Select
                  value={draftAgent}
                  onValueChange={(value) => {
                    setDraftAgent(value as AgentKind);
                    setFeedback(null);
                  }}
                >
                  <SelectTrigger className="w-full" aria-label="Agent for new sessions">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {SUPPORTED_AGENTS.map((agent) => <SelectItem key={agent} value={agent}>{agentName(agent)}</SelectItem>)}
                  </SelectContent>
                </Select>
              </div>
              <fieldset className="flex flex-col gap-1.5">
                <legend className="mb-1.5 text-[12px] font-medium text-text-secondary">Where it works</legend>
                <div className="grid grid-cols-2 gap-0.5 rounded-lg bg-bg-secondary p-0.5 ring-1 ring-inset ring-border-soft" role="radiogroup" aria-label="Where it works">
                  {(['live', 'background'] as const).map((mode) => (
                    <button
                      key={mode}
                      type="button"
                      role="radio"
                      aria-checked={draftMode === mode}
                      className={cn(
                        'h-7 rounded-md px-2 text-[12px] transition-colors',
                        draftMode === mode
                          ? 'bg-panel font-medium text-text shadow-[var(--shadow-chip)] ring-1 ring-border'
                          : 'text-text-tertiary hover:text-text-secondary',
                      )}
                      onClick={() => setDraftMode(mode)}
                    >
                      {mode === 'live' ? 'Current Draft' : 'New Agent Change'}
                    </button>
                  ))}
                </div>
                <p className="text-[11.5px] leading-[1.5] text-text-tertiary">
                  {draftMode === 'live'
                    ? 'Edits your Draft directly.'
                    : 'Works on a separate copy. You review the result before it reaches your Draft.'}
                </p>
              </fieldset>
              <div className="border-t border-border-soft pt-3">
                <ReadinessLine
                  agent={draftAgent}
                  state={draftState}
                  onCheck={() => void readiness.check(draftAgent, true)}
                  onSignIn={() => void readiness.signIn(draftAgent)}
                  onCancelSignIn={() => void readiness.cancelSignIn(draftAgent)}
                />
              </div>
              {feedback && (
                <InlineFeedback compact tone={feedback.tone ?? 'error'} title={feedback.title} description={feedback.description} details={feedback.details} />
              )}
            </div>
          </div>
          <AgentComposer
            placeholder={`Ask ${agentName(draftAgent)} to work in this Space…`}
            disabledReason={draftState.readiness && draftState.readiness.status !== 'available'
              ? `${agentName(draftAgent)} is not ready`
              : null}
            busy={busyAction}
            stopping={false}
            meta={(
              <span>
                <span className="text-text-secondary">{agentName(draftAgent)}</span>
                {' · '}{draftMode === 'background' ? 'New Agent Change' : 'Current Draft'}
              </span>
            )}
            onSend={startTask}
          />
        </>
      )}
    </aside>
  );
}
