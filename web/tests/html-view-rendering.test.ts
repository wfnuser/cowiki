import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { createServer } from 'vite';

test('HTML View composes with comment anchors without nesting a preview inside pre', async () => {
  const vite = await createServer({ root: fileURLToPath(new URL('../', import.meta.url)), appType: 'custom', logLevel: 'silent', server: { middlewareMode: true } });
  try {
    const { PageReader } = await vite.ssrLoadModule('/src/components/PageReader.tsx');
    const { commentMarkdownComponents } = await vite.ssrLoadModule('/src/components/PageCommentsLayer.tsx');
    const html = renderToStaticMarkup(React.createElement(PageReader, {
      body: '```html\n<button>Run</button>\n```\n\n```text\nplain code\n```',
      markdownComponents: commentMarkdownComponents,
    }));
    assert.match(html, /title="Sandboxed HTML preview"/);
    assert.doesNotMatch(html, /<pre[^>]*>\s*<section/);
    assert.match(html, /<pre[^>]*data-source-line="5"[^>]*><code/);
  } finally { await vite.close(); }
});
