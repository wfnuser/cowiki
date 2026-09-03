import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import { workspaceContextStatus } from '../src/lib/workspace-context.ts';

const mainLayout = readFileSync(new URL('../src/pages/MainLayout.tsx', import.meta.url), 'utf8');

test('hosted Spaces are always identified as Cloud', () => {
  assert.deepEqual(workspaceContextStatus({ desktop: false }), {
    kind: 'cloud',
    label: 'Cloud',
    detail: 'Shared Cloud Space',
    attention: false,
  });
});

test('local Spaces distinguish unlinked, unsent, synced, and review states', () => {
  assert.equal(workspaceContextStatus({ desktop: true, state: 'unlinked' }).label, 'Local only');
  assert.equal(
    workspaceContextStatus({ desktop: true, state: 'upToDate', hasLocalChanges: true }).label,
    'Local + Cloud · Not uploaded',
  );
  assert.equal(workspaceContextStatus({ desktop: true, state: 'synced' }).label, 'Local + Cloud · Synced');
  assert.equal(workspaceContextStatus({ desktop: true, state: 'submitted' }).label, 'Local + Cloud · In review');
});

test('sync problems are visible and never described as uploaded', () => {
  const needsSync = workspaceContextStatus({ desktop: true, state: 'needsSync' });
  const conflict = workspaceContextStatus({ desktop: true, state: 'conflicted' });
  assert.equal(needsSync.label, 'Local + Cloud · Cloud update');
  assert.equal(conflict.label, 'Local + Cloud · Attention');
  assert.equal(conflict.attention, true);
  assert.doesNotMatch(`${needsSync.label} ${conflict.label}`, /uploaded/i);
});

test('active-Space context is keyed so a previous Space cannot leak into the header', () => {
  assert.match(mainLayout, /desktopCloudStatus\.spaceSlug === activeWorkspace\?\.slug/);
  assert.match(mainLayout, /workingDiffsSpaceSlug === activeWorkspace\?\.slug/);
  assert.match(mainLayout, /WorkspaceContextBadge/);
});
