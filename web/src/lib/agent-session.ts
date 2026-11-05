// The native Agent session contract (mirrors web/src-tauri/src/agent_runtime/
// contract.rs). Pure functions only: no Tauri calls, so it runs under node.
import type { AgentKind } from './agents.ts';

export type AgentStatus =
  | 'starting'
  | 'idle'
  | 'running'
  | 'waiting'
  | 'stopping'
  | 'stopped'
  | 'failed'
  | 'interrupted';
export type AgentMode = 'live' | 'background';
export type ToolStatus = 'running' | 'completed' | 'failed' | 'declined' | 'stopped';
export type ToolKind = 'command' | 'file' | 'search' | 'mcp' | 'think' | 'other';

export type AgentMessageItem = {
  type: 'message';
  id: string;
  role: 'user' | 'assistant';
  text: string;
  turnId: string | null;
};
export type AgentToolItem = {
  type: 'tool';
  id: string;
  turnId: string | null;
  name: string;
  toolKind: ToolKind;
  status: ToolStatus;
  output: string;
  truncated: boolean;
};
export type AgentItem = AgentMessageItem | AgentToolItem;

export type AgentApproval = {
  id: string;
  turnId: string | null;
  kind: 'command' | 'file' | 'tool';
  title: string;
  description: string;
  command: string | null;
  cwd: string | null;
  paths: string[];
};

export type AgentSnapshot = {
  sessionId: string;
  provider: AgentKind;
  title: string;
  spaceSlug: string;
  mode: AgentMode;
  changeId: string | null;
  threadId: string | null;
  model: string | null;
  seq: number;
  status: AgentStatus;
  items: AgentItem[];
  approvals: AgentApproval[];
  error: string | null;
  notice: string | null;
  closed: boolean;
  createdAt: number;
  updatedAt: number;
};

export type AgentEvent = { sessionId: string; seq: number; snapshot: AgentSnapshot };

export type NativeStatus =
  | 'available'
  | 'signedOut'
  | 'notInstalled'
  | 'incompatible'
  | 'broken'
  | 'unsupported';

export type NativeReadiness = {
  agent: AgentKind;
  status: NativeStatus;
  protocol: string | null;
  version: string | null;
  account: string | null;
  message: string | null;
  detail: string | null;
  canSignIn: boolean;
};

export type LoginEvent = { agent: AgentKind; status: 'completed' | 'failed' | 'cancelled'; message: string | null };

const BUSY: ReadonlySet<AgentStatus> = new Set(['starting', 'running', 'waiting', 'stopping']);

export function isBusy(status: AgentStatus): boolean {
  return BUSY.has(status);
}

/** A valid event for this session with a newer sequence wins; anything else
 * (another session, a duplicate, an older or malformed snapshot) is ignored. */
export function acceptSnapshot(current: AgentSnapshot | null, event: AgentEvent): AgentSnapshot | null {
  const { snapshot } = event;
  if (!snapshot || snapshot.sessionId !== event.sessionId || snapshot.seq !== event.seq) return current;
  if (current && current.sessionId !== event.sessionId) return current;
  if (current && snapshot.seq <= current.seq) return current;
  return snapshot;
}

export type SessionMap = ReadonlyMap<string, AgentSnapshot>;

/** Applies an event to the sessions of one Space; other Spaces never enter. */
export function applyEvent(sessions: SessionMap, spaceSlug: string, event: AgentEvent): SessionMap {
  if (!event?.snapshot || event.snapshot.spaceSlug !== spaceSlug) return sessions;
  const current = sessions.get(event.sessionId) ?? null;
  const next = acceptSnapshot(current, event);
  if (next === current || next === null) return sessions;
  const copy = new Map(sessions);
  copy.set(event.sessionId, next);
  return copy;
}

/** Merges a listed snapshot (from `agent_list_sessions`) without going back
 * in time if a newer event already arrived. */
export function mergeListed(sessions: SessionMap, spaceSlug: string, listed: AgentSnapshot[]): SessionMap {
  let result = sessions;
  for (const snapshot of listed) {
    result = applyEvent(result, spaceSlug, { sessionId: snapshot.sessionId, seq: snapshot.seq, snapshot });
  }
  return result;
}

export function orderedSessions(sessions: SessionMap): AgentSnapshot[] {
  return [...sessions.values()].sort((left, right) => right.updatedAt - left.updatedAt);
}

export function statusLabel(snapshot: Pick<AgentSnapshot, 'status' | 'closed'>): string {
  if (snapshot.closed) return 'Closed';
  switch (snapshot.status) {
    case 'starting': return 'Starting';
    case 'idle': return 'Ready';
    case 'running': return 'Working';
    case 'waiting': return 'Needs approval';
    case 'stopping': return 'Stopping';
    case 'stopped': return 'Stopped';
    case 'failed': return 'Failed';
    case 'interrupted': return 'Interrupted';
  }
}

export function canSend(snapshot: AgentSnapshot): boolean {
  return !snapshot.closed && !isBusy(snapshot.status);
}

export function canResume(snapshot: AgentSnapshot): boolean {
  return !snapshot.closed && snapshot.status === 'interrupted' && Boolean(snapshot.threadId);
}

/** Tool steps that belong together are shown under one disclosure; the last
 * running step stays visible. */
export function summarizeTool(item: AgentToolItem): string {
  switch (item.status) {
    case 'running': return 'Running';
    case 'completed': return 'Done';
    case 'failed': return 'Failed';
    case 'declined': return 'Declined';
    case 'stopped': return 'Stopped';
  }
}
