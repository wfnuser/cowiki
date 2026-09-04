import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import type { CloudSyncResult } from '../src/local-api.ts';
import {
  cloudDialogModel,
  cloudPullRequestUrl,
} from '../src/components/cloud/cloud-space-model.ts';

const dialog = readFileSync(new URL('../src/components/cloud/CloudSpaceDialog.tsx', import.meta.url), 'utf8');
const mainLayout = readFileSync(new URL('../src/pages/MainLayout.tsx', import.meta.url), 'utf8');
const workspaceContextAction = readFileSync(new URL('../src/components/layout/WorkspaceContextBadge.tsx', import.meta.url), 'utf8');

function result(state: CloudSyncResult['state'], overrides: Partial<CloudSyncResult> = {}): CloudSyncResult {
  return {
    state,
    conflicts: [],
    committed: false,
    message: state,
    pullRequest: null,
    cloudSpaceId: state === 'unlinked' ? null : '22222222-2222-4222-8222-222222222222',
    cloudBaseUrl: state === 'unlinked' ? null : 'https://cloud.cowiki.test',
    ...overrides,
  };
}

test('unlinked local Spaces publish explicitly and confirm dirty work', () => {
  assert.deepEqual(cloudDialogModel(result('unlinked'), false), {
    kind: 'publish',
    title: 'Publish to Cloud',
    primaryLabel: 'Publish Space',
    commitMessageRequired: false,
    safeStop: false,
  });
  assert.equal(cloudDialogModel(result('unlinked'), true).commitMessageRequired, true);
});

test('linked dirty Spaces submit with a confirmed commit message', () => {
  const model = cloudDialogModel(result('dirty'), true);
  assert.equal(model.kind, 'submit');
  assert.equal(model.primaryLabel, 'Commit and submit');
  assert.equal(model.commitMessageRequired, true);
});

test('clean linked Spaces can sync and submit their existing commits', () => {
  assert.equal(cloudDialogModel(result('needsSync'), false).kind, 'sync-submit');
  assert.equal(cloudDialogModel(result('upToDate'), false).kind, 'submit');
});

test('submitted pull requests link to the browser review', () => {
  const model = cloudDialogModel(result('submitted', {
    pullRequest: {
      id: '33333333-3333-4333-8333-333333333333',
      number: 7,
      title: 'Share research notes',
      headRef: 'user/11111111-1111-4111-8111-111111111111',
      headOid: 'a'.repeat(40),
      status: 'open',
    },
  }), false);
  assert.equal(model.kind, 'submitted');
  assert.equal(
    cloudPullRequestUrl(result('submitted', {
      pullRequest: {
        id: '33333333-3333-4333-8333-333333333333', number: 7, title: 'PR',
        headRef: 'user/id', headOid: 'a'.repeat(40), status: 'open',
      },
    })),
    'https://cloud.cowiki.test/cloud/spaces/22222222-2222-4222-8222-222222222222/reviews/33333333-3333-4333-8333-333333333333',
  );
});

test('Open in browser delegates to the desktop external URL boundary', () => {
  assert.match(dialog, /openExternalUrl/);
  assert.doesNotMatch(dialog, /window\.open/);
});

test('conflicts stop safely without exposing Git recovery jargon', () => {
  const model = cloudDialogModel(result('conflicted', { conflicts: ['wiki/intro.md'] }), false);
  assert.equal(model.kind, 'attention');
  assert.equal(model.safeStop, true);
  assert.equal(model.primaryLabel, null);
  assert.doesNotMatch(dialog, /Continue Rebase|Abort Rebase/);
});

test('desktop shell exposes one Space-scoped Cloud entry', () => {
  assert.match(mainLayout, /CloudSpaceDialog/);
  assert.match(mainLayout, /WorkspaceContextBadge/);
  assert.match(workspaceContextAction, /Publish to Cloud/);
  assert.doesNotMatch(dialog, /API key|api key/i);
});
