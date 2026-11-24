import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import {
  canCreateCheckpoint,
  checkpointTime,
  defaultCheckpointName,
  draftChangeLabel,
} from '../src/lib/history.ts';

const mainLayout = readFileSync(new URL('../src/pages/MainLayout.tsx', import.meta.url), 'utf8');
const spacePanel = readFileSync(new URL('../src/components/layout/SpacePanel.tsx', import.meta.url), 'utf8');
const historyView = readFileSync(new URL('../src/components/views/HistoryView.tsx', import.meta.url), 'utf8');

test('checkpoint names use the user local date and time', () => {
  const localTime = new Date(2026, 6, 15, 23, 5);

  assert.equal(defaultCheckpointName(localTime), 'Checkpoint 2026-07-15 23:05');
});

test('the current draft says how many files changed', () => {
  assert.equal(draftChangeLabel(0), 'No changes');
  assert.equal(draftChangeLabel(1), '1 file changed');
  assert.equal(draftChangeLabel(3), '3 files changed');
});

test('checkpoint times read like a person says them', () => {
  const now = new Date(2026, 10, 24, 18, 0);
  assert.equal(checkpointTime(new Date(2026, 10, 24, 16, 5), now, 'en-US'), '4:05 PM');
  assert.equal(checkpointTime(new Date(2026, 10, 23, 23, 59), now, 'en-US'), 'Yesterday, 11:59 PM');
  assert.equal(checkpointTime(new Date(2026, 10, 2, 9, 30), now, 'en-US'), 'Nov 2, 9:30 AM');
  assert.equal(checkpointTime(new Date(2025, 11, 31, 9, 30), now, 'en-US'), 'Dec 31, 2025, 9:30 AM');
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

test('History shares the left-aligned content grid', () => {
  assert.match(historyView, /width: 'min\(860px, 100%\)'/);
  assert.doesNotMatch(historyView, /margin: '0 auto'/);
});
