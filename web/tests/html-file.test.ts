import assert from 'node:assert/strict';
import test from 'node:test';
import { JSDOM } from 'jsdom';
import { bundleHtmlFile } from '../src/lib/html-file.ts';

test('local HTML embeds CSS, nested fonts, scripts and images without rewriting the original', async () => {
  const dom = new JSDOM();
  Object.defineProperty(globalThis, 'document', { value: dom.window.document, configurable: true });
  const assets: Record<string,string> = { 'paper/assets/style.css': '@font-face{src:url(fonts/demo.woff2) format("woff2"),url(fonts/fallback.woff)}body{color:red}', 'paper/assets/fonts/demo.woff2': 'font', 'paper/assets/app.js': 'document.body.dataset.ready="yes";', 'paper/assets/figure.png': 'image' };
  const source = '<!doctype html><html><head><link rel="stylesheet" href="assets/style.css"><script defer src="assets/app.js"></script></head><body><h1>Paper</h1><img src="assets/figure.png"></body></html>';
  const result = await bundleHtmlFile(source, 'paper/index.html', async path => {
    assert.ok(path in assets, path); return [...new TextEncoder().encode(assets[path])];
  });
  assert.match(result.html, /body\{color:red\}/);
  assert.match(result.html, /data:font\/woff2;base64,/);
  assert.match(result.html, /data:image\/png;base64,/);
  assert.ok(result.html.indexOf('<h1>Paper') < result.html.indexOf('dataset.ready'));
  assert.deepEqual(result.warnings, []);
  dom.window.close();
});

test('external URLs, path escapes and failed assets never become trusted resource reads', async () => {
  const dom = new JSDOM();
  Object.defineProperty(globalThis, 'document', { value: dom.window.document, configurable: true });
  const seen: string[] = [];
  const result = await bundleHtmlFile('<img src="https://evil.test/track"><script src="../secret.js"></script><img src="assets/missing.png">', 'paper/index.html', async path => {
    seen.push(path); throw Error('missing');
  });
  assert.deepEqual(seen, ['paper/assets/missing.png']);
  assert.equal(result.warnings.length, 3);
  assert.doesNotMatch(result.html, /https:\/\/evil/);
  dom.window.close();
});

test('full HTML documents retain body classes needed by presentation scripts', async () => {
  const dom = new JSDOM();
  Object.defineProperty(globalThis, 'document', { value: dom.window.document, configurable: true });
  const result = await bundleHtmlFile('<!doctype html><html lang="zh-CN"><body class="presentation"><h1>Slide</h1></body></html>', 'slides.html', async () => []);
  assert.match(result.html, /<body class="presentation">/);
  assert.match(result.html, /<html lang="zh-CN">/);
  dom.window.close();
});
