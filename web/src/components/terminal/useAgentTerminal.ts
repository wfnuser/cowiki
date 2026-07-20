import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { useCallback, useEffect, useRef, useState } from 'react';

import {
  agentInitialCommand,
  belongsToTerminalSession,
  normalizeTerminalSize,
  type AgentKind,
  type AgentTerminalMode,
  type TerminalDataEvent,
  type TerminalExitEvent,
  type TerminalSize,
} from './terminal-contract';

type TerminalStatus = 'idle' | 'starting' | 'running' | 'exited' | 'error';

type UseAgentTerminalOptions = {
  cwd: string;
  agent: AgentKind;
  mode: AgentTerminalMode;
  spaceSlug: string;
  changeId?: string;
  initialTask?: string;
  onData: (data: string) => void;
  onExit?: (exitCode: number | null) => void;
};

export function useAgentTerminal({
  agent,
  changeId,
  cwd,
  mode,
  initialTask,
  onData,
  onExit,
  spaceSlug,
}: UseAgentTerminalOptions) {
  const sessionIdRef = useRef<string | null>(null);
  const generationRef = useRef(0);
  const listenersReadyRef = useRef<Promise<void> | null>(null);
  const onDataRef = useRef(onData);
  const onExitRef = useRef(onExit);
  const [status, setStatus] = useState<TerminalStatus>('idle');
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    onDataRef.current = onData;
    onExitRef.current = onExit;
  }, [onData, onExit]);

  const stop = useCallback(async () => {
    generationRef.current += 1;
    const sessionId = sessionIdRef.current;
    sessionIdRef.current = null;
    if (!sessionId) return;
    await invoke('terminal_kill', { sessionId }).catch(() => undefined);
  }, []);

  const start = useCallback(async (size: TerminalSize = { cols: 80, rows: 24 }) => {
    const generation = generationRef.current + 1;
    generationRef.current = generation;
    const previousSessionId = sessionIdRef.current;
    sessionIdRef.current = null;
    if (previousSessionId) {
      await invoke('terminal_kill', { sessionId: previousSessionId }).catch(() => undefined);
    }
    if (generationRef.current !== generation) return;
    setStatus('starting');
    setError(null);
    try {
      await listenersReadyRef.current;
      if (generationRef.current !== generation) return;
      const normalized = normalizeTerminalSize(size.cols, size.rows);
      // Install the opaque session id before Rust starts the PTY so even the
      // shell's first bytes and an immediate exit are routed to this tab.
      const requestedSessionId = `terminal:${crypto.randomUUID()}`;
      sessionIdRef.current = requestedSessionId;
      const created = await invoke<{ sessionId: string }>('terminal_create', {
        request: {
          sessionId: requestedSessionId,
          cwd,
          mode,
          spaceSlug,
          changeId,
          agent,
          initialCommand: agentInitialCommand(agent),
          taskPrompt: initialTask,
          ...normalized,
        },
      });
      if (generationRef.current !== generation) {
        await invoke('terminal_kill', { sessionId: created.sessionId }).catch(() => undefined);
        return;
      }
      if (created.sessionId !== requestedSessionId) {
        await invoke('terminal_kill', { sessionId: created.sessionId }).catch(() => undefined);
        throw new Error('terminal returned an unexpected session id');
      }
      // An extremely short-lived process may have emitted terminal:exit while
      // the invoke response was in flight. The listener already marked it
      // exited; never resurrect that dead session as running.
      if (sessionIdRef.current !== requestedSessionId) return;
      sessionIdRef.current = created.sessionId;
      setStatus('running');
    } catch (cause) {
      if (generationRef.current !== generation) return;
      sessionIdRef.current = null;
      setStatus('error');
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  }, [agent, changeId, cwd, initialTask, mode, spaceSlug]);

  const write = useCallback(async (data: string) => {
    const sessionId = sessionIdRef.current;
    if (!sessionId) return;
    await invoke('terminal_write', { sessionId, data });
  }, []);

  const resize = useCallback(async (cols: number, rows: number) => {
    const sessionId = sessionIdRef.current;
    if (!sessionId) return;
    await invoke('terminal_resize', {
      sessionId,
      ...normalizeTerminalSize(cols, rows),
    });
  }, []);

  useEffect(() => {
    let disposed = false;
    const unlisteners: UnlistenFn[] = [];

    const listenersReady = Promise.all([
      listen<TerminalDataEvent>('terminal:data', ({ payload }) => {
        const sessionId = sessionIdRef.current;
        if (sessionId && belongsToTerminalSession(payload, sessionId)) {
          onDataRef.current(payload.data);
        }
      }),
      listen<TerminalExitEvent>('terminal:exit', ({ payload }) => {
        const sessionId = sessionIdRef.current;
        if (!sessionId || !belongsToTerminalSession(payload, sessionId)) return;
        sessionIdRef.current = null;
        setStatus('exited');
        onExitRef.current?.(payload.exitCode);
      }),
    ]).then((listeners) => {
      if (disposed) listeners.forEach((unlisten) => unlisten());
      else unlisteners.push(...listeners);
    });
    listenersReadyRef.current = listenersReady;

    return () => {
      disposed = true;
      if (listenersReadyRef.current === listenersReady) listenersReadyRef.current = null;
      unlisteners.forEach((unlisten) => unlisten());
      void stop();
    };
  }, [stop]);

  return { error, resize, start, status, stop, write };
}
