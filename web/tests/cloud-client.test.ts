import assert from 'node:assert/strict';
import test from 'node:test';

import {
  CloudApiError,
  createCloudClient,
  createPublicCloudClient,
  previewCloudInvitation,
  type CloudFetch,
} from '../src/cloud/client.ts';
import {
  canManageMembers,
  canManageTarget,
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

test('Cloud logout revokes the server credential', async () => {
  const calls: Array<{ url: string; init?: RequestInit }> = [];
  const fakeFetch: CloudFetch = async (input, init) => {
    calls.push({ url: String(input), init });
    return new Response(null, { status: 204 });
  };
  const client = createCloudClient(
    { baseUrl: 'https://cloud.cowiki.test', apiKey: 'cw_key_test', userId, userName: 'CoWiki' },
    fakeFetch,
  );

  await client.logout();

  assert.equal(calls[0].url, 'https://cloud.cowiki.test/api/auth/logout');
  assert.equal(calls[0].init?.method, 'POST');
  assert.equal(new Headers(calls[0].init?.headers).get('authorization'), 'Bearer cw_key_test');
});

test('Cloud Source lineage reads only the authenticated Source endpoint', async () => {
  const calls: string[] = [];
  const fakeFetch: CloudFetch = async (input) => {
    calls.push(String(input));
    return Response.json({ ref: 'main', oid: 'a'.repeat(40), path: '.cowiki/sources/interview.md', content: '# Interview' });
  };
  const client = createCloudClient(
    { baseUrl: 'https://cloud.cowiki.test', apiKey: 'key', userId, userName: 'User' },
    fakeFetch,
  );

  await client.getSourceContent(spaceId, '.cowiki/sources/interview.md');

  assert.deepEqual(calls, [
    `https://cloud.cowiki.test/api/spaces/${spaceId}/sources/content?ref=main&path=.cowiki%2Fsources%2Finterview.md`,
  ]);
});

test('Cloud comment and notification methods use the shared authenticated contract', async () => {
  const calls: Array<{ url: string; init?: RequestInit }> = [];
  const fakeFetch: CloudFetch = async (input, init) => {
    calls.push({ url: String(input), init });
    return init?.method === 'DELETE' || String(input).endsWith('/read-all')
      ? new Response(null, { status: 204 })
      : Response.json({ comments: [], snapshots: [] });
  };
  const client = createCloudClient(
    { baseUrl: 'https://cloud.cowiki.test', apiKey: 'key', userId, userName: 'User' },
    fakeFetch,
  );

  await client.listComments(spaceId, 'wiki/roadmap.md');
  await client.createComment(spaceId, {
    path: 'wiki/roadmap.md', body: '@reviewer check this', source: '# Roadmap', startLine: 1, endLine: 1,
  });
  await client.setCommentResolved(spaceId, 'comment-id', true);
  await client.deleteComment(spaceId, 'comment-id');
  await client.listNotifications();
  await client.notificationUnreadCount();
  await client.markAllNotificationsRead();

  assert.equal(calls[0].url, `https://cloud.cowiki.test/api/spaces/${spaceId}/comments?path=wiki%2Froadmap.md`);
  assert.deepEqual(JSON.parse(String(calls[1].init?.body)), {
    path: 'wiki/roadmap.md', body: '@reviewer check this', source: '# Roadmap', startLine: 1, endLine: 1,
  });
  assert.equal(calls[2].init?.method, 'PATCH');
  assert.deepEqual(JSON.parse(String(calls[2].init?.body)), { resolved: true });
  assert.equal(calls[3].init?.method, 'DELETE');
  assert.equal(calls[4].url, 'https://cloud.cowiki.test/api/notifications');
  assert.equal(calls[5].url, 'https://cloud.cowiki.test/api/notifications/unread-count');
  assert.equal(calls[6].url, 'https://cloud.cowiki.test/api/notifications/read-all');
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

test('Space visibility mutations preserve explicit public and private intent', async () => {
  const calls: Array<{ url: string; init?: RequestInit }> = [];
  const fakeFetch: CloudFetch = async (input, init) => {
    calls.push({ url: String(input), init });
    return Response.json({ id: spaceId, visibility: 'public' });
  };
  const client = createCloudClient(
    { baseUrl: 'https://cloud.cowiki.test', apiKey: 'key', userId, userName: 'User' },
    fakeFetch,
  );

  await client.createSpace('Competition', 'competition', 'public');
  await client.updateSpaceVisibility(spaceId, 'private');

  assert.deepEqual(JSON.parse(String(calls[0].init?.body)), {
    name: 'Competition',
    slug: 'competition',
    visibility: 'public',
  });
  assert.equal(calls[1].url, `https://cloud.cowiki.test/api/spaces/${spaceId}`);
  assert.equal(calls[1].init?.method, 'PATCH');
  assert.deepEqual(JSON.parse(String(calls[1].init?.body)), { visibility: 'private' });
});

test('Space creation capability can be read and unlocked with a trimmed invite code', async () => {
  const calls: Array<{ url: string; init?: RequestInit }> = [];
  const responses = [
    {
      authorized: false,
      createdCount: 0,
      limit: 2,
      canCreate: false,
      reason: 'invite_required',
    },
    {
      authorized: true,
      createdCount: 0,
      limit: 2,
      canCreate: true,
      reason: null,
    },
  ];
  const fakeFetch: CloudFetch = async (input, init) => {
    calls.push({ url: String(input), init });
    return Response.json(responses.shift());
  };
  const client = createCloudClient(
    { baseUrl: 'https://cloud.cowiki.test', apiKey: 'key', userId, userName: 'User' },
    fakeFetch,
  );

  const locked = await client.getSpaceCreationCapability();
  const unlocked = await client.redeemSpaceCreationInvite('  cw_space_test  ');

  assert.equal(locked.reason, 'invite_required');
  assert.equal(unlocked.canCreate, true);
  assert.deepEqual(calls.map((call) => call.url), [
    'https://cloud.cowiki.test/api/space-creation-capability',
    'https://cloud.cowiki.test/api/space-creation-capability/redeem',
  ]);
  assert.equal(calls[1].init?.method, 'POST');
  assert.deepEqual(JSON.parse(String(calls[1].init?.body)), { code: 'cw_space_test' });
  assert.equal(new Headers(calls[1].init?.headers).get('authorization'), 'Bearer key');
});

test('Cloud API failures retain the structured server error code', async () => {
  const fakeFetch: CloudFetch = async () => Response.json(
    { error: 'Space creation requires an invite', code: 'invite_required' },
    { status: 403 },
  );
  const client = createCloudClient(
    { baseUrl: 'https://cloud.cowiki.test', apiKey: 'key', userId, userName: 'User' },
    fakeFetch,
  );

  await assert.rejects(
    client.createSpace('Competition', 'competition'),
    (error: unknown) => error instanceof CloudApiError
      && error.status === 403
      && error.code === 'invite_required',
  );
});

test('public Cloud reads never send a bearer credential', async () => {
  const calls: Array<{ url: string; init?: RequestInit }> = [];
  const fakeFetch: CloudFetch = async (input, init) => {
    calls.push({ url: String(input), init });
    return Response.json([]);
  };
  const client = createPublicCloudClient('https://cloud.cowiki.test/', fakeFetch);

  await client.listSpaces();
  await client.getSpace('competition');
  await client.getTree('competition');
  await client.getContent('competition', 'project/info.md');

  assert.deepEqual(calls.map((call) => call.url), [
    'https://cloud.cowiki.test/api/public/spaces',
    'https://cloud.cowiki.test/api/public/spaces/competition',
    'https://cloud.cowiki.test/api/public/spaces/competition/tree?ref=main',
    'https://cloud.cowiki.test/api/public/spaces/competition/content?ref=main&path=project%2Finfo.md',
  ]);
  for (const call of calls) {
    assert.equal(new Headers(call.init?.headers).has('authorization'), false);
  }
});

test('public Cloud reads reject origins with embedded credentials or paths', () => {
  assert.throws(
    () => createPublicCloudClient('https://user:password@cloud.cowiki.test'),
    /origin without credentials/,
  );
  assert.throws(
    () => createPublicCloudClient('https://cloud.cowiki.test/api'),
    /origin without credentials/,
  );
});

test('role helpers preserve the Cloud permission matrix', () => {
  assert.equal(canManageMembers('manager'), true);
  assert.equal(canManageMembers('editor'), false);
  assert.equal(canManageTarget('manager', 'editor'), true);
  assert.equal(canManageTarget('manager', 'viewer'), true);
  assert.equal(canManageTarget('manager', 'manager'), false);
  assert.equal(canManageTarget('manager', 'owner'), false);
  assert.equal(canManageTarget('owner', 'manager'), true);
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

test('pull request diff is fetched from the reviewed head endpoint', async () => {
  const calls: string[] = [];
  const fakeFetch: CloudFetch = async (input) => {
    calls.push(String(input));
    return Response.json({
      baseOid: 'a'.repeat(40),
      headOid: 'b'.repeat(40),
      files: [{ path: 'index.md', status: 'modified', additions: 2, deletions: 1 }],
      patch: 'diff --git a/index.md b/index.md\n-old\n+new',
    });
  };
  const client = createCloudClient(
    { baseUrl: 'https://cloud.cowiki.test', apiKey: 'key', userId, userName: 'User' },
    fakeFetch,
  );
  const diff = await client.getPullRequestDiff(spaceId, '33333333-3333-4333-8333-333333333333');
  assert.equal(diff.files[0].additions, 2);
  assert.equal(
    calls[0],
    `https://cloud.cowiki.test/api/spaces/${spaceId}/pull-requests/33333333-3333-4333-8333-333333333333/diff`,
  );
});
