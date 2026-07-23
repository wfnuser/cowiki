import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const mainLayout = readFileSync(new URL('../src/pages/MainLayout.tsx', import.meta.url), 'utf8');
const spacePanel = readFileSync(
  new URL('../src/components/layout/SpacePanel.tsx', import.meta.url),
  'utf8',
);
const desktopCapability = JSON.parse(
  readFileSync(new URL('../src-tauri/capabilities/default.json', import.meta.url), 'utf8'),
) as { permissions: string[] };

test('the desktop top bar exposes a deep Tauri drag region', () => {
  assert.match(mainLayout, /data-tauri-drag-region="deep"/);
});

test('the Space title bar drags the window instead of selecting its label', () => {
  const titleBar = spacePanel
    .split('{/* Space name header */}')[1]
    .split('{/* Nav items */}')[0];
  assert.match(titleBar, /data-tauri-drag-region="deep"/);
  assert.match(titleBar, /userSelect: 'none'/);
});

test('the main window is allowed to start native window dragging', () => {
  assert.ok(desktopCapability.permissions.includes('core:window:allow-start-dragging'));
});
