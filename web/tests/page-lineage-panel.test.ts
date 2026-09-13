import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import React, { act } from 'react';
import { createServer, type ViteDevServer } from 'vite';
import { JSDOM } from 'jsdom';

let vite: ViteDevServer;
test.before(async () => { vite = await createServer({ root: fileURLToPath(new URL('../', import.meta.url)), appType: 'custom', logLevel: 'silent', server: { middlewareMode: true } }); });
test.after(async () => { await vite.close(); });
const lineage = {
  sources: ['.cowiki/sources/interview.md'],
  agents: [{ name: 'Codex', changeId: 'change-1', task: 'Organize interview' }],
  commit: { oid: '0123456789abcdef', summary: 'Compile interview', author: 'Qinghao', committedAt: 1725000000 },
  review: { id: 'review-id', number: 12, title: 'Compile interview' },
};
async function mount(props: Record<string, unknown> = {}) {
  const dom = new JSDOM('<!doctype html><div id="root"></div>', { url: 'http://localhost' });
  Object.assign(globalThis, { window: dom.window, document: dom.window.document, HTMLElement: dom.window.HTMLElement, Node: dom.window.Node, IS_REACT_ACT_ENVIRONMENT: true });
  const { createRoot } = await import('react-dom/client');
  const { PageReader } = await vite.ssrLoadModule('/src/components/PageReader.tsx');
  const host = document.getElementById('root')!;
  const root = createRoot(host);
  await act(async () => root.render(React.createElement(PageReader, { body: '# Durable knowledge', lineage, ...props })));
  const click = async (name: string) => {
    const button = [...host.querySelectorAll('button')].find(b => b.textContent?.includes(name) || b.getAttribute('aria-label') === name);
    assert.ok(button, 'Missing action: ' + name);
    await act(async () => button.click());
  };
  return { host, click, cleanup: async () => { await act(async () => root.unmount()); dom.window.close(); } };
}
test('lineage opens beside the article and closes back to reading', async () => {
  const opened: string[] = [];
  const ui = await mount({ onOpenReview: (id: string) => opened.push(id) });
  try {
    assert.equal(ui.host.querySelector('aside'), null);
    assert.doesNotMatch(ui.host.textContent!, /0123456789abcdef|change-1|review-id/);
    await ui.click('Sources & records');
    const panel = ui.host.querySelector('aside[aria-label="Sources & records"]');
    assert.ok(panel, 'side panel appears on demand');
    assert.ok(!ui.host.querySelector('article')!.contains(panel));
    await ui.click('Records');
    assert.match(panel.textContent!, /Codex/);
    assert.match(panel.textContent!, /Organize interview/);
    await ui.click('Review: Compile interview');
    assert.deepEqual(opened, ['review-id']);
    await ui.click('Close sources and records');
    assert.equal(ui.host.querySelector('aside'), null);
    assert.equal(document.activeElement?.getAttribute('aria-expanded'), 'false');
  } finally { await ui.cleanup(); }
});
test('sources load on demand and show readable titles with retryable errors', async () => {
  let calls = 0;
  const ui = await mount({ loadSource: async (path: string) => {
    assert.equal(path, '.cowiki/sources/interview.md');
    calls++;
    if (calls === 1) throw new Error('Source unavailable');
    return { title: 'Interview with the team', content: '---\nsource_url: https://example.com/interview\ncaptured_at: 2026-09-12\n---\n# Interview\n\nEvidence from the team.' };
  } });
  try {
    assert.equal(calls, 0);
    await ui.click('Sources & records');
    assert.match(ui.host.textContent!, /Source unavailable/);
    await ui.click('Retry');
    assert.match(ui.host.textContent!, /Interview with the team/);
    assert.match(ui.host.textContent!, /example.com/);
    assert.equal(ui.host.querySelector('a[target="_blank"]')?.getAttribute('href'), 'https://example.com/interview');
  } finally { await ui.cleanup(); }
});
test('empty provenance never fabricates history', async () => {
  const ui = await mount({ lineage: { sources: [], agents: [], commit: null, review: null } });
  try {
    await ui.click('Sources & records');
    assert.match(ui.host.textContent!, /No sources linked/);
    await ui.click('Records');
    assert.match(ui.host.textContent!, /No recorded changes/);
    assert.doesNotMatch(ui.host.textContent!, /Created|Approved|Qinghao|Codex/);
  } finally { await ui.cleanup(); }
});
