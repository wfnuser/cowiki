import assert from 'node:assert/strict';
import test from 'node:test';

import {
  CloudApiError,
  createCloudClient,
  previewCloudInvitation,
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

test('Space invitations have a public preview and authenticated lifecycle', async () => {
  const calls: Array<{ url: string; init?: RequestInit }> = [];
  const fakeFetch: CloudFetch = async (input, init) => {
    calls.push({ url: String(input), init });
    if (calls.length === 1) {
      return Response.json({
        spaceId,
        spaceName: 'Competition',
        spaceSlug: 'competition',
        role: 'editor',
        expiresAt: '2026-08-01T00:00:00Z',
      });
    }
    if (calls.length === 2) {
      return Response.json({ id: spaceId, role: 'editor' });
    }
    if (calls.length === 3) {
      return Response.json({
        id: 'invite-id',
        spaceId,
        role: 'editor',
        expiresAt: '2026-08-01T00:00:00Z',
        acceptedCount: 0,
        createdAt: '2026-07-28T00:00:00Z',
        token: 'cw_invite_test',
        inviteUrl: 'https://cloud.cowiki.test/invite/cw_invite_test',
      }, { status: 201 });
    }
    if (calls.length === 4) return Response.json([]);
    return new Response(null, { status: 204 });
  };
  const preview = await previewCloudInvitation(
    'https://cloud.cowiki.test',
    'cw_invite_test',
    fakeFetch,
  );
  assert.equal(preview.spaceName, 'Competition');
  const client = createCloudClient(
    { baseUrl: 'https://cloud.cowiki.test', apiKey: 'key', userId, userName: 'User' },
    fakeFetch,
  );
  await client.acceptInvitation('cw_invite_test');
  await client.createInvitation(spaceId, 'editor', 168);
  await client.listInvitations(spaceId);
  await client.revokeInvitation(spaceId, 'invite-id');

  assert.equal(new Headers(calls[0].init?.headers).has('authorization'), false);
  assert.equal(calls[1].url, 'https://cloud.cowiki.test/api/invitations/cw_invite_test/accept');
  assert.equal(new Headers(calls[1].init?.headers).get('authorization'), 'Bearer key');
  assert.deepEqual(JSON.parse(String(calls[2].init?.body)), {
    role: 'editor',
    expiresInHours: 168,
  });
  assert.equal(calls[4].init?.method, 'DELETE');
});
