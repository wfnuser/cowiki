import {
  agentDisplayName,
  type AgentKind,
  type AgentTerminalMode,
} from './terminal-contract.ts';

export type AgentTerminalTab = {
  id: string;
  agent: AgentKind;
  mode: AgentTerminalMode;
  changeId?: string;
  worktreePath?: string;
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
  background?: { changeId: string; worktreePath: string },
): AgentTerminalTabsState {
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
        ...(background ?? {}),
      },
    ],
  };
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

export function terminalTabLabel(tabs: AgentTerminalTab[], tab: AgentTerminalTab): string {
  const matchingTabs = tabs.filter((candidate) => candidate.agent === tab.agent);
  const position = matchingTabs.findIndex((candidate) => candidate.id === tab.id);
  const label = agentDisplayName(tab.agent);
  const numbered = matchingTabs.length > 1 ? `${label} ${position + 1}` : label;
  return tab.mode === 'background' ? `${numbered} · Background` : numbered;
}
