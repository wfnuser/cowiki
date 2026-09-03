import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import { isHtmlCodeLanguage, sandboxedHtmlDocument } from '../src/lib/html-view.ts';

test('HTML code fences are the only Markdown blocks promoted to HTML View', () => {
  assert.equal(isHtmlCodeLanguage('language-html'), true);
  assert.equal(isHtmlCodeLanguage('language-HTML'), true);
  assert.equal(isHtmlCodeLanguage('language-javascript'), false);
  assert.equal(isHtmlCodeLanguage(undefined), false);
});

test('HTML View injects a restrictive policy while preserving self-contained interaction', () => {
  const document = sandboxedHtmlDocument('<button onclick="this.textContent=\'Done\'">Run</button>');
  assert.match(document, /default-src 'none'/);
  assert.match(document, /script-src 'unsafe-inline'/);
  assert.match(document, /connect-src 'none'/);
  assert.match(document, /<button onclick=/);
});

test('the iframe allows scripts without granting same-origin access', () => {
  const source = readFileSync(new URL('../src/components/HtmlView.tsx', import.meta.url), 'utf8');
  assert.match(source, /sandbox="allow-scripts"/);
  assert.doesNotMatch(source, /allow-same-origin/);
  assert.doesNotMatch(source, /dangerouslySetInnerHTML/);
});
