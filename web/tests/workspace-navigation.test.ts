import assert from 'node:assert/strict';
import test from 'node:test';

import type { PageMeta } from '../src/api.ts';
import { resolveWorkspaceSwitchTarget } from '../src/lib/workspace-navigation.ts';

test('switching to an uncached Space opens its first page after the tree loads', async () => {
  const loadedPages: PageMeta[] = [{
    slug: 'knowledge',
    path: 'knowledge',
    title: 'Knowledge',
    summary: '',
    branch: 'main',
    kind: 'folder',
    children: [{
      slug: 'knowledge/start',
      path: 'knowledge/start.md',
      title: 'Start',
      summary: '',
      branch: 'main',
      kind: 'page',
      children: [],
    }],
  }];

  const target = await resolveWorkspaceSwitchTarget(
    undefined,
    async () => loadedPages,
  );

  assert.deepEqual(target, {
    conceptId: 'knowledge/start',
    path: 'knowledge/start.md',
  });
});
