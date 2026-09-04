import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { createServer, type ViteDevServer } from 'vite';

const webRoot = fileURLToPath(new URL('../', import.meta.url));
const mainLayoutSource = await import('node:fs').then(({ readFileSync }) => (
  readFileSync(new URL('../src/pages/MainLayout.tsx', import.meta.url), 'utf8')
));
let vite: ViteDevServer;

test.before(async () => {
  vite = await createServer({ root: webRoot, appType: 'custom', logLevel: 'silent', server: { middlewareMode: true } });
});

test.after(async () => {
  await vite.close();
});

test('workspace sync context stays in the compact Cloud action, not the breadcrumb', async () => {
  const { WorkspaceContextBadge } = await vite.ssrLoadModule('/src/components/layout/WorkspaceContextBadge.tsx');
  const html = renderToStaticMarkup(React.createElement(WorkspaceContextBadge, {
    context: {
      kind: 'linked',
      label: 'Local + Cloud · Not uploaded',
      detail: 'Local changes have not been submitted to Cloud',
      attention: true,
    },
    onClick: () => undefined,
  }));

  assert.match(html, />Cloud</);
  assert.match(html, /title="Local changes have not been submitted to Cloud"/);
  assert.doesNotMatch(html, /Local \+ Cloud|Not uploaded/);

  const breadcrumb = mainLayoutSource
    .split('<ContentBreadcrumb>')[1]
    .split('</ContentBreadcrumb>')[0];
  const actions = mainLayoutSource
    .split('<ContentHeaderActions>')[1]
    .split('</ContentHeaderActions>')[0];
  assert.doesNotMatch(breadcrumb, /WorkspaceContextBadge/);
  assert.match(actions, /WorkspaceContextBadge/);
  assert.match(actions, /connected=\{workspaceContext\.kind === 'linked'\}/);
});

test('an unlinked Space keeps the explicit Publish to Cloud action', async () => {
  const { WorkspaceContextBadge } = await vite.ssrLoadModule('/src/components/layout/WorkspaceContextBadge.tsx');
  const html = renderToStaticMarkup(React.createElement(WorkspaceContextBadge, {
    context: {
      kind: 'local',
      label: 'Local only',
      detail: 'This Space has not been published to Cloud',
      attention: false,
    },
    connected: false,
    onClick: () => undefined,
  }));

  assert.match(html, />Publish to Cloud</);
  assert.match(html, /title="Publish to Cloud"/);
  assert.doesNotMatch(html, /Local only/);
});
