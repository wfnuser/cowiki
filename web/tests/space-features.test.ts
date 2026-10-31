import assert from 'node:assert/strict';
import test from 'node:test';

import {
  DEFAULT_SPACE_FEATURES,
  loadPersonalPreferences,
  parseSpaceFeatures,
  savePersonalPreferences,
  setModuleHidden,
  visibleModules,
  withSpaceFeatures,
} from '../src/lib/space-features.ts';

class MemoryStorage {
  private readonly values = new Map<string, string>();
  getItem(key: string): string | null { return this.values.get(key) ?? null; }
  setItem(key: string, value: string): void { this.values.set(key, value); }
}

test('missing or malformed Space config falls back to defaults', () => {
  assert.deepEqual(parseSpaceFeatures(null), DEFAULT_SPACE_FEATURES);
  assert.deepEqual(parseSpaceFeatures({ features: 'nope' }), DEFAULT_SPACE_FEATURES);
});

test('Space config keeps valid values and rejects unsafe paths', () => {
  const features = parseSpaceFeatures({
    features: {
      modules: { journal: false, tasks: true },
      journal: { folder: 'Daily/', format: 'YYYY/MM/DD', template: 'Templates/Daily' },
    },
  });
  assert.deepEqual(features, {
    modules: { journal: false, tasks: true },
    journal: { folder: 'Daily', format: 'YYYY/MM/DD', template: 'Templates/Daily.md' },
  });
  const unsafe = parseSpaceFeatures({
    features: { journal: { folder: '../outside', format: '../../x', template: '../secret' } },
  });
  assert.equal(unsafe.journal.folder, 'journal');
  assert.equal(unsafe.journal.format, 'YYYY-MM-DD');
  assert.equal(unsafe.journal.template, null);
});

test('writing features keeps other Space config keys', () => {
  const merged = withSpaceFeatures({ other: 1, features: { stale: true } }, DEFAULT_SPACE_FEATURES);
  assert.equal(merged.other, 1);
  assert.deepEqual(parseSpaceFeatures(merged), DEFAULT_SPACE_FEATURES);
});

test('personal preferences hide modules per Space without changing the Space', () => {
  const storage = new MemoryStorage();
  let preferences = loadPersonalPreferences(storage);
  assert.equal(preferences.openJournalOnStartup, false);
  preferences = setModuleHidden(preferences, 'space-a', 'tasks', true);
  preferences = { ...preferences, openJournalOnStartup: true };
  savePersonalPreferences(storage, preferences);

  const loaded = loadPersonalPreferences(storage);
  assert.equal(loaded.openJournalOnStartup, true);
  assert.deepEqual(visibleModules(DEFAULT_SPACE_FEATURES, loaded, 'space-a'), ['journal']);
  assert.deepEqual(visibleModules(DEFAULT_SPACE_FEATURES, loaded, 'space-b'), ['journal', 'tasks']);

  const disabled = { ...DEFAULT_SPACE_FEATURES, modules: { journal: false, tasks: true } };
  assert.deepEqual(visibleModules(disabled, loaded, 'space-b'), ['tasks']);

  storage.setItem('cowiki.personal.preferences', '{broken');
  assert.deepEqual(loadPersonalPreferences(storage).hiddenModules, {});
});
