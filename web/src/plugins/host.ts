import type { PersonalPreferences, SpaceFeatures } from '../lib/space-features.ts';
import type { SpacePageAdapter, SpacePagePorts } from './space-pages.ts';
import { createSpacePageAdapter, PluginUnavailableError } from './space-pages.ts';

export interface PluginSnapshot {
  space: { id: string; local: boolean } | null;
  /** Changes when a Space is activated, including a return to a cached Space. */
  activation: number;
  readiness: 'pending' | 'ready' | 'failed';
  /** null is never default-enabled; readiness distinguishes waiting from failure. */
  features: SpaceFeatures | null;
  preferences: PersonalPreferences;
}

export interface PluginTextSetting {
  id: string;
  label: string;
  placeholder?: string;
  monospace?: boolean;
  value: (features: SpaceFeatures) => string;
  update: (features: SpaceFeatures, value: string) => SpaceFeatures;
}

export interface PluginPreferenceSetting {
  id: string;
  label: string;
  value: (preferences: PersonalPreferences) => boolean;
  update: (preferences: PersonalPreferences, value: boolean) => PersonalPreferences;
}

export interface BuiltinPlugin {
  id: string;
  label: string;
  description: string;
  enabled: (features: SpaceFeatures) => boolean;
  setEnabled: (features: SpaceFeatures, enabled: boolean) => SpaceFeatures;
  actions: readonly {
    id: string;
    label: string;
    icon: 'calendar' | 'file';
    run: (context: { features: SpaceFeatures; pages: SpacePageAdapter; now: () => Date }) => Promise<void>;
  }[];
  settings?: {
    title: string;
    fields: readonly PluginTextSetting[];
    help: string;
    preview?: (features: SpaceFeatures, now: Date) => string;
    preferences?: readonly PluginPreferenceSetting[];
  };
  launch?: { action: string; when: (preferences: PersonalPreferences) => boolean };
}

export interface PluginAction {
  id: string;
  label: string;
  icon: 'calendar' | 'file';
}

export interface PluginHostOptions {
  definitions: readonly BuiltinPlugin[];
  getSnapshot: () => PluginSnapshot;
  pages: (space: NonNullable<PluginSnapshot['space']>, action: PluginAction, isCurrent: () => boolean) => SpacePagePorts;
  now?: () => Date;
}

/** Called on committed activations (or an explicit departure), not during render. */
export function createSpaceActivationTracker() {
  let spaceId: string | null = null;
  let activation = 0;
  return (id: string | null) => {
    if (id !== spaceId) { spaceId = id; activation++; }
    return activation;
  };
}

export function createPluginHost({ definitions, getSnapshot, pages, now = () => new Date() }: PluginHostOptions) {
  const plugins = new Map<string, BuiltinPlugin>();
  const commands = new Map<string, { plugin: BuiltinPlugin; action: BuiltinPlugin['actions'][number] }>();
  for (const plugin of definitions) {
    if (plugins.has(plugin.id)) throw new Error(`Duplicate built-in plugin ID: ${plugin.id}`);
    plugins.set(plugin.id, plugin);
    for (const action of plugin.actions) {
      const id = `${plugin.id}:${action.id}`;
      if (commands.has(id)) throw new Error(`Duplicate built-in action ID: ${id}`);
      commands.set(id, { plugin, action });
    }
  }
  let launched = false;
  let launchActivation: number | null = null;
  type Availability = Pick<PluginSnapshot, 'space' | 'readiness' | 'features'>;
  const permitted = (plugin: BuiltinPlugin, snapshot: Availability) =>
    !!snapshot.space?.local && snapshot.readiness === 'ready' && !!snapshot.features && plugin.enabled(snapshot.features);
  const dispatch = async (id: string): Promise<boolean> => {
    const command = commands.get(id);
    const snapshot = getSnapshot();
    if (!command || !permitted(command.plugin, snapshot)) return false;
    const isCurrent = () => {
      const current = getSnapshot();
      return current.space?.id === snapshot.space!.id
        && current.activation === snapshot.activation
        && current.features === snapshot.features && permitted(command.plugin, current);
    };
    try {
      await command.action.run({ features: snapshot.features!, now,
        pages: createSpacePageAdapter(pages(snapshot.space!, { id, label: command.action.label, icon: command.action.icon }, isCurrent), isCurrent) });
      return isCurrent();
    } catch (error) {
      if (error instanceof PluginUnavailableError) return false;
      throw error;
    }
  };
  return {
    actions: (snapshot: Availability = getSnapshot()): PluginAction[] => {
      return [...commands].filter(([, { plugin }]) => permitted(plugin, snapshot))
        .map(([id, { action }]) => ({ id, label: action.label, icon: action.icon }));
    },
    dispatch,
    cancelLaunch() { launched = true; },
    async launch({ workspacesLoaded, initialPathname }: { workspacesLoaded: boolean; initialPathname: string }): Promise<void> {
      if (launched) return;
      if (initialPathname !== '/' && initialPathname !== '') { launched = true; return; }
      const snapshot = getSnapshot();
      if (!snapshot.space) return;
      // Observe the initial visible activation even while boot navigation or
      // configuration is pending. Failure/departure permanently consumes launch.
      launchActivation ??= snapshot.activation;
      if (snapshot.activation !== launchActivation || snapshot.readiness === 'failed' || !snapshot.space.local) { launched = true; return; }
      if (!workspacesLoaded || snapshot.readiness !== 'ready' || !snapshot.features) return;
      launched = true;
      for (const plugin of definitions) {
        if (plugin.launch?.when(snapshot.preferences)) await dispatch(`${plugin.id}:${plugin.launch.action}`);
      }
    },
  };
}
