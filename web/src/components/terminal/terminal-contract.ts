import { agentDefinition, type AgentKind } from '../../lib/agents.ts';

export type { AgentKind } from '../../lib/agents.ts';

export type TerminalSize = {
  cols: number;
  rows: number;
};

export type TerminalDataEvent = {
  sessionId: string;
  data: string;
};

export type TerminalExitEvent = {
  sessionId: string;
  exitCode: number | null;
};

export function agentInitialCommand(agent: AgentKind): string {
  return agentDefinition(agent).command;
}

export function agentDisplayName(agent: AgentKind): string {
  return agentDefinition(agent).displayName;
}

export function normalizeTerminalSize(cols: number, rows: number): TerminalSize {
  return {
    cols: clampInteger(cols, 80, 20, 500),
    rows: clampInteger(rows, 24, 5, 200),
  };
}

export function belongsToTerminalSession(
  event: { sessionId?: unknown } | null | undefined,
  sessionId: string,
): boolean {
  return event?.sessionId === sessionId;
}

function clampInteger(value: number, fallback: number, min: number, max: number): number {
  if (!Number.isFinite(value)) return fallback;
  return Math.max(min, Math.min(max, Math.floor(value)));
}
