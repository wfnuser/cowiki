import assert from 'node:assert/strict';
import test from 'node:test';

import {
  restoreSystemFrontmatter,
  sourceUrlFromDocument,
  splitSystemFrontmatter,
} from '../src/lib/page-frontmatter.ts';

test('system frontmatter is hidden from the editable document and restored exactly', () => {
  const original = '---\ntype: Note\ntitle: "Test"\nsummary: ""\n---\n\nEditable text\n- [ ] Task';
  const page = splitSystemFrontmatter(original);

  assert.equal(page.body, 'Editable text\n- [ ] Task');
  assert.equal(restoreSystemFrontmatter(page.systemFrontmatter, page.body), original);
});

test('plain Markdown remains fully editable', () => {
  assert.deepEqual(splitSystemFrontmatter('# Hello'), { systemFrontmatter: '', body: '# Hello' });
});

test('read-only Source rendering hides OKF frontmatter', () => {
  const source = splitSystemFrontmatter(
    '---\ntitle: "https://example.com"\ntype: Source\n---\n\nhttps://example.com',
  );

  assert.equal(source.body, 'https://example.com');
});

test('a captured web Source exposes its original URL without leaking other metadata', () => {
  const source = [
    '---',
    'title: "Example: \\"quoted\\""',
    'type: Source',
    'source_url: "https://example.com/article?q=local%20first"',
    'captured_at: "2026-09-04T12:00:00Z"',
    '---',
    '',
    '# Article',
  ].join('\n');

  assert.equal(sourceUrlFromDocument(source), 'https://example.com/article?q=local%20first');
  assert.equal(sourceUrlFromDocument('---\ntype: Source\n---\n\nPlain source'), null);
  assert.equal(
    sourceUrlFromDocument('---\nsource_url: "javascript:alert(1)"\n---\n'),
    null,
  );
});
