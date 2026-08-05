import {
  agentDisplayName,
  type AgentKind,
  type AgentTerminalIntent,
  type AgentTerminalMode,
} from './terminal-contract.ts';

export type AgentTerminalTab = {
  id: string;
  agent: AgentKind;
  mode: AgentTerminalMode;
  intent?: AgentTerminalIntent;
  changeId?: string;
  worktreePath?: string;
  initialTask?: string;
};

export type AgentTerminalTabsState = {
  activeTabId: string | null;
  tabs: AgentTerminalTab[];
};

export function addAgentTab(
  state: AgentTerminalTabsState,
  agent: AgentKind,
  id: string,
  mode: AgentTerminalMode = 'live',
  background?: { changeId: string; worktreePath: string; initialTask?: string },
  intent: AgentTerminalIntent = 'run',
): AgentTerminalTabsState {
  if (intent === 'login' && (agent !== 'codex' || mode !== 'live')) {
    throw new Error('Only Codex can open a live Agent login tab');
  }
  if (mode === 'background' && !background) {
    throw new Error('background Agent tabs require a managed Change');
  }
  return {
    activeTabId: id,
    tabs: [
      ...state.tabs,
      {
        id,
        agent,
        mode,
        ...(intent === 'run' ? {} : { intent }),
        ...(background ?? {}),
      },
    ],
  };
}

export function addOrFocusCodexLoginTab(
  state: AgentTerminalTabsState,
  id: string,
): AgentTerminalTabsState {
  const existing = state.tabs.find((tab) => tab.agent === 'codex' && tab.intent === 'login');
  if (existing) return { ...state, activeTabId: existing.id };
  return addAgentTab(state, 'codex', id, 'live', undefined, 'login');
}

export function closeAgentTab(
  state: AgentTerminalTabsState,
  tabId: string,
): AgentTerminalTabsState {
  const closedIndex = state.tabs.findIndex((tab) => tab.id === tabId);
  if (closedIndex < 0) return state;

  const tabs = state.tabs.filter((tab) => tab.id !== tabId);
  if (state.activeTabId !== tabId) return { ...state, tabs };

  return {
    tabs,
    activeTabId: tabs[closedIndex]?.id ?? tabs[closedIndex - 1]?.id ?? null,
  };
}

export function openOrActivateAgentChangeTab(
  state: AgentTerminalTabsState,
  agent: AgentKind,
  id: string,
  change: { changeId: string; worktreePath: string; initialTask?: string },
): AgentTerminalTabsState {
  const existing = state.tabs.find((tab) => tab.changeId === change.changeId);
  if (existing) return { ...state, activeTabId: existing.id };
  return addAgentTab(state, agent, id, 'background', change);
}

export function terminalTabLabel(tabs: AgentTerminalTab[], tab: AgentTerminalTab): string {
  const matchingTabs = tabs.filter((candidate) => candidate.agent === tab.agent);
  const position = matchingTabs.findIndex((candidate) => candidate.id === tab.id);
  const label = agentDisplayName(tab.agent);
  const numbered = matchingTabs.length > 1 ? `${label} ${position + 1}` : label;
  if (tab.intent === 'login') return `${numbered} · Sign in`;
  return tab.mode === 'background' ? `${numbered} · Background` : numbered;
}
