import { expect, test, type Page } from '@playwright/test';
import { createServer, type ViteDevServer } from 'vite';
import { fileURLToPath } from 'node:url';

let server: ViteDevServer;
let origin: string;
test.beforeAll(async () => {
  server = await createServer({ root: fileURLToPath(new URL('../../', import.meta.url)), logLevel: 'silent', server: { host: '127.0.0.1', port: 0 } });
  await server.listen();
  const address = server.httpServer!.address();
  if (!address || typeof address === 'string') throw new Error('Missing test port');
  origin = 'http://127.0.0.1:' + address.port;
});
test.afterAll(async () => { await server.close(); });

// One open task today and enough finished ones to overflow the window.
const done = Array.from({ length: 40 }, (_, index) => `- [x] Finished task ${index + 1} ✅ 2026-11-2${index % 4}`).join('\n');
const TASKS = `---\ntype: Note\ntitle: Plan\n---\n\n# Plan\n\n- [ ] Write the report 📅 2026-11-24\n${done}\n`;

async function desktop(page: Page) {
  const space = { name: 'My Space', slug: 'personal', id: 'personal', visibility: 'private', role: 'owner', localPath: '/Users/me/cowiki/personal' };
  await page.clock.setFixedTime(new Date(2026, 10, 24, 15, 0));
  await page.route('**/*', (route) => route.request().url().startsWith(origin) ? route.continue() : route.abort());
  await page.exposeFunction('fixtureInvoke', async (cmd: string) => {
    switch (cmd) {
      case 'local_list_spaces': return [space];
      case 'local_list_pages': return [{ slug: 'Plan', path: 'Plan.md', title: 'Plan', summary: '', branch: 'main', kind: 'page', children: [] }];
      case 'local_read_space_config': return { features: { modules: { journal: true, tasks: true, flashcards: true }, journal: { format: 'YYYY-MM-DD', folder: 'journal', template: null } } };
      case 'local_list_task_sources': return [{ path: 'Plan.md', content: TASKS }];
      case 'local_list_card_sources': case 'local_recent_pages': case 'local_list_sources': case 'local_list_backlinks': return [];
      case 'cloud_get_status': return { state: 'unlinked', conflicts: [], message: '', pullRequest: null, committed: false, cloudSpaceId: null, cloudBaseUrl: null };
      default: return null;
    }
  });
  await page.addInitScript(() => {
    Object.assign(window, { __TAURI_INTERNALS__: { invoke: (cmd: string, args: unknown) => Reflect.get(window, 'fixtureInvoke')(cmd, args) } });
    localStorage.setItem('cowiki.personal.preferences', JSON.stringify({ autoCheckpoint: false }));
  });
}

test('a list growing past the window does not narrow the column', async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 600 });
  await desktop(page);
  await page.goto(origin);
  await page.getByRole('button', { name: /^Tasks\s*\d*$/ }).first().click();
  const tabs = page.getByRole('tablist').first();
  const width = async () => (await tabs.boundingBox())!.width;

  await expect(page.getByText('Write the report')).toBeVisible();
  const today = await width();
  await page.getByRole('tab', { name: /^Done/ }).click();
  await expect(page.getByText('Finished task 40')).toBeAttached();
  expect(await width()).toBe(today);
});

test('a task checkbox sits centred on the first line of its text', async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 600 });
  await desktop(page);
  await page.goto(origin);
  await page.getByRole('button', { name: /^Tasks\s*\d*$/ }).first().click();
  const row = page.locator('li:has(input[type=checkbox])', { hasText: 'Write the report' });
  await expect(row).toBeVisible();
  const offset = await row.evaluate((li) => {
    const box = li.querySelector('input')!.getBoundingClientRect();
    const text = li.querySelector('span')!;
    const line = parseFloat(getComputedStyle(text).lineHeight);
    return (box.top + box.height / 2) - (text.getBoundingClientRect().top + line / 2);
  });
  expect(Math.abs(offset)).toBeLessThanOrEqual(0.5);
});
