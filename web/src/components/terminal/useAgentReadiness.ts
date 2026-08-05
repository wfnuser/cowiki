import { useCallback, useRef, useState } from 'react';

import { probeAgent } from '@/local-api';

import type { AgentKind, AgentReadiness } from './terminal-contract';

export type AgentProbeState = {
  phase: 'idle' | 'checking' | 'settled';
  readiness?: AgentReadiness;
};

const idleState: AgentProbeState = { phase: 'idle' };

export function useAgentReadiness() {
  const [states, setStates] = useState<Partial<Record<AgentKind, AgentProbeState>>>({});
  const statesRef = useRef(states);
  const inflightRef = useRef(new Map<AgentKind, Promise<AgentReadiness>>());

  const updateState = useCallback((agent: AgentKind, state: AgentProbeState) => {
    statesRef.current = { ...statesRef.current, [agent]: state };
    setStates(statesRef.current);
  }, []);

  const check = useCallback((agent: AgentKind, force = false): Promise<AgentReadiness> => {
    const current = statesRef.current[agent];
    if (!force && current?.phase === 'settled' && current.readiness) {
      return Promise.resolve(current.readiness);
    }
    const inflight = inflightRef.current.get(agent);
    if (inflight) return inflight;

    updateState(agent, { phase: 'checking', readiness: current?.readiness });
    const request = probeAgent(agent)
      .catch((cause): AgentReadiness => ({
        agent,
        status: 'broken',
        message: 'CoWiki could not check this CLI',
        detail: cause instanceof Error ? cause.message : String(cause),
      }))
      .then((readiness) => {
        updateState(agent, { phase: 'settled', readiness });
        return readiness;
      })
      .finally(() => {
        if (inflightRef.current.get(agent) === request) inflightRef.current.delete(agent);
      });
    inflightRef.current.set(agent, request);
    return request;
  }, [updateState]);

  const stateFor = useCallback(
    (agent: AgentKind): AgentProbeState => states[agent] ?? idleState,
    [states],
  );

  return { check, stateFor, states };
}
