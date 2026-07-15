import { isAgentKind, type AgentKind } from './agents.ts';

export type DefaultAgent = AgentKind;

export interface ClientSettings {
  defaultAgent: DefaultAgent;
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
      && isAgentKind((value as Partial<ClientSettings>).defaultAgent)
    ) {
      return { defaultAgent: (value as ClientSettings).defaultAgent };
    }
  } catch {
    // Ignore malformed local settings and keep the client usable.
  }

  return { ...DEFAULT_CLIENT_SETTINGS };
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
