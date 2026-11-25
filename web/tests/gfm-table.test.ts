import assert from 'node:assert/strict';
import test from 'node:test';

import { parseGfmTable, tableCellOffset, tokenizeCell } from '../src/components/editor/gfmTable.ts';

test('parseGfmTable parses headers, alignments, body rows and escaped pipes', () => {
  const source = [
    '| Name | Count | Notes |',
    '| :--- | :---: | ---: |',
    '| Alpha | 1 | `a\\|b` |',
    '| Beta | 2 | **bold** |',
  ].join('\n');

  const parsed = parseGfmTable(source);
  assert.deepEqual(parsed, {
    headers: ['Name', 'Count', 'Notes'],
    alignments: ['left', 'center', 'right'],
    rows: [
      ['Alpha', '1', '`a|b`'],
      ['Beta', '2', '**bold**'],
    ],
  });
});

test('parseGfmTable rejects malformed delimiter rows', () => {
  assert.equal(parseGfmTable('| A | B |\n| not a rule |\n| 1 | 2 |'), null);
  assert.equal(parseGfmTable('Only one line'), null);
});

test('tableCellOffset maps header and body cells to the matching source offset', () => {
  const source = '| Col A | Col B |\n| --- | --- |\n| First | Second |';
  const headB = tableCellOffset(source, 'head', 0, 1);
  assert.equal(source.slice(headB, headB + 5), 'Col B');

  const bodyA = tableCellOffset(source, 'body', 0, 0);
  assert.equal(source.slice(bodyA, bodyA + 5), 'First');

  const bodyB = tableCellOffset(source, 'body', 0, 1);
  assert.equal(source.slice(bodyB, bodyB + 6), 'Second');
});

test('tokenizeCell handles inline code, emphasis, links and wikilinks', () => {
  const tokens = tokenizeCell('See `code`, **bold**, [[Target|Label]] and [web](https://example.com)');
  assert.deepEqual(tokens, [
    { kind: 'text', text: 'See ' },
    { kind: 'code', text: 'code' },
    { kind: 'text', text: ', ' },
    { kind: 'strong', children: [{ kind: 'text', text: 'bold' }] },
    { kind: 'text', text: ', ' },
    { kind: 'wikilink', target: 'Target', label: 'Label' },
    { kind: 'text', text: ' and ' },
    { kind: 'link', label: 'web', href: 'https://example.com' },
  ]);
});
