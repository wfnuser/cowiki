import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import {
  canCreateCheckpoint,
  defaultCheckpointName,
  draftChangeLabel,
} from '../src/lib/history.ts';

const mainLayout = readFileSync(new URL('../src/pages/MainLayout.tsx', import.meta.url), 'utf8');
const spacePanel = readFileSync(new URL('../src/components/layout/SpacePanel.tsx', import.meta.url), 'utf8');

test('checkpoint names use the user local date and time', () => {
  const localTime = new Date(2026, 6, 15, 23, 5);

  assert.equal(defaultCheckpointName(localTime), 'Checkpoint 2026-07-15 23:05');
});

test('the current draft distinguishes saved files from checkpoints', () => {
  assert.equal(draftChangeLabel(0, false), 'No saved changes in the current draft');
  assert.equal(draftChangeLabel(1, false), '1 saved file in the current draft');
  assert.equal(draftChangeLabel(3, true), '3 saved files changed since the latest checkpoint');
  assert.equal(draftChangeLabel(0, true), 'No changes since the latest checkpoint');
});

test('a checkpoint requires at least one change from its baseline', () => {
  assert.equal(canCreateCheckpoint(undefined), false);
  assert.equal(canCreateCheckpoint(0), false);
  assert.equal(canCreateCheckpoint(1), true);
});

test('space navigation replaces Activity with History', () => {
  assert.doesNotMatch(spacePanel, /label: 'Activity'/);
  assert.match(spacePanel, /label: 'History'/);
  assert.doesNotMatch(mainLayout, /kind: 'activity'/);
  assert.match(mainLayout, /kind: 'history'/);
});
