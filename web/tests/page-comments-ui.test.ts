import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { createServer, type ViteDevServer } from 'vite';
import { JSDOM } from 'jsdom';
import { act } from 'react';

const webRoot = fileURLToPath(new URL('../', import.meta.url));
let vite: ViteDevServer;

test.before(async () => {
  vite = await createServer({ root: webRoot, appType: 'custom', logLevel: 'silent', server: { middlewareMode: true } });
});

test.after(async () => {
  await vite.close();
});

test('comments stay closed until the reader asks to see them', async () => {
  const { CommentsHeaderToggle, CommentsPanel, CommentsProvider } = await vite.ssrLoadModule('/src/components/PageCommentsLayer.tsx');
  const store = {
    scope: 'cloud', scopeLabel: 'Cloud shared', currentUserId: 'user-1', currentUserName: 'Reader',
    list: async () => ({ comments: [], snapshots: [] }), listMembers: async () => [],
    create: async () => { throw new Error('not called during server render'); },
    setResolved: async () => { throw new Error('not called during server render'); },
    delete: async () => { throw new Error('not called during server render'); },
  };
  const html = renderToStaticMarkup(React.createElement(
    CommentsProvider,
    { store, pageSlug: 'index.md', source: '# Page', articleRef: { current: null } },
    React.createElement(React.Fragment, null, React.createElement(CommentsHeaderToggle), React.createElement(CommentsPanel)),
  ));

  assert.match(html, /aria-controls="page-comments-panel"/);
  assert.match(html, /aria-expanded="false"/);
  assert.doesNotMatch(html, /<aside/);
});

test('Cloud comment timestamps accept the backend calendar tuple', async () => {
  const { cloudPageCommentStore } = await vite.ssrLoadModule('/src/lib/page-comment-store.ts');
  const tuple = [2026, 247, 3, 24, 3, 865_492_000, 0, 0, 0];
  const client = {
    listComments: async () => ({ comments: [{
      id: 'comment-1', pagePath: 'index.md', userId: 'user-1', userHandle: 'reader', userName: 'Reader',
      userAvatarUrl: null, contentHash: null, startLine: null, endLine: null, body: 'Looks good',
      parentId: null, resolved: false, createdAt: tuple, updatedAt: '2026-09-04T03:24:03.865Z',
    }], snapshots: [] }),
  };
  const response = await cloudPageCommentStore(client, 'space-1', 'user-1', 'Reader').list('index.md');

  assert.equal(response.comments[0].created_at, '2026-09-04T03:24:03.865Z');
  assert.equal(response.comments[0].updated_at, '2026-09-04T03:24:03.865Z');
});

test('a linked Space never falls back to local comments while offline or checking its link', async () => {
  const { desktopPageCommentStore } = await vite.ssrLoadModule('/src/lib/page-comment-store.ts');
  const client = {};
  const session = { userId: 'user-1', userName: 'Reader' };
  assert.equal(desktopPageCommentStore('local-space', undefined, client, session), null);
  assert.equal(desktopPageCommentStore('local-space', { slug: 'other', id: null }, client, session), null);
  assert.equal(desktopPageCommentStore('local-space', { slug: 'local-space', id: 'cloud-id' }, null, null), null);
  assert.equal(desktopPageCommentStore('local-space', { slug: 'local-space', id: 'cloud-id' }, client, session)?.scope, 'cloud');
  assert.equal(desktopPageCommentStore('local-space', { slug: 'local-space', id: null }, null, null)?.scope, 'local');
});

test('desktop embeds the Cloud mention inbox without a second shell and updates its unread badge', async () => {
  const dom = new JSDOM('<!doctype html><div id="root"></div>', { url: 'http://localhost' });
  Object.assign(globalThis, { window: dom.window, document: dom.window.document, HTMLElement: dom.window.HTMLElement, Node: dom.window.Node, IS_REACT_ACT_ENVIRONMENT: true });
  const { createRoot } = await import('react-dom/client');
  const { MemoryRouter } = await import('react-router-dom');
  const { CloudNotificationsPage } = await vite.ssrLoadModule('/src/cloud/CloudNotificationsPage.tsx');
  const { createCloudClient } = await vite.ssrLoadModule('/src/cloud/client.ts');
  const container = document.getElementById('root')!;
  const root = createRoot(container);
  const session = { baseUrl: 'https://cloud.example', apiKey: 'test-key', userId: '11111111-1111-4111-8111-111111111111', userName: 'Reader' };
  const notification = { id: 'mention-1', spaceId: '22222222-2222-4222-8222-222222222222', spaceName: 'Team', pagePath: 'wiki/meeting.md', commentId: 'comment-1', actorHandle: 'reviewer', commentBody: 'Please check this note', read: false, createdAt: '2026-09-12T01:00:00Z' };
  const requests: Array<{ url: string; method: string; body: unknown }> = [];
  const originalFetch = globalThis.fetch;
  globalThis.fetch = async (url, options) => {
    requests.push({ url: String(url), method: options?.method ?? 'GET', body: options?.body });
    return new Response(JSON.stringify(String(url).endsWith('/notifications') ? [notification] : []), { status: 200, headers: { 'Content-Type': 'application/json' } });
  };
  let unread = -1;
  try {
    await act(async () => root.render(React.createElement(MemoryRouter, null,
      React.createElement(CloudNotificationsPage, { client: createCloudClient(session), session,
        embedded: true, onUnreadChange: (count: number) => { unread = count; }, onSignOut: () => undefined }),
    )));
    assert.match(container.textContent ?? '', /@reviewer mentioned you in Team/);
    assert.match(container.textContent ?? '', /Please check this note/);
    assert.equal(unread, 1, 'desktop badge receives the same inbox unread count');
    assert.equal(container.querySelector('.h-screen'), null, 'an embedded inbox must not add another full-screen rail');
    const mention = Array.from(container.querySelectorAll('button')).find((el) => el.textContent?.includes('Please check this note'))!;
    await act(async () => mention.click());
    assert.equal(unread, 0);
    assert.ok(requests.some((request) => request.url.endsWith('/api/notifications/mention-1') && request.method === 'PATCH' && request.body === '{"read":true}'));
  } finally {
    await act(async () => root.unmount());
    globalThis.fetch = originalFetch;
    dom.window.close();
  }
});

test('comments reject stale page responses and show a failed load instead of silently hiding the panel', async () => {
  const dom = new JSDOM('<!doctype html><div id="root"></div>', { url: 'http://localhost' });
  Object.assign(globalThis, { window: dom.window, document: dom.window.document, HTMLElement: dom.window.HTMLElement, Node: dom.window.Node, IS_REACT_ACT_ENVIRONMENT: true });
  const { createRoot } = await import('react-dom/client');
  const { CommentsHeaderToggle, CommentsPanel, CommentsProvider } = await vite.ssrLoadModule('/src/components/PageCommentsLayer.tsx');
  const container = document.getElementById('root')!;
  const root = createRoot(container);
  let finishOld!: (value: unknown) => void;
  const store = {
    scope: 'cloud', scopeLabel: 'Cloud shared', currentUserId: 'user-1', currentUserName: 'Reader',
    list: (path: string) => path === 'old.md' ? new Promise((resolve) => { finishOld = resolve; })
      : path === 'failed.md' ? Promise.reject(new Error('Cloud is offline')) : Promise.resolve({ comments: [], snapshots: [] }),
    listMembers: async () => [],
  };
  const render = async (pageSlug: string) => {
    await act(async () => root.render(React.createElement(CommentsProvider,
      { store, pageSlug, source: '# Page', articleRef: { current: null } },
      React.createElement(React.Fragment, null, React.createElement(CommentsHeaderToggle), React.createElement(CommentsPanel)),
    )));
  };
  try {
    await render('old.md');
    await render('new.md');
    await act(async () => finishOld({ comments: [{ id: 'old-comment', user_id: 'user-1', parent_id: null,
      body: 'Private old page comment', resolved: false, content_hash: null, start_line: null, end_line: null,
      created_at: '2026-09-01T00:00:00Z' }], snapshots: [] }));
    assert.equal(container.querySelector('button')?.textContent?.trim(), '0');
    await act(async () => container.querySelector('button')!.click());
    assert.ok(container.querySelector('#page-comments-panel'), 'the empty panel should open');
    assert.doesNotMatch(container.textContent ?? '', /Private old page comment/);
    await render('failed.md');
    if (container.querySelector('button')?.getAttribute('aria-expanded') === 'false') {
      await act(async () => container.querySelector('button')!.click());
    }
    assert.match(container.textContent ?? '', /Cloud is offline/);
  } finally {
    await act(async () => root.unmount());
    dom.window.close();
  }
});

test('reply submission is single-flight, preserves failed drafts and exposes mention choices', async () => {
  const dom = new JSDOM('<!doctype html><div id="root"></div>', { url: 'http://localhost' });
  Object.assign(globalThis, { window: dom.window, document: dom.window.document, HTMLElement: dom.window.HTMLElement, Node: dom.window.Node, IS_REACT_ACT_ENVIRONMENT: true });
  const { createRoot } = await import('react-dom/client');
  const { CommentsHeaderToggle, CommentsPanel, CommentsProvider } = await vite.ssrLoadModule('/src/components/PageCommentsLayer.tsx');
  const container = document.getElementById('root')!;
  const root = createRoot(container);
  let rejectSubmission!: (error: Error) => void;
  let calls = 0;
  const store = {
    key: 'cloud:space:user', scope: 'cloud', scopeLabel: 'Cloud shared', currentUserId: 'user-1', currentUserName: 'Reader',
    list: async () => ({ comments: [{ id: 'comment-1', user_id: 'user-1', parent_id: null,
      body: 'Review this page', resolved: false, content_hash: 'snapshot', start_line: 1, end_line: 1,
      created_at: '2026-09-01T00:00:00Z' }], snapshots: [{ content_hash: 'snapshot', source: '# Page' }] }),
    listMembers: async () => [{ id: 'reviewer', name: 'Reviewer', mention: 'reviewer' }],
    create: () => { calls += 1; return new Promise((_, reject) => { rejectSubmission = reject; }); },
    setResolved: async () => { throw new Error('Resolve failed: offline'); },
  };
  const button = (label: string) => Array.from(container.querySelectorAll('button')).find((el) => el.textContent?.trim() === label)!;
  try {
    await act(async () => root.render(React.createElement(CommentsProvider,
      { store, pageSlug: 'page.md', source: '# Page', articleRef: { current: null } },
      React.createElement(React.Fragment, null, React.createElement(CommentsHeaderToggle), React.createElement(CommentsPanel)),
    )));
    await act(async () => container.querySelector('button')!.click());
    await act(async () => button('Reply').click());
    const textarea = container.querySelector('textarea')!;
    await act(async () => {
      Object.getOwnPropertyDescriptor(dom.window.HTMLTextAreaElement.prototype, 'value')!.set!.call(textarea, '@rev');
      textarea.dispatchEvent(new dom.window.Event('input', { bubbles: true }));
    });
    assert.match(container.textContent ?? '', /@reviewer/);
    // A menu placed outside its composer cannot be inside an overflow-clipped wrapper.
    const suggestion = Array.from(container.querySelectorAll('button')).find((el) => el.textContent?.includes('@reviewer'))!;
    for (let parent = suggestion.parentElement!.parentElement; parent && parent !== container; parent = parent.parentElement) {
      assert.notEqual(parent.style.overflow, 'hidden', 'mention choices must not be clipped');
    }
    await act(async () => { button('Send').click(); button('Send').click(); });
    assert.equal(calls, 1);
    assert.equal(button('Sending…')?.disabled, true);
    await act(async () => rejectSubmission(new Error('Could not reach Cloud')));
    assert.match(container.textContent ?? '', /Could not reach Cloud/);
    assert.equal(textarea.value, '@rev');
    assert.equal(button('Send').disabled, false);
    await act(async () => button('Cancel').click());
    await act(async () => button('Resolve').click());
    assert.match(container.textContent ?? '', /Resolve failed: offline/);
  } finally {
    await act(async () => root.unmount());
    dom.window.close();
  }
});
