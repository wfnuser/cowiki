import { isAgentKind, type AgentKind } from './agents.ts';

export type DefaultAgent = AgentKind;

export interface ClientSettings {
  /** The Agent new sessions start with: the one used last, or picked in Settings. */
  defaultAgent: DefaultAgent;
  /** The model last picked for each Agent (ids the Agent listed). */
  agentModels?: Partial<Record<AgentKind, string>>;
}

export interface ClientSettingsStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

export type SettingsTab = 'client' | 'account' | 'keys';

export const CLIENT_SETTINGS_STORAGE_KEY = 'cowiki.client.settings';

const DEFAULT_CLIENT_SETTINGS: ClientSettings = { defaultAgent: 'codex' };

export function loadClientSettings(storage: ClientSettingsStorage): ClientSettings {
  try {
    const value: unknown = JSON.parse(storage.getItem(CLIENT_SETTINGS_STORAGE_KEY) ?? 'null');
    if (
      value
      && typeof value === 'object'
    ) {
      const defaultAgent = (value as Record<string, unknown>).defaultAgent;
      const agentModels = modelsFrom((value as Record<string, unknown>).agentModels);
      if (defaultAgent === 'gemini') {
        return { defaultAgent: 'antigravity', ...agentModels };
      }
      if (isAgentKind(defaultAgent)) {
        return { defaultAgent, ...agentModels };
      }
    }
  } catch {
    // Ignore malformed local settings and keep the client usable.
  }

  return { ...DEFAULT_CLIENT_SETTINGS };
}

function modelsFrom(value: unknown): { agentModels?: Partial<Record<AgentKind, string>> } {
  if (!value || typeof value !== 'object') return {};
  const models = Object.fromEntries(
    Object.entries(value as Record<string, unknown>)
      .filter(([agent, model]) => isAgentKind(agent) && typeof model === 'string' && model.length > 0 && model.length <= 200),
  ) as Partial<Record<AgentKind, string>>;
  return Object.keys(models).length > 0 ? { agentModels: models } : {};
}

export function saveClientSettings(
  storage: ClientSettingsStorage,
  settings: ClientSettings,
): void {
  storage.setItem(CLIENT_SETTINGS_STORAGE_KEY, JSON.stringify(settings));
}

export function settingsTabs({
  clientMode,
  cloudConnected,
}: {
  clientMode: boolean;
  cloudConnected: boolean;
}): SettingsTab[] {
  const tabs: SettingsTab[] = clientMode ? ['client', 'account'] : ['account'];
  if (cloudConnected) tabs.push('keys');
  return tabs;
}
