// The only renderer entry point to native Agent sessions. It passes ids and
// text; directories, processes and approvals stay in the desktop backend.
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

import type { AgentKind } from './lib/agents.ts';
import {
  applyEvent,
  mergeListed,
  type AgentEvent,
  type AgentMode,
  type AgentSnapshot,
  type LoginEvent,
  type NativeReadiness,
  type SessionMap,
} from './lib/agent-session.ts';

export type AgentTransport = {
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
  listen<T>(event: string, handler: (payload: T) => void): Promise<() => void>;
};

export const tauriTransport: AgentTransport = {
  invoke: (command, args) => invoke(command, args),
  listen: async (event, handler) => listen(event, ({ payload }) => handler(payload as never)),
};

export type CreateSessionRequest = {
  spaceSlug: string;
  mode: AgentMode;
  changeId: string | null;
  agent: AgentKind;
  title?: string;
};

export type SessionSubscription = {
  dispose(): void;
  refresh(): Promise<void>;
};

export function createAgentApi(transport: AgentTransport = tauriTransport) {
  const call = <T>(command: string, args?: Record<string, unknown>) => transport.invoke<T>(command, args);
  return {
    probe: (agent: AgentKind, force = false) => call<NativeReadiness>('agent_probe_native', { agent, force }),
    loginStart: (agent: AgentKind) => call<{ agent: AgentKind; url: string | null; alreadyOpen: boolean }>('agent_login_start', { agent }),
    loginCancel: (agent: AgentKind) => call<void>('agent_login_cancel', { agent }),
    onLogin: (handler: (event: LoginEvent) => void) => transport.listen<LoginEvent>('agent:login', handler),
    list: (spaceSlug: string) => call<AgentSnapshot[]>('agent_list_sessions', { spaceSlug }),
    create: (request: CreateSessionRequest) => call<AgentSnapshot>('agent_create_session', {
      request: { ...request, changeId: request.changeId ?? null, title: request.title ?? null },
    }),
    snapshot: (sessionId: string) => call<AgentSnapshot>('agent_snapshot', { sessionId }),
    send: (sessionId: string, text: string) => call<AgentSnapshot>('agent_send', { sessionId, text }),
    stop: (sessionId: string) => call<AgentSnapshot>('agent_stop', { sessionId }),
    resume: (sessionId: string) => call<AgentSnapshot>('agent_resume', { sessionId }),
    answer: (sessionId: string, approvalId: string, decision: 'allow' | 'deny') =>
      call<AgentSnapshot>('agent_answer_approval', { sessionId, approvalId, decision }),
    remove: (sessionId: string) => call<void>('agent_remove_session', { sessionId }),

    /**
     * Follows the sessions of one Space. The listener is registered before the
     * list is read so nothing between the two is lost; snapshots and events
     * converge by sequence number. Subscribing never sends anything, and
     * disposing only stops listening (sessions keep running).
     */
    subscribe(spaceSlug: string, onChange: (sessions: SessionMap) => void, onError?: (error: unknown) => void): SessionSubscription {
      let sessions: SessionMap = new Map();
      let disposed = false;
      const update = (next: SessionMap) => {
        if (disposed || next === sessions) return;
        sessions = next;
        onChange(sessions);
      };
      const unlisten = transport.listen<AgentEvent>('agent:event', (event) => {
        update(applyEvent(sessions, spaceSlug, event));
      });
      const refresh = async () => {
        await unlisten;
        try {
          const listed = await call<AgentSnapshot[]>('agent_list_sessions', { spaceSlug });
          if (disposed) return;
          const merged = mergeListed(sessions, spaceSlug, listed);
          // Report the first (possibly empty) list even if nothing changed.
          if (merged === sessions) onChange(sessions);
          else update(merged);
        } catch (error) {
          if (!disposed) onError?.(error);
        }
      };
      void refresh();
      return {
        dispose() {
          disposed = true;
          void unlisten.then((stop) => stop()).catch(() => undefined);
        },
        refresh,
      };
    },
  };
}

export type AgentApi = ReturnType<typeof createAgentApi>;

export const agentApi = createAgentApi();
