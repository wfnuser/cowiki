import assert from 'node:assert/strict';
import test from 'node:test';

import { isHtmlCodeLanguage } from '../src/lib/html-view.ts';

test('HTML code fences are the only Markdown blocks promoted to HTML View', () => {
  assert.equal(isHtmlCodeLanguage('language-html'), true);
  assert.equal(isHtmlCodeLanguage('language-HTML'), true);
  assert.equal(isHtmlCodeLanguage('language-javascript'), false);
  assert.equal(isHtmlCodeLanguage(undefined), false);
});

// Security and actual script interaction run in Chromium/WebKit in browser/html-sandbox.spec.ts.
