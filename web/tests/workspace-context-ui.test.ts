import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import React from 'react';
import { MemoryRouter } from 'react-router-dom';
import { renderToStaticMarkup } from 'react-dom/server';
import { createServer } from 'vite';
import { workspaceContextStatus } from '../src/lib/workspace-context.ts';

test('the hosted Space shell actually exposes the shared Cloud context badge', async () => {
  const vite = await createServer({ root: fileURLToPath(new URL('../', import.meta.url)), appType: 'custom', logLevel: 'silent', server: { middlewareMode: true } });
  try {
    const { CloudSpaceView } = await vite.ssrLoadModule('/src/cloud/CloudSpaceView.tsx');
    const html = renderToStaticMarkup(React.createElement(MemoryRouter, null,
      React.createElement(CloudSpaceView, {
        client: {}, onSignOut: () => undefined,
        session: { userName: 'Reader', userId: '11111111-1111-4111-8111-111111111111' },
        route: { spaceId: '22222222-2222-4222-8222-222222222222', view: 'wiki' },
      }),
    ));
    assert.match(html, /title="Shared Cloud Space"[^>]*>[\s\S]*?<span>Cloud<\/span>/);
  } finally {
    await vite.close();
  }
});

test('the visible header distinguishes local-only, pending upload and Cloud updates', async () => {
  const vite = await createServer({ root: fileURLToPath(new URL('../', import.meta.url)), appType: 'custom', logLevel: 'silent', server: { middlewareMode: true } });
  try {
    const { WorkspaceContextBadge } = await vite.ssrLoadModule('/src/components/layout/WorkspaceContextBadge.tsx');
    for (const [state, label] of [
      ['unlinked', 'Local only'], ['dirty', 'Not uploaded'], ['needsSync', 'Cloud update'], ['conflicted', 'Attention'],
    ] as const) {
      const html = renderToStaticMarkup(React.createElement(WorkspaceContextBadge, {
        context: workspaceContextStatus({ desktop: true, state }), connected: state !== 'unlinked',
      }));
      // Inspect visible text, not the tooltip: readers must not need to hover to learn where their changes live.
      const visible = html.replace(/<[^>]*>/g, '');
      assert.ok(visible.includes(label), `Expected ${label} in visible header, got ${visible}`);
      if (state === 'unlinked') {
        assert.match(html, /title="[^"]*publish to Cloud/i, 'local-only context explains the publish action');
      }
    }
  } finally {
    await vite.close();
  }
});
