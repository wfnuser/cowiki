import { FitAddon } from '@xterm/addon-fit';
import { Terminal } from '@xterm/xterm';
import '@xterm/xterm/css/xterm.css';
import { Bot, ChevronDown, FileText, GitBranch, History, LoaderCircle, PanelRightClose, Plus, RotateCcw, SquareTerminal, X } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';

import { Button } from '@/components/ui/button';
import { InlineFeedback } from '@/components/ui/inline-feedback';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { cn } from '@/lib/utils';
import { SUPPORTED_AGENTS } from '@/lib/agents';
import { createLocalAgentChange } from '@/api';
import { APP_HEADER_HEIGHT, fonts, terminalTheme } from '@/lib/design';

import {
  agentDisplayName,
  agentReadinessAction,
  type AgentReadiness,
  type AgentKind,
  type AgentTerminalIntent,
  type AgentTerminalMode,
} from './terminal-contract';
import {
  addAgentTab,
  addOrFocusCodexLoginTab,
  closeAgentTab,
  openOrActivateAgentChangeTab,
  terminalTabLabel,
  type AgentTerminalTab,
  type AgentTerminalTabsState,
} from './terminal-tabs';
import { useAgentTerminal } from './useAgentTerminal';
import { type AgentProbeState, useAgentReadiness } from './useAgentReadiness';

type AgentTerminalPanelProps = {
  spacePath: string;
  spaceSlug: string;
  defaultAgent: AgentKind;
  openRequest?: AgentPanelOpenRequest | null;
  onOpenRequestHandled?: (requestId: string) => void;
  onClose?: () => void;
  className?: string;
};

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

let terminalTabSequence = 0;

type OpenAgent = (
  agent: AgentKind,
  mode?: AgentTerminalMode,
  title?: string,
  initialTask?: string,
  intent?: AgentTerminalIntent,
) => Promise<boolean>;

type LaunchFeedback = {
  title: string;
  description?: string;
  details?: string;
};

function nextTerminalTabId(agent: AgentKind): string {
  terminalTabSequence += 1;
  return `${agent}-${Date.now()}-${terminalTabSequence}`;
}

export function AgentTerminalPanel({
  spacePath,
  spaceSlug,
  defaultAgent,
  openRequest,
  onOpenRequestHandled,
  onClose,
  className,
}: AgentTerminalPanelProps) {
  const [tabState, setTabState] = useState<AgentTerminalTabsState>({
    activeTabId: null,
    tabs: [],
  });
  const [launchFeedback, setLaunchFeedback] = useState<Partial<Record<AgentKind, LaunchFeedback>>>({});
  const [pendingLaunches, setPendingLaunches] = useState<ReadonlySet<string>>(new Set());
  const pendingLaunchesRef = useRef(new Set<string>());
  const { check: checkReadiness, stateFor: readinessStateFor } = useAgentReadiness();
  const handledOpenRequestRef = useRef<string | null>(null);

  const checkAgent = useCallback((agent: AgentKind, force = false) => {
    if (force) setLaunchFeedback((current) => ({ ...current, [agent]: undefined }));
    return checkReadiness(agent, force);
  }, [checkReadiness]);

  const openAgent = useCallback(async (
    agent: AgentKind,
    mode: AgentTerminalMode = 'live',
    title?: string,
    initialTask?: string,
    intent: AgentTerminalIntent = 'run',
  ) => {
    const launchKey = `${agent}:${mode}:${intent}`;
    if (pendingLaunchesRef.current.has(launchKey)) return false;
    pendingLaunchesRef.current.add(launchKey);
    setPendingLaunches(new Set(pendingLaunchesRef.current));
    setLaunchFeedback((current) => ({ ...current, [agent]: undefined }));
    try {
      if (intent === 'login') {
        if (agent !== 'codex') throw new Error('Only Codex supports Agent sign-in');
        setTabState((state) => addOrFocusCodexLoginTab(state, nextTerminalTabId('codex')));
        return true;
      }
      const readinessResult = await checkReadiness(agent);
      const readinessAction = agentReadinessAction(agent, readinessResult.status);
      if (readinessAction === 'login') {
        setTabState((state) => addOrFocusCodexLoginTab(state, nextTerminalTabId('codex')));
        return true;
      }
      if (readinessAction === 'blocked') {
        setLaunchFeedback((current) => ({
          ...current,
          [agent]: readinessFeedback(agent, readinessResult),
        }));
        return false;
      }
      if (mode === 'live') {
        setTabState((state) => addAgentTab(state, agent, nextTerminalTabId(agent), mode));
        return true;
      }
      const change = await createLocalAgentChange(
        spaceSlug,
        title || agentDisplayName(agent),
        agentDisplayName(agent),
      );
      setTabState((state) => addAgentTab(
        state,
        agent,
        nextTerminalTabId(agent),
        mode,
        { changeId: change.id, worktreePath: change.worktreePath, initialTask },
      ));
      return true;
    } catch (cause) {
      setLaunchFeedback((current) => ({
        ...current,
        [agent]: {
          title: `Could not start ${agentDisplayName(agent)}`,
          description: 'Check the CLI and try again.',
          details: cause instanceof Error ? cause.message : String(cause),
        },
      }));
      return false;
    } finally {
      pendingLaunchesRef.current.delete(launchKey);
      setPendingLaunches(new Set(pendingLaunchesRef.current));
    }
  }, [checkReadiness, spaceSlug]);

  useEffect(() => {
    if (!openRequest) return;
    if (handledOpenRequestRef.current === openRequest.requestId) return;
    if (openRequest.kind === 'existing-change') {
      const task = window.setTimeout(() => {
        if (handledOpenRequestRef.current === openRequest.requestId) return;
        handledOpenRequestRef.current = openRequest.requestId;
        setTabState((state) => openOrActivateAgentChangeTab(
          state,
          openRequest.agent,
          nextTerminalTabId(openRequest.agent),
          {
            changeId: openRequest.changeId,
            worktreePath: openRequest.worktreePath,
          },
        ));
        onOpenRequestHandled?.(openRequest.requestId);
      }, 0);
      return () => window.clearTimeout(task);
    }
    const task = window.setTimeout(() => {
      if (handledOpenRequestRef.current === openRequest.requestId) return;
      handledOpenRequestRef.current = openRequest.requestId;
      void openAgent(openRequest.agent, 'background', openRequest.title, openRequest.initialTask)
        .finally(() => onOpenRequestHandled?.(openRequest.requestId));
    }, 0);
    return () => window.clearTimeout(task);
  }, [onOpenRequestHandled, openAgent, openRequest]);

  const closeTab = (tabId: string) => {
    setTabState((state) => closeAgentTab(state, tabId));
  };

  return (
    <aside className={cn('flex h-full min-w-0 flex-col border-l border-border bg-bg', className)}>
      <header
        className="flex shrink-0 items-center border-b border-border bg-bg-secondary pl-1.5 pr-1"
        style={{ height: APP_HEADER_HEIGHT, minHeight: APP_HEADER_HEIGHT }}
      >
        <div className="flex min-w-0 flex-1 items-center gap-0.5 overflow-x-auto">
          {tabState.tabs.map((tab) => {
            const active = tab.id === tabState.activeTabId;
            return (
              <div
                key={tab.id}
                className={cn(
                  'group flex h-9 min-w-0 max-w-40 shrink-0 items-center gap-1.5 rounded-md border px-2 text-xs',
                  active
                    ? 'border-border bg-text text-on-accent'
                    : 'border-transparent text-text-tertiary hover:bg-panel/65 hover:text-text',
                )}
              >
                <button
                  type="button"
                  className="flex min-w-0 flex-1 items-center gap-1.5"
                  onClick={() => setTabState((state) => ({ ...state, activeTabId: tab.id }))}
                >
                  <Bot className={cn('size-3.5 shrink-0', active ? 'text-accent' : 'text-text-tertiary')} />
                  <span className="truncate">{terminalTabLabel(tabState.tabs, tab)}</span>
                </button>
                <button
                  type="button"
                  aria-label={`Close ${terminalTabLabel(tabState.tabs, tab)}`}
                  className={cn(
                    'rounded p-0.5 hover:bg-on-accent/15',
                    active ? 'text-on-accent/55 hover:text-on-accent' : 'text-text-faint hover:bg-text/5 hover:text-text',
                  )}
                  onClick={() => closeTab(tab.id)}
                >
                  <X className="size-3" />
                </button>
              </div>
            );
          })}
          <NewViewMenu
            onOpenAgent={openAgent}
            stateFor={readinessStateFor}
            feedback={launchFeedback}
            pendingLaunches={pendingLaunches}
            onCheck={(agent) => checkAgent(agent, true)}
          />
        </div>
        {onClose && (
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            className="ml-1 text-text-tertiary"
            aria-label="Collapse agent panel"
            onClick={onClose}
          >
            <PanelRightClose />
          </Button>
        )}
      </header>

      <div className="relative min-h-0 flex-1">
        {tabState.tabs.length === 0 ? (
          <AgentLaunchPage
            defaultAgent={defaultAgent}
            onOpenAgent={openAgent}
            stateFor={readinessStateFor}
            feedback={launchFeedback}
            pendingLaunches={pendingLaunches}
            onCheck={checkAgent}
          />
        ) : (
          tabState.tabs.map((tab) => (
            <AgentTerminalInstance
              key={tab.id}
              tab={tab}
              spacePath={spacePath}
              spaceSlug={spaceSlug}
              active={tab.id === tabState.activeTabId}
              onLoginFinished={tab.intent === 'login'
                ? () => { void checkAgent(tab.agent, true); }
                : undefined}
            />
          ))
        )}
      </div>
    </aside>
  );
}

function AgentLaunchPage({
  defaultAgent,
  onOpenAgent,
  stateFor,
  feedback,
  pendingLaunches,
  onCheck,
}: {
  defaultAgent: AgentKind;
  onOpenAgent: OpenAgent;
  stateFor: (agent: AgentKind) => AgentProbeState;
  feedback: Partial<Record<AgentKind, LaunchFeedback>>;
  pendingLaunches: ReadonlySet<string>;
  onCheck: (agent: AgentKind, force?: boolean) => Promise<AgentReadiness>;
}) {
  const [selectedAgent, setSelectedAgent] = useState(defaultAgent);
  const probeState = stateFor(selectedAgent);
  const readinessResult = probeState.readiness;
  const checking = probeState.phase !== 'settled';
  const signingIn = pendingLaunches.has(`${selectedAgent}:live:login`);
  const starting = pendingLaunches.has(`${selectedAgent}:live:run`);

  useEffect(() => {
    if (probeState.phase === 'idle') void onCheck(selectedAgent);
  }, [onCheck, probeState.phase, selectedAgent]);

  const unavailable = readinessResult
    && (readinessResult.status === 'notInstalled' || readinessResult.status === 'broken')
    ? readinessFeedback(selectedAgent, readinessResult)
    : null;
  const inlineFeedback = feedback[selectedAgent] ?? unavailable;
  const signedOut = readinessResult?.status === 'signedOut';
  const available = readinessResult?.status === 'available';
  const busy = checking || starting || signingIn;
  const buttonLabel = checking
    ? `Checking ${agentDisplayName(selectedAgent)}…`
    : starting
      ? `Starting ${agentDisplayName(selectedAgent)}…`
      : signingIn
        ? `Opening sign-in…`
        : available
          ? `Start ${agentDisplayName(selectedAgent)}`
          : signedOut
            ? `Sign in to ${agentDisplayName(selectedAgent)}`
            : readinessResult?.status === 'notInstalled'
              ? `${agentDisplayName(selectedAgent)} not found`
              : `${agentDisplayName(selectedAgent)} needs attention`;

  return (
    <div className="flex h-full flex-col items-center justify-center px-7 text-center">
      <div className="mb-4 flex size-12 items-center justify-center rounded-xl border border-accent/20 bg-accent-soft">
        <Bot className="size-6 text-accent" />
      </div>
      <h2 className="text-base font-semibold text-text">Work with an agent</h2>
      <p className="mt-1.5 max-w-64 text-xs leading-relaxed text-text-tertiary">
        Choose a local Agent CLI and start it in the Current Draft.
      </p>
      <AgentPicker selectedAgent={selectedAgent} onSelectAgent={setSelectedAgent} />
      <Button
        className="mt-3 w-full max-w-72 bg-accent text-on-accent hover:bg-accent-hover"
        disabled={busy || (!available && !signedOut)}
        onClick={() => void onOpenAgent(
          selectedAgent,
          'live',
          undefined,
          undefined,
          signedOut ? 'login' : 'run',
        )}
      >
        {busy && <LoaderCircle className="size-4 animate-spin" />}
        {buttonLabel}
      </Button>
      {signedOut && !inlineFeedback && (
        <p className="mt-2 max-w-72 text-[11px] leading-relaxed text-text-tertiary">
          Sign-in runs inside the official CLI. CoWiki does not read or store its credentials.
        </p>
      )}
      {inlineFeedback && (
        <InlineFeedback
          className="mt-2 w-full max-w-72"
          compact
          title={inlineFeedback.title}
          description={inlineFeedback.description}
          details={inlineFeedback.details}
          action={(
            <Button size="xs" variant="outline" onClick={() => void onCheck(selectedAgent, true)}>
              Check again
            </Button>
          )}
        />
      )}
    </div>
  );
}

function AgentPicker({
  selectedAgent,
  onSelectAgent,
}: {
  selectedAgent: AgentKind;
  onSelectAgent: (agent: AgentKind) => void;
}) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          className="mt-5 flex w-full max-w-72 items-center rounded-lg border border-border bg-panel px-3 py-2.5 text-left hover:bg-bg-hover"
        >
          <Bot className="mr-2 size-4 text-accent" />
          <span>
            <span className="block text-[10px] font-medium uppercase tracking-wide text-text-tertiary">Agent</span>
            <span className="block text-xs font-semibold text-text">{agentDisplayName(selectedAgent)}</span>
          </span>
          <ChevronDown className="ml-auto size-3.5 text-text-tertiary" />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="center" className="w-72">
        <DropdownMenuLabel className="text-xs text-text-tertiary">Choose agent</DropdownMenuLabel>
        {SUPPORTED_AGENTS.map((agent) => (
          <DropdownMenuItem key={agent} onSelect={() => onSelectAgent(agent)}>
            <Bot /> {agentDisplayName(agent)}
            {agent === selectedAgent && <span className="ml-auto text-[10px] text-accent">Selected</span>}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function readinessFeedback(agent: AgentKind, readiness: AgentReadiness): LaunchFeedback {
  const name = agentDisplayName(agent);
  const details = [readiness.message, readiness.detail]
    .filter((value, index, values): value is string => Boolean(value) && values.indexOf(value) === index)
    .join('\n');
  if (readiness.status === 'notInstalled') {
    return {
      title: `${name} isn’t available`,
      description: `Install ${name}, make it available in your login shell, then check again.`,
      details: details || undefined,
    };
  }
  return {
    title: `${name} needs attention`,
    description: 'Check that the CLI can start normally, then try again.',
    details: details || undefined,
  };
}

function NewViewMenu({
  onOpenAgent,
  stateFor,
  feedback,
  pendingLaunches,
  onCheck,
}: {
  onOpenAgent: OpenAgent;
  stateFor: (agent: AgentKind) => AgentProbeState;
  feedback: Partial<Record<AgentKind, LaunchFeedback>>;
  pendingLaunches: ReadonlySet<string>;
  onCheck: (agent: AgentKind) => Promise<AgentReadiness>;
}) {
  const [open, setOpen] = useState(false);
  const [attemptedAgent, setAttemptedAgent] = useState<AgentKind | null>(null);

  const attemptOpen = async (agent: AgentKind, mode: AgentTerminalMode) => {
    setAttemptedAgent(agent);
    const opened = await onOpenAgent(agent, mode);
    if (opened) setOpen(false);
  };

  const itemLabel = (agent: AgentKind, mode: AgentTerminalMode) => {
    const checking = stateFor(agent).phase === 'checking';
    const pending = pendingLaunches.has(`${agent}:${mode}:run`);
    return (
      <>
        {checking || pending
          ? <LoaderCircle className="animate-spin" />
          : mode === 'background' ? <GitBranch /> : <Bot />}
        {checking ? `Checking ${agentDisplayName(agent)}…` : agentDisplayName(agent)}
      </>
    );
  };
  const isAgentBusy = (agent: AgentKind, mode: AgentTerminalMode) => (
    stateFor(agent).phase === 'checking' || pendingLaunches.has(`${agent}:${mode}:run`)
  );

  const attemptedFeedback = attemptedAgent ? feedback[attemptedAgent] : undefined;
  return (
    <DropdownMenu open={open} onOpenChange={setOpen}>
      <DropdownMenuTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          className="shrink-0 text-text-tertiary"
          aria-label="Open agent or view"
        >
          <Plus />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-48">
        <DropdownMenuLabel className="text-xs text-text-tertiary">Current Draft</DropdownMenuLabel>
        {SUPPORTED_AGENTS.map((agent) => (
          <DropdownMenuItem
            key={`live-${agent}`}
            disabled={isAgentBusy(agent, 'live')}
            onSelect={(event) => {
              event.preventDefault();
              void attemptOpen(agent, 'live');
            }}
          >
            {itemLabel(agent, 'live')}
          </DropdownMenuItem>
        ))}
        <DropdownMenuSeparator />
        <DropdownMenuLabel className="text-xs text-text-tertiary">New Agent Change</DropdownMenuLabel>
        {SUPPORTED_AGENTS.map((agent) => (
          <DropdownMenuItem
            key={`background-${agent}`}
            disabled={isAgentBusy(agent, 'background')}
            onSelect={(event) => {
              event.preventDefault();
              void attemptOpen(agent, 'background');
            }}
          >
            {itemLabel(agent, 'background')}
          </DropdownMenuItem>
        ))}
        {attemptedFeedback && attemptedAgent && (
          <div className="px-1.5 py-1">
            <InlineFeedback
              compact
              title={attemptedFeedback.title}
              description={attemptedFeedback.description}
              details={attemptedFeedback.details}
              action={(
                <Button size="xs" variant="outline" onClick={() => void onCheck(attemptedAgent)}>
                  Check again
                </Button>
              )}
            />
          </div>
        )}
        <DropdownMenuSeparator />
        <DropdownMenuLabel className="text-xs text-text-tertiary">Views</DropdownMenuLabel>
        <DropdownMenuItem disabled><FileText /> Files <span className="ml-auto text-[10px]">Soon</span></DropdownMenuItem>
        <DropdownMenuItem disabled><History /> History <span className="ml-auto text-[10px]">Soon</span></DropdownMenuItem>
        <DropdownMenuItem disabled><SquareTerminal /> Terminal <span className="ml-auto text-[10px]">Soon</span></DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function AgentTerminalInstance({
  tab,
  spacePath,
  spaceSlug,
  active,
  onLoginFinished,
}: {
  tab: AgentTerminalTab;
  spacePath: string;
  spaceSlug: string;
  active: boolean;
  onLoginFinished?: () => void;
}) {
  const cwd = tab.mode === 'background' ? tab.worktreePath! : spacePath;
  const containerRef = useRef<HTMLDivElement | null>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const activeRef = useRef(active);

  useEffect(() => {
    activeRef.current = active;
  }, [active]);

  const handleData = useCallback((data: string) => {
    terminalRef.current?.write(data);
  }, []);

  const { error, resize, start, status, write } = useAgentTerminal({
    cwd,
    mode: tab.mode,
    spaceSlug,
    changeId: tab.changeId,
    agent: tab.agent,
    intent: tab.intent,
    initialTask: tab.initialTask,
    onData: handleData,
    onExit: (exitCode) => {
      terminalRef.current?.writeln(
        `\r\n[${agentDisplayName(tab.agent)} exited${exitCode == null ? '' : `: ${exitCode}`}]`,
      );
      if (tab.intent === 'login') onLoginFinished?.();
    },
  });

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const terminal = new Terminal({
      allowProposedApi: false,
      convertEol: true,
      cursorBlink: true,
      fontFamily: fonts.mono,
      fontSize: 13,
      scrollback: 10_000,
      theme: terminalTheme,
    });
    const fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);
    terminal.open(container);
    terminalRef.current = terminal;
    fitAddonRef.current = fitAddon;

    const fit = () => {
      if (!activeRef.current) return;
      fitAddon.fit();
      void resize(terminal.cols, terminal.rows);
    };
    const resizeObserver = new ResizeObserver(fit);
    resizeObserver.observe(container);
    const inputDisposable = terminal.onData((data) => void write(data));
    fit();

    return () => {
      resizeObserver.disconnect();
      inputDisposable.dispose();
      terminal.dispose();
      terminalRef.current = null;
      fitAddonRef.current = null;
    };
  }, [resize, write]);

  useEffect(() => {
    const terminal = terminalRef.current;
    if (!terminal) return;
    terminal.writeln(`${tab.intent === 'login' ? 'Opening sign-in for' : 'Starting'} ${agentDisplayName(tab.agent)} in ${cwd}…`);
    fitAddonRef.current?.fit();
    void start({ cols: terminal.cols, rows: terminal.rows });
  }, [cwd, start, tab.agent, tab.intent]);

  useEffect(() => {
    if (!active) return;
    const terminal = terminalRef.current;
    fitAddonRef.current?.fit();
    if (terminal) void resize(terminal.cols, terminal.rows);
    terminal?.focus();
  }, [active, resize]);

  useEffect(() => {
    if (!error) return;
    terminalRef.current?.writeln(`\r\n[Unable to start ${agentDisplayName(tab.agent)}: ${error}]`);
  }, [error, tab.agent]);

  return (
    <section
      className="absolute inset-0 min-h-0 flex-col bg-text"
      style={{ display: active ? 'flex' : 'none' }}
    >
      <div className="flex h-7 shrink-0 items-center border-b border-on-accent/8 px-2.5 text-[10px] text-on-accent/35">
        <span>{status}</span>
        <span className="ml-2 shrink-0 text-on-accent/55">
          {tab.intent === 'login'
            ? 'Agent access'
            : tab.mode === 'background'
              ? 'Background Change'
              : 'Current Draft'}
        </span>
        <span className="ml-2 truncate">{cwd}</span>
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          className="ml-auto text-on-accent/45 hover:bg-on-accent/10 hover:text-on-accent"
          aria-label={`Restart ${agentDisplayName(tab.agent)}`}
          onClick={() => {
            const terminal = terminalRef.current;
            terminal?.reset();
            void start({ cols: terminal?.cols ?? 80, rows: terminal?.rows ?? 24 });
          }}
        >
          <RotateCcw />
        </Button>
      </div>
      <div ref={containerRef} className="min-h-0 flex-1 px-2 py-1" />
    </section>
  );
}
