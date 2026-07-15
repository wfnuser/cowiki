import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const mainLayout = readFileSync(new URL('../src/pages/MainLayout.tsx', import.meta.url), 'utf8');

test('the page header has no unused overflow actions menu', () => {
  assert.equal(mainLayout.includes('aria-label="More actions"'), false);
});
