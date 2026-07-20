import { FitAddon } from '@xterm/addon-fit';
import { Terminal } from '@xterm/xterm';
import '@xterm/xterm/css/xterm.css';
import { Bot, FileText, GitBranch, History, PanelRightClose, Plus, RotateCcw, SquareTerminal, X } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';

import { Button } from '@/components/ui/button';
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
import { APP_HEADER_HEIGHT } from '@/lib/design';

import {
  agentDisplayName,
  type AgentKind,
  type AgentTerminalMode,
} from './terminal-contract';
import {
  addAgentTab,
  closeAgentTab,
  terminalTabLabel,
  type AgentTerminalTab,
  type AgentTerminalTabsState,
} from './terminal-tabs';
import { useAgentTerminal } from './useAgentTerminal';

type AgentTerminalPanelProps = {
  spacePath: string;
  spaceSlug: string;
  defaultAgent: AgentKind;
  onClose?: () => void;
  className?: string;
};

let terminalTabSequence = 0;

function nextTerminalTabId(agent: AgentKind): string {
  terminalTabSequence += 1;
  return `${agent}-${Date.now()}-${terminalTabSequence}`;
}

export function AgentTerminalPanel({
  spacePath,
  spaceSlug,
  defaultAgent,
  onClose,
  className,
}: AgentTerminalPanelProps) {
  const [tabState, setTabState] = useState<AgentTerminalTabsState>({
    activeTabId: null,
    tabs: [],
  });
  const [launchError, setLaunchError] = useState<string | null>(null);

  const openAgent = async (agent: AgentKind, mode: AgentTerminalMode = 'live') => {
    setLaunchError(null);
    try {
      if (mode === 'live') {
        setTabState((state) => addAgentTab(state, agent, nextTerminalTabId(agent), mode));
        return;
      }
      const change = await createLocalAgentChange(spaceSlug, agentDisplayName(agent));
      setTabState((state) => addAgentTab(
        state,
        agent,
        nextTerminalTabId(agent),
        mode,
        { changeId: change.id, worktreePath: change.worktreePath },
      ));
    } catch (cause) {
      setLaunchError(cause instanceof Error ? cause.message : String(cause));
    }
  };

  const closeTab = (tabId: string) => {
    setTabState((state) => closeAgentTab(state, tabId));
  };

  return (
    <aside className={cn('flex h-full min-w-0 flex-col border-l border-border bg-[#faf9f7]', className)}>
      <header
        className="flex shrink-0 items-center border-b border-border bg-[#f5f4f1] pl-1.5 pr-1"
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
                    ? 'border-border bg-[#1d1c1a] text-white'
                    : 'border-transparent text-text-tertiary hover:bg-white/65 hover:text-text',
                )}
              >
                <button
                  type="button"
                  className="flex min-w-0 flex-1 items-center gap-1.5"
                  onClick={() => setTabState((state) => ({ ...state, activeTabId: tab.id }))}
                >
                  <Bot className={cn('size-3.5 shrink-0', active ? 'text-[#e2590b]' : 'text-text-tertiary')} />
                  <span className="truncate">{terminalTabLabel(tabState.tabs, tab)}</span>
                </button>
                <button
                  type="button"
                  aria-label={`Close ${terminalTabLabel(tabState.tabs, tab)}`}
                  className={cn(
                    'rounded p-0.5 hover:bg-white/15',
                    active ? 'text-white/55 hover:text-white' : 'text-text-faint hover:bg-black/5 hover:text-text',
                  )}
                  onClick={() => closeTab(tab.id)}
                >
                  <X className="size-3" />
                </button>
              </div>
            );
          })}
          <NewViewMenu onOpenAgent={openAgent} />
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
        {launchError && (
          <div className="absolute inset-x-0 top-0 z-10 bg-red-950 px-3 py-2 text-xs text-red-200">
            {launchError}
          </div>
        )}
        {tabState.tabs.length === 0 ? (
          <AgentLaunchPage defaultAgent={defaultAgent} onOpenAgent={openAgent} />
        ) : (
          tabState.tabs.map((tab) => (
            <AgentTerminalInstance
              key={tab.id}
              tab={tab}
              spacePath={spacePath}
              spaceSlug={spaceSlug}
              active={tab.id === tabState.activeTabId}
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
}: {
  defaultAgent: AgentKind;
  onOpenAgent: (agent: AgentKind, mode?: AgentTerminalMode) => void;
}) {
  return (
    <div className="flex h-full flex-col items-center justify-center px-7 text-center">
      <div className="mb-4 flex size-12 items-center justify-center rounded-xl border border-[#efd4c3] bg-[#fbeadd]">
        <Bot className="size-6 text-[#e2590b]" />
      </div>
      <h2 className="text-base font-semibold text-text">Work with an agent</h2>
      <p className="mt-1.5 max-w-64 text-xs leading-relaxed text-text-tertiary">
        Work live in the Current Draft, or isolate a background Agent Change for review.
      </p>
      <Button className="mt-5 min-w-44 bg-[#e2590b] text-white hover:bg-[#c94b08]" onClick={() => onOpenAgent(defaultAgent, 'live')}>
        Start {agentDisplayName(defaultAgent)} live
      </Button>
      <Button
        variant="outline"
        className="mt-2 min-w-44"
        onClick={() => onOpenAgent(defaultAgent, 'background')}
      >
        Run in background
      </Button>
      <AgentPicker defaultAgent={defaultAgent} onOpenAgent={onOpenAgent} />
    </div>
  );
}

function AgentPicker({
  defaultAgent,
  onOpenAgent,
}: {
  defaultAgent: AgentKind;
  onOpenAgent: (agent: AgentKind, mode?: AgentTerminalMode) => void;
}) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          className="mt-2 rounded px-3 py-1.5 text-xs font-medium text-text-secondary hover:bg-bg-hover"
        >
          Choose another agent
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="center" className="w-52">
        <DropdownMenuLabel className="text-xs text-text-tertiary">Live in Current Draft</DropdownMenuLabel>
        {SUPPORTED_AGENTS.filter((agent) => agent !== defaultAgent).map((agent) => (
          <DropdownMenuItem key={`live-${agent}`} onSelect={() => onOpenAgent(agent, 'live')}>
            <Bot /> {agentDisplayName(agent)}
          </DropdownMenuItem>
        ))}
        <DropdownMenuSeparator />
        <DropdownMenuLabel className="text-xs text-text-tertiary">Run in background</DropdownMenuLabel>
        {SUPPORTED_AGENTS.filter((agent) => agent !== defaultAgent).map((agent) => (
          <DropdownMenuItem key={`background-${agent}`} onSelect={() => onOpenAgent(agent, 'background')}>
            <GitBranch /> {agentDisplayName(agent)}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function NewViewMenu({
  onOpenAgent,
}: {
  onOpenAgent: (agent: AgentKind, mode?: AgentTerminalMode) => void;
}) {
  return (
    <DropdownMenu>
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
        <DropdownMenuLabel className="text-xs text-text-tertiary">Live in Current Draft</DropdownMenuLabel>
        {SUPPORTED_AGENTS.map((agent) => (
          <DropdownMenuItem key={`live-${agent}`} onSelect={() => onOpenAgent(agent, 'live')}>
            <Bot /> {agentDisplayName(agent)}
          </DropdownMenuItem>
        ))}
        <DropdownMenuSeparator />
        <DropdownMenuLabel className="text-xs text-text-tertiary">Run in background</DropdownMenuLabel>
        {SUPPORTED_AGENTS.map((agent) => (
          <DropdownMenuItem key={`background-${agent}`} onSelect={() => onOpenAgent(agent, 'background')}>
            <GitBranch /> {agentDisplayName(agent)}
          </DropdownMenuItem>
        ))}
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
}: {
  tab: AgentTerminalTab;
  spacePath: string;
  spaceSlug: string;
  active: boolean;
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
    onData: handleData,
    onExit: (exitCode) => {
      terminalRef.current?.writeln(
        `\r\n[${agentDisplayName(tab.agent)} exited${exitCode == null ? '' : `: ${exitCode}`}]`,
      );
    },
  });

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const terminal = new Terminal({
      allowProposedApi: false,
      convertEol: true,
      cursorBlink: true,
      fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
      fontSize: 13,
      scrollback: 10_000,
      theme: {
        background: '#1d1c1a',
        foreground: '#eeeae3',
        cursor: '#e2590b',
        selectionBackground: '#5c5149',
      },
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
    terminal.writeln(`Starting ${agentDisplayName(tab.agent)} in ${cwd}…`);
    fitAddonRef.current?.fit();
    void start({ cols: terminal.cols, rows: terminal.rows });
  }, [cwd, start, tab.agent]);

  useEffect(() => {
    if (!active) return;
    const terminal = terminalRef.current;
    fitAddonRef.current?.fit();
    if (terminal) void resize(terminal.cols, terminal.rows);
    terminal?.focus();
  }, [active, resize]);

  return (
    <section
      className="absolute inset-0 min-h-0 flex-col bg-[#1d1c1a]"
      style={{ display: active ? 'flex' : 'none' }}
    >
      <div className="flex h-7 shrink-0 items-center border-b border-white/8 px-2.5 text-[10px] text-white/35">
        <span>{status}</span>
        <span className="ml-2 shrink-0 text-white/55">
          {tab.mode === 'background' ? 'Background Change' : 'Current Draft'}
        </span>
        <span className="ml-2 truncate">{cwd}</span>
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          className="ml-auto text-white/45 hover:bg-white/10 hover:text-white"
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
      {error && <div className="shrink-0 bg-red-950 px-3 py-2 text-xs text-red-200">{error}</div>}
      <div ref={containerRef} className="min-h-0 flex-1 px-2 py-1" />
    </section>
  );
}
