import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import {
  cloudNavigation,
  memberManagementMode,
  mergeActionVisible,
  resolveInitialCloudPage,
  routeScopedValue,
} from '../src/cloud/cloud-shell-model.ts';

const app = readFileSync(new URL('../src/App.tsx', import.meta.url), 'utf8');
const cloudApp = readFileSync(new URL('../src/cloud/CloudApp.tsx', import.meta.url), 'utf8');
const cloudHome = readFileSync(new URL('../src/cloud/CloudHome.tsx', import.meta.url), 'utf8');
const cloudSpace = readFileSync(new URL('../src/cloud/CloudSpaceView.tsx', import.meta.url), 'utf8');
const spacePanel = readFileSync(new URL('../src/components/layout/SpacePanel.tsx', import.meta.url), 'utf8');
const contentHeader = readFileSync(new URL('../src/components/layout/ContentHeader.tsx', import.meta.url), 'utf8');
const pageReader = readFileSync(new URL('../src/components/PageReader.tsx', import.meta.url), 'utf8');
const mainLayout = readFileSync(new URL('../src/pages/MainLayout.tsx', import.meta.url), 'utf8');
const wiki = readFileSync(new URL('../src/cloud/CloudWikiView.tsx', import.meta.url), 'utf8');
const quickSetup = readFileSync(new URL('../src/cloud/CloudQuickSetup.tsx', import.meta.url), 'utf8');
const reviews = readFileSync(new URL('../src/cloud/CloudReviewsView.tsx', import.meta.url), 'utf8');
const localReviews = readFileSync(new URL('../src/components/review/LocalReviewInbox.tsx', import.meta.url), 'utf8');
const members = readFileSync(new URL('../src/cloud/CloudMembersView.tsx', import.meta.url), 'utf8');
const invitation = readFileSync(new URL('../src/cloud/CloudInvitationPage.tsx', import.meta.url), 'utf8');
const publicReader = readFileSync(new URL('../src/cloud/PublicCloudSpacePage.tsx', import.meta.url), 'utf8');

test('browser routing has a focused Cloud shell with no Tauri dependency', () => {
  assert.match(app, /path="\/cloud\/\*"/);
  assert.match(cloudHome, /listSpaces\(\)/);
  assert.match(cloudApp, /CloudSpaceView/);
  for (const source of [cloudApp, cloudHome, wiki, reviews, members]) {
    assert.doesNotMatch(source, /@tauri-apps|local-api|invoke\(/);
  }
  assert.match(cloudApp, /await client\.logout\(\)/);
  assert.match(cloudHome, /New shared Space/);
  assert.match(cloudHome, /createSpace/);
  assert.match(cloudHome, /navigate\(cloudSpaceRoute\(created\.id\)\)/);
});

test('public Space routes render merged Markdown without a session', () => {
  assert.match(app, /path="\/spaces\/:slug\/\*"/);
  assert.match(publicReader, /createPublicCloudClient/);
  assert.match(publicReader, /PageReader/);
  assert.match(publicReader, /<main className="flex min-w-0 flex-1 flex-col">/);
  assert.match(publicReader, /<div className="relative min-h-0 flex-1">/);
  assert.doesNotMatch(publicReader, /Authorization|CloudMembersView|CloudReviewsView/);
});

test('Cloud reuses the client Space rail and keeps the zero-Space state quiet', () => {
  assert.match(cloudHome, /SpaceRail/);
  assert.match(cloudSpace, /SpaceRail/);
  assert.match(cloudHome, /Join a Space/);
  assert.match(cloudHome, /invitation link/i);
  assert.doesNotMatch(cloudHome, /Use invitation link/);
  assert.doesNotMatch(cloudHome, />Shared Spaces</);
  assert.doesNotMatch(cloudSpace, /CloudHeader/);
});

test('Cloud Space reuses the client panel, knowledge tree, and page reader', () => {
  assert.match(cloudSpace, /SpacePanel/);
  assert.match(cloudSpace, /readOnly/);
  assert.match(spacePanel, /readOnly/);
  assert.match(wiki, /PageReader/);
  assert.match(mainLayout, /PageReader/);
  assert.match(cloudSpace, /CloudApiError/);
  assert.match(cloudSpace, /status === 404/);
});

test('desktop and every Cloud tab reuse the exact same content header shell', () => {
  assert.match(mainLayout, /ContentHeader/);
  assert.match(cloudSpace, /ContentHeader/);
  assert.match(cloudSpace, /ContentBreadcrumb/);
  assert.doesNotMatch(wiki, /ContentHeader/);
  assert.match(contentHeader, /borderBottom: `1px solid \$\{C\.line\}`/);
});

test('Cloud Wiki presents published knowledge without Git or authoring controls', () => {
  assert.doesNotMatch(wiki, />main</);
  assert.doesNotMatch(wiki, /Read only|GitBranch|tree\.oid/);
  assert.doesNotMatch(wiki, /className="[^"]*\bborder-b\b/);
  assert.doesNotMatch(wiki, /VersionSwitcher|Switch version|Publish to Cloud/);
  assert.doesNotMatch(wiki, />\s*Agent\s*</);
  assert.doesNotMatch(wiki, />\s*Edit\s*</);
  assert.doesNotMatch(wiki, /readOnlyLabel=/);
});

test('an empty Space gives its Owner a concise Agent quick setup', () => {
  assert.match(wiki, /CloudQuickSetup/);
  assert.match(wiki, /space\.role === 'owner'/);
  assert.match(quickSetup, /Quick setup/);
  assert.match(quickSetup, /padding: '36px 56px 56px'/);
  assert.match(quickSetup, /className="page-title mb-0"/);
  assert.match(quickSetup, /Install the CoWiki skill/);
  assert.match(quickSetup, /Choose a local Space/);
  assert.match(quickSetup, /Publish with your Agent/);
  assert.match(quickSetup, /Future updates appear after review/);
  assert.match(quickSetup, /\/skill\.md/);
  assert.match(quickSetup, /choose a local CoWiki Space/);
  assert.match(quickSetup, /navigator\.clipboard\.writeText/);
  assert.match(quickSetup, /Copy prompt/);
  assert.match(quickSetup, /Waiting for the first published version/);
  assert.doesNotMatch(quickSetup, /\bgit\b|\bbranch\b|\bclone\b/i);
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
  assert.match(wiki, /PageReader/);
  assert.match(pageReader, /ReactMarkdown/);
  assert.match(pageReader, /remarkGfm/);
});

test('a previous Space tree is hidden while the selected Space loads', () => {
  const previousTree = {
    ref: 'main',
    oid: 'old-tree',
    entries: [{ path: 'old-page.md', kind: 'page' as const }],
  };
  const loaded = { spaceId: 'space-old', value: previousTree };

  assert.equal(routeScopedValue('space-new', loaded), null);
  assert.equal(routeScopedValue('space-old', loaded), previousTree);
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

test('Members let Owners and Managers delegate roles within the target matrix', () => {
  assert.match(members, /canManageMembers/);
  assert.match(members, /canManageTarget/);
  assert.match(members, /setMember/);
  assert.match(members, /updateSpaceVisibility/);
  assert.match(members, /Public|Private/);
  assert.match(members, /'manager'/);
  assert.match(members, /'editor'/);
  assert.match(members, /'viewer'/);
  assert.match(members, /createInvitation/);
  assert.match(members, /Invite link/);
  assert.doesNotMatch(members, /removeMember/);
  assert.match(members, /padding: '36px 56px 56px'/);
  assert.match(reviews, /padding: '36px 56px 56px'/);
});

test('Cloud review loads and renders the exact Markdown diff before merge', () => {
  assert.match(reviews, /getPullRequestDiff/);
  assert.match(reviews, /cloudDiffToFileDiffs/);
  assert.match(reviews, /DiffView/);
  assert.match(reviews, /authorName/);
  assert.match(reviews, /canMerge\(space\.role\)/);
  assert.doesNotMatch(reviews, /dangerouslySetInnerHTML/);
});

test('local and Cloud Reviews reuse the same inbox presentation', () => {
  assert.match(localReviews, /ReviewInbox/);
  assert.match(localReviews, /ReviewInboxRow/);
  assert.match(reviews, /ReviewInbox/);
  assert.match(reviews, /ReviewInboxRow/);
});
