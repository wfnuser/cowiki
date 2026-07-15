import assert from 'node:assert/strict';
import test from 'node:test';

import { localSpaceIdentityFromPath } from '../src/local-space.ts';

test('a selected macOS folder becomes the local Space name and slug', () => {
  assert.deepEqual(
    localSpaceIdentityFromPath('/Users/octo/Documents/Research Notes/'),
    { name: 'Research Notes', slug: 'research-notes' },
  );
});

test('a selected Windows folder uses the final path segment', () => {
  assert.deepEqual(
    localSpaceIdentityFromPath('C:\\Users\\octo\\My Wiki'),
    { name: 'My Wiki', slug: 'my-wiki' },
  );
});

test('folder names without ASCII letters still receive a usable slug', () => {
  assert.deepEqual(
    localSpaceIdentityFromPath('/Users/octo/Documents/中文笔记'),
    { name: '中文笔记', slug: 'local-space' },
  );
});
