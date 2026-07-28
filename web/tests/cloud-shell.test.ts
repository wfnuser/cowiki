import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import {
  cloudNavigation,
  memberManagementMode,
  mergeActionVisible,
  resolveInitialCloudPage,
} from '../src/cloud/cloud-shell-model.ts';

const app = readFileSync(new URL('../src/App.tsx', import.meta.url), 'utf8');
const cloudApp = readFileSync(new URL('../src/cloud/CloudApp.tsx', import.meta.url), 'utf8');
const cloudHome = readFileSync(new URL('../src/cloud/CloudHome.tsx', import.meta.url), 'utf8');
const wiki = readFileSync(new URL('../src/cloud/CloudWikiView.tsx', import.meta.url), 'utf8');
const reviews = readFileSync(new URL('../src/cloud/CloudReviewsView.tsx', import.meta.url), 'utf8');
const members = readFileSync(new URL('../src/cloud/CloudMembersView.tsx', import.meta.url), 'utf8');
const invitation = readFileSync(new URL('../src/cloud/CloudInvitationPage.tsx', import.meta.url), 'utf8');

test('browser routing has a focused Cloud shell with no Tauri dependency', () => {
  assert.match(app, /path="\/cloud\/\*"/);
  assert.match(cloudHome, /listSpaces\(\)/);
  assert.match(cloudApp, /CloudSpaceView/);
  for (const source of [cloudApp, cloudHome, wiki, reviews, members]) {
    assert.doesNotMatch(source, /@tauri-apps|local-api|invoke\(/);
  }
});

test('Cloud Space navigation exposes read surfaces to every member', () => {
  assert.deepEqual(cloudNavigation('viewer').map((item) => item.id), ['wiki', 'reviews', 'members']);
  assert.deepEqual(cloudNavigation('owner').map((item) => item.id), ['wiki', 'reviews', 'members']);
});

test('Editor and Viewer never receive management or merge actions', () => {
  assert.equal(memberManagementMode('owner'), 'manage');
  assert.equal(memberManagementMode('manager'), 'manage');
  assert.equal(memberManagementMode('editor'), 'read');
  assert.equal(memberManagementMode('viewer'), 'read');
  assert.equal(mergeActionVisible('editor'), false);
  assert.equal(mergeActionVisible('viewer'), false);
  assert.equal(mergeActionVisible('manager'), true);
  assert.doesNotMatch(reviews, /Continue Rebase|Abort Rebase/);
});

test('read-only Wiki selects index.md before the first available page', () => {
  assert.equal(resolveInitialCloudPage([
    { path: 'guides', kind: 'folder' },
    { path: 'guides/start.md', kind: 'page' },
    { path: 'index.md', kind: 'page' },
  ]), 'index.md');
  assert.equal(resolveInitialCloudPage([
    { path: 'guides', kind: 'folder' },
    { path: 'guides/start.md', kind: 'page' },
  ]), 'guides/start.md');
  assert.match(wiki, /ReactMarkdown/);
  assert.match(wiki, /remarkGfm/);
});

test('member and PR mutations reload server-authoritative state', () => {
  assert.match(members, /await loadMembers\(\)/);
  assert.match(reviews, /await loadPullRequests\(\)/);
  assert.match(reviews, /expectedHeadOid|headOid/);
});

test('Space invitation route remains readable before sign in and accepts into one Space', () => {
  assert.match(app, /path="\/invite\/:token"/);
  assert.match(invitation, /previewCloudInvitation/);
  assert.match(invitation, /Sign in with GitHub/);
  assert.match(invitation, /acceptInvitation/);
  assert.match(invitation, /cloudSpaceRoute/);
  assert.doesNotMatch(invitation, /@tauri-apps|invoke\(/);
});

test('Owners and Managers administer Space-scoped invitation links', () => {
  assert.match(members, /Invite link/);
  assert.match(members, /createInvitation/);
  assert.match(members, /listInvitations/);
  assert.match(members, /revokeInvitation/);
  assert.match(members, /Copy link/);
  assert.match(members, /Seven days/);
  assert.match(members, /mode === 'manage'/);
});
