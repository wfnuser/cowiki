import assert from 'node:assert/strict';
import test from 'node:test';

import {
  DEFAULT_SPACE_FEATURES,
  loadPersonalPreferences,
  parseSpaceFeatures,
  savePersonalPreferences,
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
      templates: { folder: '/Meta/Templates/' },
      // Left over from the removed weekly review module.
      review: { folder: 'journal' },
    },
  });
  assert.deepEqual(features, {
    modules: { journal: false, tasks: true, flashcards: true },
    journal: { folder: 'Daily', format: 'YYYY/MM/DD', template: 'Templates/Daily.md' },
    templates: { folder: 'Meta/Templates' },
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

test('personal preferences and the modules a Space turns on', () => {
  const storage = new MemoryStorage();
  let preferences = loadPersonalPreferences(storage);
  assert.deepEqual(preferences, { openJournalOnStartup: false, autoCheckpoint: true });
  preferences = { openJournalOnStartup: true, autoCheckpoint: false };
  savePersonalPreferences(storage, preferences);
  assert.deepEqual(loadPersonalPreferences(storage), preferences);
  // Older saved preferences may still carry the removed per-person module hiding.
  storage.setItem('cowiki.personal.preferences', JSON.stringify({ openJournalOnStartup: true, hiddenModules: { a: ['tasks'] } }));
  assert.deepEqual(loadPersonalPreferences(storage), { openJournalOnStartup: true, autoCheckpoint: true });

  assert.deepEqual(visibleModules(DEFAULT_SPACE_FEATURES), ['journal', 'tasks', 'flashcards']);
  const disabled = { ...DEFAULT_SPACE_FEATURES, modules: { journal: false, tasks: true, flashcards: false } };
  assert.deepEqual(visibleModules(disabled), ['tasks']);

  storage.setItem('cowiki.personal.preferences', '{broken');
  assert.deepEqual(loadPersonalPreferences(storage), { openJournalOnStartup: false, autoCheckpoint: true });
});
