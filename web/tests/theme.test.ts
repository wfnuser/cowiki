import assert from 'node:assert/strict';
import test from 'node:test';

import { loadTheme, resolvedTheme, saveTheme, THEME_STORAGE_KEY } from '../src/lib/theme.ts';

test('the theme follows the system unless the person picked one', () => {
  const values = new Map<string, string>();
  const storage = { getItem: (key: string) => values.get(key) ?? null, setItem: (key: string, value: string) => { values.set(key, value); } };
  assert.equal(loadTheme(storage), 'system');
  saveTheme('dark', storage);
  assert.equal(values.get(THEME_STORAGE_KEY), 'dark');
  assert.equal(loadTheme(storage), 'dark');
  values.set(THEME_STORAGE_KEY, 'purple');
  assert.equal(loadTheme(storage), 'system');
  const broken = { getItem: () => { throw new Error('blocked'); }, setItem: () => { throw new Error('blocked'); } };
  assert.equal(loadTheme(broken), 'system');
  assert.doesNotThrow(() => saveTheme('light', broken));
  assert.equal(resolvedTheme('system', true), 'dark');
  assert.equal(resolvedTheme('system', false), 'light');
  assert.equal(resolvedTheme('light', true), 'light');
});
