import assert from 'node:assert/strict';
import test from 'node:test';

import { restoreSystemFrontmatter, splitSystemFrontmatter } from '../src/lib/page-frontmatter.ts';

test('system frontmatter is hidden from the editable document and restored exactly', () => {
  const original = '---\ntitle: "Test"\nsummary: ""\nkind: concept\n---\n\nEditable text\n- [ ] Task';
  const page = splitSystemFrontmatter(original);

  assert.equal(page.body, 'Editable text\n- [ ] Task');
  assert.equal(restoreSystemFrontmatter(page.systemFrontmatter, page.body), original);
});

test('plain Markdown remains fully editable', () => {
  assert.deepEqual(splitSystemFrontmatter('# Hello'), { systemFrontmatter: '', body: '# Hello' });
});
