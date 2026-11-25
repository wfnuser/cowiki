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

// A note every other day of November up to the 24th: more than one page of days.
const DAYS = Array.from({ length: 12 }, (_, index) => `2026-11-${String(24 - index * 2).padStart(2, '0')}`);

async function desktop(page: Page) {
  const space = { name: 'My Space', slug: 'personal', id: 'personal', visibility: 'private', role: 'owner', localPath: '/Users/me/cowiki/personal' };
  const meta = (slug: string) => ({ slug, path: slug + '.md', title: slug.split('/').pop(), summary: '', branch: 'main', kind: 'page', children: [] });
  await page.clock.setFixedTime(new Date(2026, 10, 24, 15, 0));
  await page.route('**/*', (route) => route.request().url().startsWith(origin) ? route.continue() : route.abort());
  await page.exposeFunction('fixtureInvoke', async (cmd: string, args: Record<string, unknown>) => {
    switch (cmd) {
      case 'local_list_spaces': return [space];
      case 'local_list_pages': return DAYS.map((day) => meta(`journal/${day}`));
      case 'local_get_page': {
        const day = String(args.pageSlug).split('/').pop();
        return { ...meta(String(args.pageSlug)), body: `---\ntype: Journal\ntitle: "${day}"\n---\n\n- A note on ${day}\n` };
      }
      case 'local_read_space_config': return { features: { modules: { journal: true, tasks: true, flashcards: true }, journal: { format: 'YYYY-MM-DD', folder: 'journal', template: null } } };
      case 'local_list_page_comments': return { comments: [], snapshots: [] };
      case 'local_list_task_sources': case 'local_list_card_sources': case 'local_recent_pages': case 'local_list_sources': case 'local_list_backlinks': return [];
      case 'cloud_get_status': return { state: 'unlinked', conflicts: [], message: '', pullRequest: null, committed: false, cloudSpaceId: null, cloudBaseUrl: null };
      default: return null;
    }
  });
  await page.addInitScript(() => {
    Object.assign(window, { __TAURI_INTERNALS__: { invoke: (cmd: string, args: unknown) => Reflect.get(window, 'fixtureInvoke')(cmd, args) } });
    localStorage.setItem('cowiki.personal.preferences', JSON.stringify({ autoCheckpoint: false }));
  });
}

test('the Journals calendar goes to a day further back than what has loaded', async ({ page }) => {
  await page.setViewportSize({ width: 1200, height: 700 });
  await desktop(page);
  await page.goto(origin);
  await page.getByRole('button', { name: /^Journals\s*\d*$/ }).first().click();
  await expect(page.locator('[data-journal-day="2026-11-24"]')).toBeVisible();
  // Only the first page of days has loaded.
  await expect(page.locator('[data-journal-day="2026-11-04"]')).toHaveCount(0);

  await page.getByRole('button', { name: 'Go to a day' }).click();
  const calendar = page.getByRole('dialog', { name: 'Journal calendar' });
  // Days without a note cannot be picked; days with one can.
  await expect(calendar.getByRole('button', { name: new Date(2026, 10, 5).toDateString() })).toBeDisabled();
  await calendar.getByRole('button', { name: new Date(2026, 10, 4).toDateString() }).click();

  const target = page.locator('[data-journal-day="2026-11-04"]');
  await expect(target).toBeInViewport();
  await expect(calendar).toHaveCount(0);
});
