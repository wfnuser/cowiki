import assert from 'node:assert/strict';
import test from 'node:test';

import {
  CloudApiError,
  createCloudClient,
  type CloudFetch,
} from '../src/cloud/client.ts';
import {
  canManageMembers,
  canMerge,
  canPush,
  normalizeCloudSession,
} from '../src/cloud/session.ts';
import { cloudSpaceRoute, parseCloudRoute } from '../src/cloud/routes.ts';

const userId = '11111111-1111-4111-8111-111111111111';
const spaceId = '22222222-2222-4222-8222-222222222222';

test('injected Cloud sessions are normalized and reject unsafe origins', () => {
  assert.deepEqual(
    normalizeCloudSession({
      baseUrl: 'https://cloud.cowiki.test/',
      apiKey: ' cw_key_test ',
      userId,
      userName: ' CoWiki User ',
    }),
    {
      baseUrl: 'https://cloud.cowiki.test',
      apiKey: 'cw_key_test',
      userId,
      userName: 'CoWiki User',
    },
  );

  for (const baseUrl of [
    'https://user:secret@cloud.cowiki.test',
    'https://cloud.cowiki.test/path',
    'https://cloud.cowiki.test/?token=secret',
  ]) {
    assert.throws(() => normalizeCloudSession({ baseUrl, apiKey: 'key', userId, userName: 'User' }));
  }
});

test('typed Cloud requests always carry the injected bearer credential', async () => {
  const calls: Array<{ url: string; init?: RequestInit }> = [];
  const responses = [
    { user: { id: userId, handle: 'cowiki', displayName: 'CoWiki', avatarUrl: null } },
    [],
    { ref: 'main', oid: 'a'.repeat(40), entries: [] },
  ];
  const fakeFetch: CloudFetch = async (input, init) => {
    calls.push({ url: String(input), init });
    return Response.json(responses.shift());
  };
  const client = createCloudClient(
    { baseUrl: 'https://cloud.cowiki.test', apiKey: 'cw_key_test', userId, userName: 'CoWiki' },
    fakeFetch,
  );

  await client.currentUser();
  await client.listSpaces();
  await client.getTree(spaceId);

  assert.deepEqual(calls.map((call) => call.url), [
    'https://cloud.cowiki.test/api/me',
    'https://cloud.cowiki.test/api/spaces',
    `https://cloud.cowiki.test/api/spaces/${spaceId}/tree?ref=main`,
  ]);
  for (const call of calls) {
    assert.equal(new Headers(call.init?.headers).get('authorization'), 'Bearer cw_key_test');
  }
});

test('Cloud mutations serialize the current contract and surface typed failures', async () => {
  const calls: Array<{ url: string; init?: RequestInit }> = [];
  const fakeFetch: CloudFetch = async (input, init) => {
    calls.push({ url: String(input), init });
    if (calls.length === 3) {
      return Response.json({ error: 'pull request head changed' }, { status: 409 });
    }
    return Response.json({ ok: true });
  };
  const client = createCloudClient(
    { baseUrl: 'https://cloud.cowiki.test', apiKey: 'key', userId, userName: 'User' },
    fakeFetch,
  );

  await client.setMember(spaceId, 'ada', 'editor');
  await client.approvePullRequest(spaceId, '33333333-3333-4333-8333-333333333333');
  await assert.rejects(
    client.mergePullRequest(
      spaceId,
      '33333333-3333-4333-8333-333333333333',
      'b'.repeat(40),
    ),
    (error: unknown) => error instanceof CloudApiError
      && error.status === 409
      && error.message === 'pull request head changed',
  );

  assert.equal(calls[0].init?.method, 'POST');
  assert.deepEqual(JSON.parse(String(calls[0].init?.body)), { handle: 'ada', role: 'editor' });
  assert.deepEqual(JSON.parse(String(calls[2].init?.body)), { expectedHeadOid: 'b'.repeat(40) });
});

test('role helpers preserve the Cloud permission matrix', () => {
  assert.equal(canManageMembers('manager'), true);
  assert.equal(canManageMembers('editor'), false);
  assert.equal(canMerge('editor'), false);
  assert.equal(canPush('editor'), true);
  assert.equal(canPush('viewer'), false);
});

test('UUID Space routes round-trip document paths', () => {
  const pathname = cloudSpaceRoute(spaceId, 'wiki', 'guides/Shared Context.md');
  assert.equal(
    pathname,
    `/cloud/spaces/${spaceId}/wiki/guides/Shared%20Context.md`,
  );
  assert.deepEqual(parseCloudRoute(pathname), {
    spaceId,
    view: 'wiki',
    documentPath: 'guides/Shared Context.md',
  });
  assert.equal(parseCloudRoute('/cloud/spaces/not-a-uuid/wiki'), null);
});
