/** Light, dark, or whatever the system uses. Stored per device. */
export type ThemePreference = 'system' | 'light' | 'dark';

export const THEME_STORAGE_KEY = 'cowiki.theme';
export const THEME_CHOICES: ReadonlyArray<[ThemePreference, string]> = [
  ['system', 'System'],
  ['light', 'Light'],
  ['dark', 'Dark'],
];

type Storage = Pick<globalThis.Storage, 'getItem' | 'setItem'>;

export function loadTheme(storage: Storage | undefined = globalThis.localStorage): ThemePreference {
  try {
    const value = storage?.getItem(THEME_STORAGE_KEY);
    return value === 'light' || value === 'dark' ? value : 'system';
  } catch {
    return 'system';
  }
}

export function saveTheme(preference: ThemePreference, storage: Storage | undefined = globalThis.localStorage): void {
  try {
    storage?.setItem(THEME_STORAGE_KEY, preference);
  } catch {
    // Private windows can refuse storage; the choice still applies now.
  }
}

export function resolvedTheme(preference: ThemePreference, systemDark: boolean): 'light' | 'dark' {
  return preference === 'system' ? (systemDark ? 'dark' : 'light') : preference;
}

const query = () => globalThis.matchMedia?.('(prefers-color-scheme: dark)');

/** Puts `.dark` on <html> (the design tokens switch on it) and keeps
 * following the system while the preference is "system". */
export function applyTheme(preference: ThemePreference): () => void {
  const root = globalThis.document?.documentElement;
  if (!root) return () => {};
  const media = query();
  const update = () => {
    const theme = resolvedTheme(preference, Boolean(media?.matches));
    root.classList.toggle('dark', theme === 'dark');
    root.dataset.theme = theme;
    root.style.colorScheme = theme;
    globalThis.dispatchEvent?.(new CustomEvent('cowiki:theme', { detail: theme }));
  };
  update();
  if (preference !== 'system' || !media) return () => {};
  media.addEventListener('change', update);
  return () => media.removeEventListener('change', update);
}

/** The theme in effect right now. */
export function currentTheme(): 'light' | 'dark' {
  return globalThis.document?.documentElement.classList.contains('dark') ? 'dark' : 'light';
}

let stopFollowing: () => void = () => {};

/** Applies the stored preference (called once before the first render). */
export function startTheme(): ThemePreference {
  const preference = loadTheme();
  stopFollowing();
  stopFollowing = applyTheme(preference);
  return preference;
}

/** Saves and applies a new preference. */
export function setThemePreference(preference: ThemePreference): void {
  saveTheme(preference);
  stopFollowing();
  stopFollowing = applyTheme(preference);
}
