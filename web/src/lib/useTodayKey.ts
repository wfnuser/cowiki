import { useEffect, useState } from 'react';
import { dateKey, msUntilNextDay } from '@/lib/journal';

const DEFAULT_NOW = () => new Date();
const MAX_TIMEOUT_MS = 2_147_483_647;

/**
 * Local `YYYY-MM-DD` for today that rolls over automatically at local midnight
 * and whenever the window wakes from sleep or regains focus.
 */
export function useTodayKey(now: () => Date = DEFAULT_NOW): string {
  const [todayKey, setTodayKey] = useState(() => dateKey(now()));

  useEffect(() => {
    let timer: number | null = null;
    const sync = () => {
      const current = now();
      const nextKey = dateKey(current);
      setTodayKey((prev) => (prev === nextKey ? prev : nextKey));
      if (timer !== null) window.clearTimeout(timer);
      // 1s cushion ensures a slightly early timer callback still lands on the new day.
      const delay = Math.min(msUntilNextDay(current) + 1000, MAX_TIMEOUT_MS);
      timer = window.setTimeout(sync, delay);
    };
    sync();
    window.addEventListener('focus', sync);
    document.addEventListener('visibilitychange', sync);
    return () => {
      if (timer !== null) window.clearTimeout(timer);
      window.removeEventListener('focus', sync);
      document.removeEventListener('visibilitychange', sync);
    };
  }, [now]);

  return todayKey;
}
