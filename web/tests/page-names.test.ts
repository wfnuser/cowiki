import assert from 'node:assert/strict';
import test from 'node:test';

import { pageFileName } from '../src/lib/page-names.ts';

test('page names stay readable and file-system safe', () => {
  assert.equal(pageFileName('周记'), '周记');
  assert.equal(pageFileName('  Attention   Is All You Need '), 'Attention Is All You Need');
  assert.equal(pageFileName('Q3: plan / review?'), 'Q3- plan - review-');
  assert.equal(pageFileName('[[Note#1]]'), '--Note-1--');
  assert.equal(pageFileName('.hidden.'), 'hidden');
  assert.equal(pageFileName('   '), '');
});
