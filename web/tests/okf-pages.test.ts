import assert from 'node:assert/strict';
import test from 'node:test';

import type { PageMeta } from '../src/api.ts';
import {
  conceptIdFromPath,
  findConcept,
  firstConcept,
  isSearchableConceptPath,
  isReservedDocument,
  pageRoute,
  submitConceptPaths,
  shouldAttemptSubmit,
  visiblePageTree,
} from '../src/lib/okf-pages.ts';

function page(path: string, title = path): PageMeta {
  return {
    slug: path.replace(/\.md$/i, ''), path, title, summary: '', branch: 'local', kind: 'page', children: [],
  };
}

function folder(path: string, children: PageMeta[]): PageMeta {
  return {
    slug: path, path, title: path.split('/').at(-1) || path, summary: '', branch: 'local', kind: 'folder', children,
  };
}

test('an OKF bundle is one arbitrary hierarchy and legacy names stay ordinary folders', () => {
  const tree = visiblePageTree([
    page('index.md'),
    page('log.md'),
    folder('research', [page('research/index.md'), page('research/paper.md', 'Paper')]),
    folder('wiki', [page('wiki/legacy.md', 'Legacy')]),
    folder('entities', [page('entities/alice.md', 'Alice')]),
  ]);

  assert.deepEqual(tree.map((item) => item.path), ['entities', 'research', 'wiki']);
  assert.deepEqual(tree[1].children.map((item) => item.path), ['research/paper.md']);
});

test('a single real wiki folder titled Knowledge keeps its full Concept IDs', () => {
  const tree = visiblePageTree([
    {
      ...folder('wiki', [page('wiki/legacy.md', 'Legacy')]),
      slug: 'wiki', title: 'Knowledge',
    },
  ]);

  assert.deepEqual(tree.map((item) => item.path), ['wiki']);
  assert.equal(tree[0].children[0].path, 'wiki/legacy.md');
  assert.equal(tree[0].children[0].slug, 'wiki/legacy');
});

test('CoWiki source storage and every hidden subtree stay out of the page tree', () => {
  const tree = visiblePageTree([
    folder('.cowiki', [folder('.cowiki/sources', [page('.cowiki/sources/raw.md')])]),
    folder('notes', [page('notes/visible.md'), page('notes/Index.md'), page('notes/Log.md')]),
  ]);

  assert.deepEqual(tree.map((item) => item.path), ['notes']);
  assert.deepEqual(tree[0].children.map((item) => item.path), [
    'notes/Index.md', 'notes/Log.md', 'notes/visible.md',
  ]);
  assert.deepEqual(submitConceptPaths(tree), [
    'notes/Index.md', 'notes/Log.md', 'notes/visible.md',
  ]);
  assert.equal(isReservedDocument('notes/index.md'), true);
  assert.equal(isReservedDocument('notes/log.md'), true);
  assert.equal(isReservedDocument('notes/Index.md'), false);
  assert.equal(isReservedDocument('notes/Log.md'), false);
  assert.equal(isReservedDocument('notes/catalog.md'), false);
  assert.equal(isSearchableConceptPath('.cowiki/sources/raw.md'), false);
  assert.equal(isSearchableConceptPath('research/index.md'), false);
  assert.equal(isSearchableConceptPath('research/paper.md'), true);
});

test('full repo-relative paths are stable Concept IDs, including duplicate filenames', () => {
  const tree = visiblePageTree([
    folder('alpha', [page('alpha/overview.md', 'Alpha')]),
    folder('beta', [page('beta/overview.md', 'Beta')]),
  ]);

  assert.equal(conceptIdFromPath('alpha/overview.md'), 'alpha/overview');
  assert.equal(findConcept(tree, 'beta/overview')?.title, 'Beta');
  assert.equal(firstConcept(tree)?.path, 'alpha/overview.md');
  assert.equal(pageRoute('my-space', '研究/Local First'), '/my-space/%E7%A0%94%E7%A9%B6/Local%20First');
});

test('a local source-only Space may submit an empty Concept path list', () => {
  assert.deepEqual(submitConceptPaths([]), []);
  assert.equal(shouldAttemptSubmit(true, []), true);
  assert.equal(shouldAttemptSubmit(false, []), false);
});

test('submit paths recursively include canonical Concept files only', () => {
  const tree = visiblePageTree([
    folder('research', [page('research/paper.md')]),
    page('overview.md'),
  ]);

  assert.deepEqual(submitConceptPaths(tree), ['research/paper.md', 'overview.md']);
  assert.equal(shouldAttemptSubmit(false, submitConceptPaths(tree)), true);
});
