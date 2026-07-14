import assert from 'node:assert/strict';
import test from 'node:test';

import {
  clampSidebarWidth,
  loadSidebarLayout,
  saveSidebarLayout,
  SIDEBAR_DEFAULT_WIDTH,
  SIDEBAR_MAX_WIDTH,
  SIDEBAR_MIN_WIDTH,
} from '../src/lib/sidebar-layout.ts';

class MemoryStorage {
  private readonly values = new Map<string, string>();

  getItem(key: string) {
    return this.values.get(key) ?? null;
  }

  setItem(key: string, value: string) {
    this.values.set(key, value);
  }
}

test('sidebar width remains within the usable range', () => {
  assert.equal(clampSidebarWidth(100), SIDEBAR_MIN_WIDTH);
  assert.equal(clampSidebarWidth(340), 340);
  assert.equal(clampSidebarWidth(800), SIDEBAR_MAX_WIDTH);
});

test('sidebar layout survives an application restart', () => {
  const storage = new MemoryStorage();
  saveSidebarLayout(storage, { width: 376, collapsed: true });

  assert.deepEqual(loadSidebarLayout(storage), { width: 376, collapsed: true });
});

test('invalid persisted values fall back to a safe layout', () => {
  const storage = new MemoryStorage();
  storage.setItem('cowiki.sidebar.layout', '{"width":"wide","collapsed":"no"}');

  assert.deepEqual(loadSidebarLayout(storage), {
    width: SIDEBAR_DEFAULT_WIDTH,
    collapsed: false,
  });
});
