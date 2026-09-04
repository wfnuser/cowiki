import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { createServer, type ViteDevServer } from 'vite';

const webRoot = fileURLToPath(new URL('../', import.meta.url));
let vite: ViteDevServer;

test.before(async () => {
  vite = await createServer({
    root: webRoot,
    appType: 'custom',
    logLevel: 'silent',
    server: { middlewareMode: true },
  });
});

test.after(async () => {
  await vite.close();
});

test('page lineage is a collapsed disclosure instead of permanent page chrome', async () => {
  const { PageReader } = await vite.ssrLoadModule('/src/components/PageReader.tsx');
  const html = renderToStaticMarkup(React.createElement(PageReader, {
    body: '# Durable knowledge\n\nThe page stays focused on reading.',
    lineage: {
      sources: ['.cowiki/sources/interview.md'],
      agents: [{ name: 'Codex', changeId: 'change-1', task: 'Organize interview' }],
      commit: { oid: '0123456789abcdef', summary: 'Compile interview', author: 'Qinghao', committedAt: 1_725_000_000 },
      review: { id: 'review-id', number: 12, title: 'Compile interview' },
    },
  }));

  assert.match(html, /<details[^>]*aria-label="Page lineage"/);
  assert.doesNotMatch(html, /<details[^>]*\sopen(?:=|\s|>)/);
  assert.match(html, /<summary[^>]*>.*Lineage.*1 source/s);
  assert.ok(
    html.indexOf('Durable knowledge') < html.indexOf('aria-label="Page lineage"'),
    'lineage belongs after the document instead of above its title',
  );
});

test('comments stay closed until the reader asks to see them', async () => {
  const { CommentsHeaderToggle, CommentsPanel, CommentsProvider } = await vite.ssrLoadModule('/src/components/PageCommentsLayer.tsx');
  const store = {
    scope: 'cloud',
    scopeLabel: 'Cloud shared',
    currentUserId: 'user-1',
    currentUserName: 'Reader',
    list: async () => ({ comments: [], snapshots: [] }),
    listMembers: async () => [],
    create: async () => { throw new Error('not called during server render'); },
    setResolved: async () => { throw new Error('not called during server render'); },
    delete: async () => { throw new Error('not called during server render'); },
  };
  const html = renderToStaticMarkup(React.createElement(
    CommentsProvider,
    { store, pageSlug: 'index.md', source: '# Page', articleRef: { current: null } },
    React.createElement(React.Fragment, null,
      React.createElement(CommentsHeaderToggle),
      React.createElement(CommentsPanel),
    ),
  ));

  assert.match(html, /aria-controls="page-comments-panel"/);
  assert.match(html, /aria-expanded="false"/);
  assert.doesNotMatch(html, /<aside/);
});

test('Cloud comment timestamps accept the backend calendar tuple', async () => {
  const { cloudPageCommentStore } = await vite.ssrLoadModule('/src/lib/page-comment-store.ts');
  const tuple = [2026, 247, 3, 24, 3, 865_492_000, 0, 0, 0];
  const client = {
    listComments: async () => ({
      comments: [{
        id: 'comment-1', pagePath: 'index.md', userId: 'user-1', userHandle: 'reader', userName: 'Reader',
        userAvatarUrl: null, contentHash: null, startLine: null, endLine: null, body: 'Looks good',
        parentId: null, resolved: false, createdAt: tuple, updatedAt: '2026-09-04T03:24:03.865Z',
      }],
      snapshots: [],
    }),
  };
  const response = await cloudPageCommentStore(client, 'space-1', 'user-1', 'Reader').list('index.md');

  assert.equal(response.comments[0].created_at, '2026-09-04T03:24:03.865Z');
  assert.equal(response.comments[0].updated_at, '2026-09-04T03:24:03.865Z');
});
