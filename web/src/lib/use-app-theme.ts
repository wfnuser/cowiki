import { useEffect, useState } from 'react';

import { currentTheme } from './theme.ts';

/** The theme in effect, updated when Settings or the system changes it. */
export function useAppTheme(): 'light' | 'dark' {
  const [theme, setTheme] = useState(currentTheme);
  useEffect(() => {
    const changed = (event: Event) => setTheme((event as CustomEvent<'light' | 'dark'>).detail);
    window.addEventListener('cowiki:theme', changed);
    return () => window.removeEventListener('cowiki:theme', changed);
  }, []);
  return theme;
}
