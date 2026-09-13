import { createServer, type Server } from 'node:http';
import { test, expect, type Page } from '@playwright/test';
import { sandboxedHtmlDocument } from '../../src/lib/html-view';

let server: Server;
let origin: string;
let requests: string[];

test.beforeAll(async () => {
  server = createServer((req, res) => {
    if (req.url !== '/') requests.push(req.url ?? '');
    res.setHeader('Content-Type', 'text/html');
    res.end('<!doctype html><title>CoWiki host</title><body>Host page</body>');
  });
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('Missing test server port');
  origin = `http://127.0.0.1:${address.port}`;
});
test.afterAll(async () => { await new Promise<void>((resolve) => server.close(() => resolve())); });
test.beforeEach(async ({ page }) => {
  requests = [];
  await page.goto(origin);
  await page.evaluate(() => { localStorage.setItem('credential', 'test-secret'); });
});

async function mount(page: Page, source: string) {
  await page.evaluate((srcdoc) => {
    const frame = document.createElement('iframe');
    frame.sandbox.add('allow-scripts');
    frame.title = 'Sandboxed HTML preview';
    frame.srcdoc = srcdoc;
    document.body.append(frame);
  }, sandboxedHtmlDocument(source));
}

test('self-contained interaction works without access to host credentials or DOM', async ({ page }) => {
  await mount(page, `<button onclick="this.textContent='Done'">Run</button><output id="boundary"></output>
    <script>
      let blocked = 0;
      try { parent.document.body.textContent = 'compromised'; } catch { blocked++; }
      try { localStorage.getItem('credential'); } catch { blocked++; }
      document.querySelector('#boundary').textContent = String(blocked);
    </script>`);
  await expect.poll(() => page.frames().filter((frame) => frame.url() === 'about:srcdoc').length).toBeGreaterThan(0);
  await expect.poll(async () => {
    for (const frame of page.frames()) if (await frame.locator('#boundary').count()) return frame.locator('#boundary').textContent();
    return '';
  }).toBe('2');
  const content = page.frames().find((frame) => frame !== page.mainFrame() && frame.childFrames().length === 0)!;
  await content.getByRole('button', { name: 'Run' }).click();
  await expect(content.getByRole('button', { name: 'Done' })).toBeVisible();
  expect(await page.evaluate(() => localStorage.getItem('credential'))).toBe('test-secret');
  await expect(page).toHaveTitle('CoWiki host');
});

for (const [name, source] of [
  ['forged head inside a comment', (url: string) => `<!-- <html><head> --><html><head></head><body><img src="${url}/image"><script>fetch('${url}/fetch').catch(()=>{});</script></body></html>`],
  ['script navigation', (url: string) => `<script>location.replace('${url}/navigation')</script>`],
  ['meta refresh', (url: string) => `<meta http-equiv="refresh" content="0;url=${url}/refresh">`],
  ['subresources and nested frames', (url: string) => `<img src="${url}/image"><iframe src="${url}/nested"></iframe><script src="${url}/script"></script><script>fetch('${url}/fetch').catch(()=>{}); navigator.sendBeacon('${url}/beacon', 'test');</script>`],
] as const) {
  test(`blocks outbound traffic from ${name}`, async ({ page }) => {
    await mount(page, source(origin));
    // Observe delayed refresh and fetch work too, not merely initial markup.
    await page.waitForTimeout(500);
    expect(requests.filter((path) => path !== '/favicon.ico')).toEqual([]);
    await expect(page).toHaveTitle('CoWiki host');
  });
}
