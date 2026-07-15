import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import { defaultCheckpointName, draftChangeLabel } from '../src/lib/history.ts';

const mainLayout = readFileSync(new URL('../src/pages/MainLayout.tsx', import.meta.url), 'utf8');
const spacePanel = readFileSync(new URL('../src/components/layout/SpacePanel.tsx', import.meta.url), 'utf8');

test('checkpoint names use the user local date and time', () => {
  const localTime = new Date(2026, 6, 15, 23, 5);

  assert.equal(defaultCheckpointName(localTime), 'Checkpoint 2026-07-15 23:05');
});

test('the current draft distinguishes saved files from checkpoints', () => {
  assert.equal(draftChangeLabel(0), 'No saved changes in the current draft');
  assert.equal(draftChangeLabel(1), '1 saved file in the current draft');
  assert.equal(draftChangeLabel(3), '3 saved files in the current draft');
});

test('space navigation replaces Activity with History', () => {
  assert.doesNotMatch(spacePanel, /label: 'Activity'/);
  assert.match(spacePanel, /label: 'History'/);
  assert.doesNotMatch(mainLayout, /kind: 'activity'/);
  assert.match(mainLayout, /kind: 'history'/);
});
