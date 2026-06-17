import { useState, useRef, useCallback, useEffect } from 'react';
import { compileAsync } from '../api';

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
}

export type StreamStatus = 'idle' | 'streaming' | 'done' | 'error';

const AUTO_CLOSE_MS = 3000;
const MAX_RECONNECT_ATTEMPTS = 3;

// ── Hook ────────────────────────────────────────────────────────

export function useCompileStream(wsSlug: string) {
  const [events, setEvents] = useState<AgentEvent[]>([]);
  const [status, setStatus] = useState<StreamStatus>('idle');
  const [error, setError] = useState<string | null>(null);
  const [isPinned, setPinned] = useState(false);
  const [isOpen, setOpen] = useState(false);
  const [countdown, setCountdown] = useState(0);
  const [isReconnecting, setReconnecting] = useState(false);
  const [mode, setMode] = useState<'function' | 'stream' | null>(null);

  const eventSourceRef = useRef<EventSource | null>(null);
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
      eventSourceRef.current?.close();
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

  const closeEventSource = useCallback(() => {
    eventSourceRef.current?.close();
    eventSourceRef.current = null;
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
    closeEventSource();

    const base = import.meta.env.VITE_API_BASE || '';
    const url = `${base}/api/agents/${wsSlug}/events`;

    const es = new EventSource(url);
    eventSourceRef.current = es;

    es.onmessage = (e) => {
      try {
        const event: AgentEvent = JSON.parse(e.data);
        setEvents((prev) => [...prev, event]);
        setReconnecting(false);

        // Parse mode from connected event
        if (event.type === 'connected' && (event as Record<string, unknown>).mode) {
          const m = (event as Record<string, unknown>).mode as string;
          if (m === 'function' || m === 'stream') setMode(m);
        }

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
            // Don't auto-close on error — keep drawer open
            break;
        }
      } catch {
        // ignore malformed events
      }
    };

    es.onerror = () => {
      es.close();
      if (reconnectCount.current < MAX_RECONNECT_ATTEMPTS) {
        reconnectCount.current++;
        setReconnecting(true);
        const delay = Math.min(1000 * Math.pow(2, reconnectCount.current - 1), 4000);
        setTimeout(() => {
          if (eventSourceRef.current === null) {
            connectSSE();
          }
        }, delay);
      } else {
        setReconnecting(false);
        setStatus('error');
        setError('Connection lost');
      }
    };
  }, [wsSlug, closeEventSource, scheduleAutoClose]);

  const startCompile = useCallback(async (branch: string) => {
    setEvents([]);
    setStatus('streaming');
    setError(null);
    setOpen(true);
    pinnedRef.current = false;
    setPinned(false);
    clearTimers();

    // Connect SSE first so we don't miss events
    setMode(null); // Reset mode — will be set by connected event
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
    mode,
  };
}
