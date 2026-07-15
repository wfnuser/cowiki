import assert from 'node:assert/strict';
import test from 'node:test';

import {
  loadClientSettings,
  saveClientSettings,
  settingsTabs,
} from '../src/lib/client-settings.ts';

class MemoryStorage {
  private readonly values = new Map<string, string>();

  getItem(key: string): string | null {
    return this.values.get(key) ?? null;
  }

  setItem(key: string, value: string): void {
    this.values.set(key, value);
  }
}

test('client settings default to Codex and persist the selected agent', () => {
  const storage = new MemoryStorage();

  assert.deepEqual(loadClientSettings(storage), { defaultAgent: 'codex' });

  saveClientSettings(storage, { defaultAgent: 'claude' });
  assert.deepEqual(loadClientSettings(storage), { defaultAgent: 'claude' });

  for (const defaultAgent of ['grok', 'gemini', 'opencode', 'hermes'] as const) {
    saveClientSettings(storage, { defaultAgent });
    assert.deepEqual(loadClientSettings(storage), { defaultAgent });
  }
});

test('malformed client settings fall back to safe defaults', () => {
  const storage = new MemoryStorage();
  storage.setItem('cowiki.client.settings', JSON.stringify({ defaultAgent: 'unknown' }));

  assert.deepEqual(loadClientSettings(storage), { defaultAgent: 'codex' });
});

test('desktop Cloud sessions keep client settings and gain account tools', () => {
  assert.deepEqual(settingsTabs({ clientMode: true, cloudConnected: true }), [
    'client',
    'account',
    'keys',
  ]);
});

test('unsigned desktop sessions expose sign-in but not Cloud API keys', () => {
  assert.deepEqual(settingsTabs({ clientMode: true, cloudConnected: false }), [
    'client',
    'account',
  ]);
});

test('the hosted app keeps account and API-key settings', () => {
  assert.deepEqual(settingsTabs({ clientMode: false, cloudConnected: true }), [
    'account',
    'keys',
  ]);
});
