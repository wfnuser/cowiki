import { useState, useRef, useCallback, useEffect } from 'react';
import { compileAsync } from '../api';
import { authHeaders } from '../auth';

// ── Types ───────────────────────────────────────────────────────

export interface AgentEvent {
  type: 'TaskStarted' | 'ToolStart' | 'ToolEnd' | 'TaskCompleted' | 'AgentStopped' | 'connected';
  agent: string;
  task_id: string;
  tool?: string;
  input?: unknown;
  success?: boolean;
  summary?: string;
  rounds?: number;
  reason?: string;
  message?: string;
  workspace?: string;
}

export type StreamStatus = 'idle' | 'streaming' | 'done' | 'error';

const AUTO_CLOSE_MS = 3000;
const MAX_RECONNECT_ATTEMPTS = 3;

/**
 * Parse SSE text frames from a ReadableStream.
 * Splits on double-newline to extract "data: <json>" frames.
 */
async function* parseSSEStream(
  reader: ReadableStreamDefaultReader<Uint8Array>,
  signal: AbortSignal,
): AsyncGenerator<AgentEvent> {
  const decoder = new TextDecoder();
  let buffer = '';

  while (true) {
    if (signal.aborted) break;
    const { done, value } = await reader.read();
    if (done) break;

    buffer += decoder.decode(value, { stream: true });
    const lines = buffer.split('\n');
    // Keep the last partial line in buffer
    buffer = lines.pop() || '';

    for (const line of lines) {
      const trimmed = line.trim();
      if (trimmed.startsWith('data: ')) {
        const json = trimmed.slice(6);
        try {
          const event: AgentEvent = JSON.parse(json);
          yield event;
        } catch {
          // skip malformed events
        }
      }
    }
  }
}

// ── Hook ────────────────────────────────────────────────────────

export function useCompileStream(wsSlug: string) {
  const [events, setEvents] = useState<AgentEvent[]>([]);
  const [status, setStatus] = useState<StreamStatus>('idle');
  const [error, setError] = useState<string | null>(null);
  const [isPinned, setPinned] = useState(false);
  const [isOpen, setOpen] = useState(false);
  const [countdown, setCountdown] = useState(0);
  const [isReconnecting, setReconnecting] = useState(false);

  const abortRef = useRef<AbortController | null>(null);
  const reconnectCount = useRef(0);
  const autoCloseTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const countdownInterval = useRef<ReturnType<typeof setInterval> | null>(null);
  const pinnedRef = useRef(false);

  // Keep pinnedRef in sync
  useEffect(() => {
    pinnedRef.current = isPinned;
  }, [isPinned]);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      abortRef.current?.abort();
      if (autoCloseTimer.current) clearTimeout(autoCloseTimer.current);
      if (countdownInterval.current) clearInterval(countdownInterval.current);
    };
  }, []);

  const clearTimers = useCallback(() => {
    if (autoCloseTimer.current) {
      clearTimeout(autoCloseTimer.current);
      autoCloseTimer.current = null;
    }
    if (countdownInterval.current) {
      clearInterval(countdownInterval.current);
      countdownInterval.current = null;
    }
    setCountdown(0);
  }, []);

  const closeStream = useCallback(() => {
    abortRef.current?.abort();
    abortRef.current = null;
    reconnectCount.current = 0;
    setReconnecting(false);
  }, []);

  const scheduleAutoClose = useCallback(() => {
    clearTimers();

    const totalMs = AUTO_CLOSE_MS;
    const tickMs = 200;
    const totalTicks = Math.ceil(totalMs / tickMs);
    let tick = totalTicks;

    setCountdown(Math.ceil((tick * tickMs) / 1000));

    countdownInterval.current = setInterval(() => {
      tick--;
      if (tick <= 0) {
        clearTimers();
        if (!pinnedRef.current) {
          setOpen(false);
        }
      } else {
        setCountdown(Math.ceil((tick * tickMs) / 1000));
      }
    }, tickMs);
  }, [clearTimers]);

  const connectSSE = useCallback(() => {
    closeStream();

    const base = import.meta.env.VITE_API_BASE || '';
    const url = `${base}/api/agents/${wsSlug}/events`;
    const abort = new AbortController();
    abortRef.current = abort;

    // Use fetch + ReadableStream instead of EventSource so we can send
    // the Authorization header (EventSource doesn't support custom headers).
    fetch(url, {
      headers: { ...authHeaders(), Accept: 'text/event-stream' },
      signal: abort.signal,
    })
      .then(async (response) => {
        if (!response.ok) {
          throw new Error(`SSE connection failed: ${response.status}`);
        }
        if (!response.body) {
          throw new Error('SSE: no response body');
        }

        setReconnecting(false);
        reconnectCount.current = 0;

        const reader = response.body.getReader();
        for await (const event of parseSSEStream(reader, abort.signal)) {
          setEvents((prev) => [...prev, event]);
          setReconnecting(false);

          switch (event.type) {
            case 'TaskStarted':
              setStatus('streaming');
              break;
            case 'TaskCompleted':
              setStatus('done');
              scheduleAutoClose();
              break;
            case 'AgentStopped':
              if (event.reason && event.reason !== 'finished') {
                setStatus('error');
                setError(event.reason);
              }
              break;
          }
        }
      })
      .catch((err: Error) => {
        if (err.name === 'AbortError') return; // intentional close
        if (reconnectCount.current < MAX_RECONNECT_ATTEMPTS) {
          reconnectCount.current++;
          setReconnecting(true);
          const delay = Math.min(1000 * Math.pow(2, reconnectCount.current - 1), 4000);
          setTimeout(() => {
            if (abortRef.current === null) {
              // Another connection was started — don't double-connect
              return;
            }
            connectSSE();
          }, delay);
        } else {
          setReconnecting(false);
          setStatus('error');
          setError('Connection lost');
        }
      });
  }, [wsSlug, closeStream, scheduleAutoClose]);

  const startCompile = useCallback(async (branch: string) => {
    setEvents([]);
    setStatus('streaming');
    setError(null);
    setOpen(true);
    pinnedRef.current = false;
    setPinned(false);
    clearTimers();

    // Connect SSE first so we don't miss events
    connectSSE();

    // Fire-and-forget POST — SSE carries the real result
    try {
      await compileAsync(branch, wsSlug);
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : 'Unknown error';
      setError(`Failed to start compile: ${msg}`);
      // If SSE never connected, mark as error
      setStatus((prev) => (prev === 'streaming' ? 'error' : prev));
    }
  }, [wsSlug, connectSSE, clearTimers]);

  const closeDrawer = useCallback(() => {
    clearTimers();
    setOpen(false);
  }, [clearTimers]);

  const openDrawer = useCallback(() => {
    setOpen(true);
  }, []);

  const togglePin = useCallback(() => {
    setPinned((p) => {
      const next = !p;
      if (next) {
        clearTimers();
      }
      return next;
    });
  }, [clearTimers]);

  return {
    events,
    isCompiling: status === 'streaming',
    status,
    error,
    startCompile,
    isOpen,
    openDrawer,
    closeDrawer,
    isPinned,
    setPinned,
    togglePin,
    countdown,
    isReconnecting,
  };
}
