import assert from 'node:assert/strict';
import test from 'node:test';

import { flattenPages, isImagePath, parseWikilink, resolveWikilinkPath } from '../src/lib/wikilinks.ts';

const pages = [
  { path: 'index.md', title: 'Knowledge' },
  { path: 'Areas/AI/Transformer.md', title: 'Transformer' },
  { path: 'Projects/CoWiki/README.md', title: 'CoWiki' },
  { path: 'Projects/AKV/README.md', title: 'AKV' },
  { path: 'Resources/Paper - Attention.md', title: 'Attention Is All You Need' },
  { path: 'Café.md', title: 'Café' },
];

test('parses alias, heading and block parts', () => {
  assert.deepEqual(parseWikilink('[[Note#Section|shown]]'), { page: 'Note', heading: 'Section', block: null, alias: 'shown' });
  assert.deepEqual(parseWikilink('![[Note#^abc123]]'), { page: 'Note', heading: null, block: 'abc123', alias: null });
  assert.equal(parseWikilink('[[#Local heading]]').page, '');
  assert.equal(parseWikilink('folder/Note.md').page, 'folder/Note');
});

test('resolves by basename anywhere in the vault, case-insensitively', () => {
  assert.equal(resolveWikilinkPath('Transformer', pages), 'Areas/AI/Transformer.md');
  assert.equal(resolveWikilinkPath('transformer#Attention|T', pages), 'Areas/AI/Transformer.md');
  assert.equal(resolveWikilinkPath('Paper - Attention', pages), 'Resources/Paper - Attention.md');
  assert.equal(resolveWikilinkPath('Café', pages), 'Café.md', 'NFD and NFC names match');
});

test('ambiguous names prefer the linking folder, then the shortest path', () => {
  assert.equal(resolveWikilinkPath('README', pages, 'Projects/AKV/notes.md'), 'Projects/AKV/README.md');
  assert.equal(resolveWikilinkPath('README', pages, 'Projects/CoWiki/x.md'), 'Projects/CoWiki/README.md');
  assert.equal(resolveWikilinkPath('README', pages), 'Projects/AKV/README.md');
});

test('paths resolve exactly, relatively, or by suffix', () => {
  assert.equal(resolveWikilinkPath('Projects/CoWiki/README', pages), 'Projects/CoWiki/README.md');
  assert.equal(resolveWikilinkPath('../AKV/README', pages, 'Projects/CoWiki/x.md'), 'Projects/AKV/README.md');
  assert.equal(resolveWikilinkPath('CoWiki/README', pages), 'Projects/CoWiki/README.md');
});

test('falls back to titles and reports missing pages', () => {
  assert.equal(resolveWikilinkPath('Attention Is All You Need', pages), 'Resources/Paper - Attention.md');
  assert.equal(resolveWikilinkPath('Nonexistent', pages), null);
  assert.equal(resolveWikilinkPath('#heading', pages), null);
});

test('flattens folders and recognises images', () => {
  const tree = [
    { path: 'a', title: 'a', kind: 'folder', children: [{ path: 'a/b.md', title: 'B', kind: 'page' }] },
    { path: 'c.md', title: 'C', kind: 'page' },
  ];
  assert.deepEqual(flattenPages(tree), [{ path: 'a/b.md', title: 'B' }, { path: 'c.md', title: 'C' }]);
  assert.equal(isImagePath('assets/diagram.PNG'), true);
  assert.equal(isImagePath('paper.pdf'), false);
});
