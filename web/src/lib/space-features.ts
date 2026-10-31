import { DEFAULT_JOURNAL_CONFIG, type JournalConfig } from './journal.ts';

/**
 * Optional modules a Space can turn on. Turning a module off only hides its
 * entry points; journal pages and task lines stay ordinary Markdown files.
 */
export type ModuleId = 'journal' | 'tasks';

export const MODULES: Array<{ id: ModuleId; label: string; description: string }> = [
  {
    id: 'journal',
    label: 'Daily journal',
    description: 'A "Today" entry that opens or creates the day\'s journal page, with date navigation.',
  },
  {
    id: 'tasks',
    label: 'Tasks',
    description: 'Collects checkbox tasks from every page into Today, Upcoming and Done lists.',
  },
];

/**
 * Space content settings. They live in the Space (`.cowiki/space.json`) so a
 * whole team shares the journal location and which modules exist.
 */
export interface SpaceFeatures {
  modules: Record<ModuleId, boolean>;
  journal: JournalConfig;
}

export const DEFAULT_SPACE_FEATURES: SpaceFeatures = {
  modules: { journal: true, tasks: true },
  journal: { ...DEFAULT_JOURNAL_CONFIG },
};

function record(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : null;
}

function cleanFolder(value: unknown, fallback: string): string {
  if (typeof value !== 'string') return fallback;
  const folder = value.replaceAll('\\', '/').trim().replace(/^\/+|\/+$/g, '');
  if (folder.split('/').some((part) => part === '..' || part.startsWith('.'))) return fallback;
  return folder;
}

function cleanFormat(value: unknown, fallback: string): string {
  if (typeof value !== 'string' || !value.trim()) return fallback;
  const format = value.trim();
  if (format.includes('..') || format.startsWith('/') || format.includes('\\')) return fallback;
  return format;
}

function cleanTemplate(value: unknown): string | null {
  if (typeof value !== 'string' || !value.trim()) return null;
  const path = value.replaceAll('\\', '/').trim().replace(/^\/+/, '');
  if (path.split('/').some((part) => part === '..')) return null;
  return /\.md$/i.test(path) ? path : `${path}.md`;
}

/** Read the `features` section of `.cowiki/space.json`, tolerating partial or old files. */
export function parseSpaceFeatures(config: unknown): SpaceFeatures {
  const root = record(config);
  const features = record(root?.features);
  const modules = record(features?.modules);
  const journal = record(features?.journal);
  return {
    modules: {
      journal: typeof modules?.journal === 'boolean' ? modules.journal : DEFAULT_SPACE_FEATURES.modules.journal,
      tasks: typeof modules?.tasks === 'boolean' ? modules.tasks : DEFAULT_SPACE_FEATURES.modules.tasks,
    },
    journal: {
      folder: cleanFolder(journal?.folder, DEFAULT_JOURNAL_CONFIG.folder),
      format: cleanFormat(journal?.format, DEFAULT_JOURNAL_CONFIG.format),
      template: cleanTemplate(journal?.template),
    },
  };
}

/** Merge features back into the full Space config without dropping other keys. */
export function withSpaceFeatures(config: unknown, features: SpaceFeatures): Record<string, unknown> {
  const root = { ...(record(config) ?? {}) };
  root.features = {
    modules: { ...features.modules },
    journal: {
      folder: features.journal.folder,
      format: features.journal.format,
      template: features.journal.template,
    },
  };
  return root;
}

/**
 * Personal usage preferences, stored on this device only. A member can hide a
 * module the Space offers without changing it for anyone else.
 */
export interface PersonalPreferences {
  openJournalOnStartup: boolean;
  hiddenModules: Record<string, ModuleId[]>;
}

export interface PreferenceStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

export const PERSONAL_PREFERENCES_KEY = 'cowiki.personal.preferences';

export const DEFAULT_PERSONAL_PREFERENCES: PersonalPreferences = {
  openJournalOnStartup: false,
  hiddenModules: {},
};

function isModuleId(value: unknown): value is ModuleId {
  return value === 'journal' || value === 'tasks';
}

export function loadPersonalPreferences(storage: PreferenceStorage): PersonalPreferences {
  try {
    const value = record(JSON.parse(storage.getItem(PERSONAL_PREFERENCES_KEY) ?? 'null'));
    if (!value) return { ...DEFAULT_PERSONAL_PREFERENCES, hiddenModules: {} };
    const hidden: Record<string, ModuleId[]> = {};
    for (const [spaceId, modules] of Object.entries(record(value.hiddenModules) ?? {})) {
      if (Array.isArray(modules)) hidden[spaceId] = [...new Set(modules.filter(isModuleId))];
    }
    return {
      openJournalOnStartup: value.openJournalOnStartup === true,
      hiddenModules: hidden,
    };
  } catch {
    return { ...DEFAULT_PERSONAL_PREFERENCES, hiddenModules: {} };
  }
}

export function savePersonalPreferences(storage: PreferenceStorage, preferences: PersonalPreferences): void {
  storage.setItem(PERSONAL_PREFERENCES_KEY, JSON.stringify(preferences));
}

export function setModuleHidden(
  preferences: PersonalPreferences,
  spaceId: string,
  module: ModuleId,
  hidden: boolean,
): PersonalPreferences {
  const current = new Set(preferences.hiddenModules[spaceId] ?? []);
  if (hidden) current.add(module);
  else current.delete(module);
  return {
    ...preferences,
    hiddenModules: { ...preferences.hiddenModules, [spaceId]: [...current] },
  };
}

/** Modules whose entry points this person sees in this Space. */
export function visibleModules(
  features: SpaceFeatures,
  preferences: PersonalPreferences,
  spaceId: string,
): ModuleId[] {
  const hidden = new Set(preferences.hiddenModules[spaceId] ?? []);
  return MODULES
    .map((module) => module.id)
    .filter((id) => features.modules[id] && !hidden.has(id));
}
