import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { agentApi } from '@/agent-api';
import type { AgentKind } from '@/lib/agents';
import {
  orderedSessions,
  type NativeReadiness,
  type SessionMap,
} from '@/lib/agent-session';

/** The current Space's sessions. It only observes: closing the panel or
 * switching Spaces never stops a session, and nothing is sent on mount. */
export function useAgentSessions(spaceSlug: string) {
  const [state, setState] = useState<{ space: string; sessions: SessionMap; ready: boolean; error: string | null }>({
    space: spaceSlug,
    sessions: new Map(),
    ready: false,
    error: null,
  });

  useEffect(() => {
    const subscription = agentApi.subscribe(
      spaceSlug,
      (sessions) => setState({ space: spaceSlug, sessions, ready: true, error: null }),
      (error) => setState((current) => ({
        ...current,
        ready: true,
        error: error instanceof Error ? error.message : String(error),
      })),
    );
    return () => subscription.dispose();
  }, [spaceSlug]);

  const [removed, setRemoved] = useState<ReadonlySet<string>>(new Set());
  // A late update from the previous Space is never shown in this one.
  const sessions = useMemo(() => {
    const visible = state.space === spaceSlug ? state.sessions : new Map() as SessionMap;
    if (removed.size === 0) return visible;
    return new Map([...visible].filter(([id]) => !removed.has(id))) as SessionMap;
  }, [removed, spaceSlug, state.sessions, state.space]);
  const ordered = useMemo(() => orderedSessions(sessions), [sessions]);
  const forget = useCallback((sessionId: string) => {
    setRemoved((current) => new Set([...current, sessionId]));
  }, []);
  return {
    sessions,
    ordered,
    forget,
    ready: state.space === spaceSlug && state.ready,
    error: state.space === spaceSlug ? state.error : null,
  };
}

export type ProbeState = {
  phase: 'idle' | 'checking' | 'settled';
  readiness?: NativeReadiness;
  signingIn?: boolean;
  loginMessage?: string | null;
};

export function useNativeReadiness() {
  const [states, setStates] = useState<Partial<Record<AgentKind, ProbeState>>>({});
  const statesRef = useRef(states);
  const inflight = useRef(new Map<AgentKind, Promise<NativeReadiness>>());

  const patch = useCallback((agent: AgentKind, next: Partial<ProbeState>) => {
    statesRef.current = {
      ...statesRef.current,
      [agent]: { phase: 'idle', ...statesRef.current[agent], ...next },
    };
    setStates(statesRef.current);
  }, []);

  const check = useCallback((agent: AgentKind, force = false): Promise<NativeReadiness> => {
    const current = statesRef.current[agent];
    if (!force && current?.phase === 'settled' && current.readiness) return Promise.resolve(current.readiness);
    const running = inflight.current.get(agent);
    if (running) return running;
    patch(agent, { phase: 'checking' });
    const request = agentApi
      .probe(agent, force)
      .catch((cause): NativeReadiness => ({
        agent,
        status: 'broken',
        protocol: null,
        version: null,
        account: null,
        message: 'CoWiki could not check this Agent',
        detail: cause instanceof Error ? cause.message : String(cause),
        canSignIn: false,
      }))
      .then((readiness) => {
        patch(agent, { phase: 'settled', readiness });
        return readiness;
      })
      .finally(() => {
        if (inflight.current.get(agent) === request) inflight.current.delete(agent);
      });
    inflight.current.set(agent, request);
    return request;
  }, [patch]);

  const signIn = useCallback(async (agent: AgentKind) => {
    patch(agent, { signingIn: true, loginMessage: null });
    try {
      await agentApi.loginStart(agent);
    } catch (cause) {
      patch(agent, { signingIn: false, loginMessage: cause instanceof Error ? cause.message : String(cause) });
    }
  }, [patch]);

  const cancelSignIn = useCallback(async (agent: AgentKind) => {
    await agentApi.loginCancel(agent).catch(() => undefined);
    patch(agent, { signingIn: false });
  }, [patch]);

  useEffect(() => {
    let stop: (() => void) | undefined;
    let disposed = false;
    void agentApi
      .onLogin((event) => {
        patch(event.agent, {
          signingIn: false,
          loginMessage: event.status === 'completed' ? null : event.message ?? (event.status === 'cancelled' ? 'Sign-in was cancelled.' : 'Sign-in did not finish.'),
        });
        void check(event.agent, true);
      })
      .then((unlisten) => {
        if (disposed) unlisten();
        else stop = unlisten;
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
      stop?.();
    };
  }, [check, patch]);

  const stateFor = useCallback((agent: AgentKind): ProbeState => states[agent] ?? { phase: 'idle' }, [states]);
  return { check, signIn, cancelSignIn, stateFor };
}
