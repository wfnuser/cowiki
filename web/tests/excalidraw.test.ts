import assert from 'node:assert/strict';
import test from 'node:test';
import LZString from 'lz-string';

import { isExcalidrawPage, parseExcalidrawPage, sceneBounds } from '../src/lib/excalidraw.ts';

const scene = {
  type: 'excalidraw',
  version: 2,
  elements: [
    { id: 'r', type: 'rectangle', x: 10, y: 20, width: 100, height: 50, seed: 1 },
    { id: 'gone', type: 'ellipse', x: 900, y: 900, width: 10, height: 10, isDeleted: true },
    { id: 'a', type: 'arrow', x: 110, y: 45, width: 90, height: 30, points: [[0, 0], [90, 30]], endArrowhead: 'arrow' },
    { id: 'i', type: 'image', x: 0, y: 100, width: 40, height: 40, fileId: 'abc123' },
  ],
  appState: { viewBackgroundColor: '#fffce8', theme: 'light' },
};

function page(data: string, block = 'compressed-json'): string {
  return [
    '---',
    'excalidraw-plugin: parsed',
    'tags: [excalidraw]',
    '---',
    '==⚠  Switch to EXCALIDRAW VIEW in the MORE OPTIONS menu of this document. ⚠== You can decompress Drawing data with the command palette.',
    '',
    'My notes about [[Ownership]].',
    '',
    '# Excalidraw Data',
    '## Text Elements',
    'Hello ^t1',
    '',
    '## Embedded Files',
    'abc123: [[Pasted image 20260731.png]]',
    '',
    '%%',
    '## Drawing',
    '```' + block,
    data,
    '```',
    '%%',
  ].join('\n');
}

test('Excalidraw pages are recognised by name or frontmatter', () => {
  assert.equal(isExcalidrawPage('Excalidraw/Drawing 1.excalidraw.md', '# x'), true);
  assert.equal(isExcalidrawPage('Board.md', '---\nexcalidraw-plugin: parsed\n---\n'), true);
  assert.equal(isExcalidrawPage('Note.md', '---\ntags: [excalidraw]\n---\n'), false);
});

test('compressed drawings are decoded, deleted elements dropped and images linked', () => {
  const compressed = LZString.compressToBase64(JSON.stringify(scene)).replace(/(.{64})/g, '$1\n');
  const parsed = parseExcalidrawPage(page(compressed));
  assert.equal(parsed.error, null);
  assert.equal(parsed.notes, 'My notes about [[Ownership]].');
  assert.deepEqual(parsed.scene?.elements.map((element) => element.id), ['r', 'a', 'i']);
  assert.equal(parsed.scene?.background, '#fffce8');
  assert.deepEqual(parsed.scene?.files, { abc123: { link: 'Pasted image 20260731.png' } });
  assert.deepEqual(sceneBounds(parsed.scene!.elements, 0), { x: 0, y: 20, width: 200, height: 120 });
});

test('plain JSON drawings, empty drawings and broken data', () => {
  assert.equal(parseExcalidrawPage(page(JSON.stringify(scene), 'json')).scene?.elements.length, 3);
  const empty = parseExcalidrawPage(page(LZString.compressToBase64('{"type":"excalidraw","elements":[],"appState":{}}')));
  assert.deepEqual(empty.scene?.elements, []);
  assert.equal(empty.scene?.background, '#ffffff');
  assert.match(parseExcalidrawPage(page('not base64 at all!!')).error ?? '', /decompress|JSON/);
  assert.equal(parseExcalidrawPage('---\nexcalidraw-plugin: parsed\n---\nJust text').error, 'This page has no Excalidraw drawing data.');
});
