import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { createServer, type ViteDevServer } from 'vite';

const webRoot = fileURLToPath(new URL('../', import.meta.url));
let vite: ViteDevServer;

test.before(async () => {
  vite = await createServer({ root: webRoot, appType: 'custom', logLevel: 'silent', server: { middlewareMode: true } });
});

test.after(async () => {
  await vite.close();
});

test('page lineage stays collapsed after the document until requested', async () => {
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

  assert.match(html, /aria-expanded="false"/);
  assert.doesNotMatch(html, /<aside/);
  assert.match(html, /Sources &amp; records/);
  assert.match(html, /1 source/);
  assert.ok(
    html.indexOf('Durable knowledge') < html.indexOf('Sources &amp; records'),
    'lineage belongs after the document instead of above its title',
  );
});
