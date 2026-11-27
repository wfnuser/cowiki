import { createContext, useContext, type ReactNode } from 'react';
import { createPortal } from 'react-dom';

/**
 * Places in the top bar a view may fill while it is on screen: `start` after
 * the Back and Forward arrows, `end` before the app's own actions. A study
 * session puts its deck, count and tools there instead of a row of its own.
 */
export const TopBarSlots = createContext<{ start: HTMLElement | null; end: HTMLElement | null }>({ start: null, end: null });

export function TopBarStart({ children }: { children: ReactNode }) {
  const { start } = useContext(TopBarSlots);
  return start ? createPortal(children, start) : null;
}

export function TopBarEnd({ children }: { children: ReactNode }) {
  const { end } = useContext(TopBarSlots);
  return end ? createPortal(children, end) : null;
}
