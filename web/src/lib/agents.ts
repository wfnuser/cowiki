export type AgentKind = 'codex' | 'claude' | 'grok' | 'antigravity' | 'opencode' | 'hermes';

type AgentDefinition = {
  displayName: string;
  providerName: string;
  command: string;
};

export const SUPPORTED_AGENTS: readonly AgentKind[] = [
  'codex',
  'claude',
  'antigravity',
  'grok',
  'opencode',
  'hermes',
];

const AGENT_DEFINITIONS: Record<AgentKind, AgentDefinition> = {
  codex: { displayName: 'Codex', providerName: 'OpenAI', command: 'codex' },
  claude: { displayName: 'Claude Code', providerName: 'Anthropic', command: 'claude' },
  antigravity: { displayName: 'Antigravity CLI', providerName: 'Google', command: 'agy' },
  grok: { displayName: 'Grok', providerName: 'xAI', command: 'grok' },
  opencode: { displayName: 'OpenCode', providerName: 'Open source', command: 'opencode' },
  hermes: { displayName: 'Hermes Agent', providerName: 'Nous Research', command: 'hermes' },
};

export function isAgentKind(value: unknown): value is AgentKind {
  return typeof value === 'string' && SUPPORTED_AGENTS.some((agent) => agent === value);
}

export function agentDefinition(agent: AgentKind): AgentDefinition {
  return AGENT_DEFINITIONS[agent];
}
