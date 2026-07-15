import type { AgentKind } from './terminal-contract';

export type AgentTerminalTab = {
  id: string;
  agent: AgentKind;
};

export type AgentTerminalTabsState = {
  activeTabId: string | null;
  tabs: AgentTerminalTab[];
};

export function addAgentTab(
  state: AgentTerminalTabsState,
  agent: AgentKind,
  id: string,
): AgentTerminalTabsState {
  return {
    activeTabId: id,
    tabs: [...state.tabs, { id, agent }],
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
  const label = tab.agent === 'codex' ? 'Codex' : 'Claude';
  return matchingTabs.length > 1 ? `${label} ${position + 1}` : label;
}
