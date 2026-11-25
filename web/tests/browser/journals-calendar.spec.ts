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

async function desktop(page: Page, options?: {
  days?: string[];
  now?: Date;
  installClock?: boolean;
  bodyForDay?: (day: string) => Promise<string> | string;
}) {
  const space = { name: 'My Space', slug: 'personal', id: 'personal', visibility: 'private', role: 'owner', localPath: '/Users/me/cowiki/personal' };
  const meta = (slug: string) => ({ slug, path: slug + '.md', title: slug.split('/').pop(), summary: '', branch: 'main', kind: 'page', children: [] });
  const days = options?.days ?? DAYS;
  const now = options?.now ?? new Date(2026, 10, 24, 15, 0);
  if (options?.installClock) await page.clock.install({ time: now });
  else await page.clock.setFixedTime(now);
  await page.route('**/*', (route) => route.request().url().startsWith(origin) ? route.continue() : route.abort());
  await page.exposeFunction('fixtureInvoke', async (cmd: string, args: Record<string, unknown>) => {
    switch (cmd) {
      case 'local_list_spaces': return [space];
      case 'local_list_pages': return days.map((day) => meta(`journal/${day}`));
      case 'local_get_page': {
        const day = String(args.pageSlug).split('/').pop() ?? '';
        const body = options?.bodyForDay
          ? await options.bodyForDay(day)
          : `---\ntype: Journal\ntitle: "${day}"\n---\n\n- A note on ${day}\n`;
        return { ...meta(String(args.pageSlug)), body };
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

test('jumping to an older day stays aligned after tall earlier days finish loading and keeps them mounted', async ({ page }) => {
  await page.setViewportSize({ width: 1200, height: 700 });
  // 15 consecutive days so days 7..14 mount asynchronously when jumping to 2026-11-10.
  const tallDays = Array.from({ length: 15 }, (_, index) => `2026-11-${String(24 - index).padStart(2, '0')}`);
  const paragraphs = (day: string) =>
    Array.from({ length: 14 }, (_, i) => `Paragraph ${i + 1} of ${day} with enough text to expand the journal day height significantly.`).join('\n\n');

  await desktop(page, {
    days: tallDays,
    bodyForDay: async (day) => {
      const idx = tallDays.indexOf(day);
      // Delay newly expanded days ABOVE the target (indices 7..13) longer than the target itself.
      if (idx >= 7 && idx < 14) {
        await new Promise((resolve) => setTimeout(resolve, 60 + (idx - 6) * 45));
      }
      return `---\ntype: Journal\ntitle: "${day}"\n---\n\n${paragraphs(day)}\n`;
    },
  });

  await page.goto(origin);
  await page.getByRole('button', { name: /^Journals\s*\d*$/ }).first().click();
  await expect(page.locator('[data-journal-day="2026-11-24"]')).toContainText('Paragraph 14 of 2026-11-24');
  await expect(page.locator('[data-journal-day="2026-11-10"]')).toHaveCount(0);

  await page.getByRole('button', { name: 'Go to a day' }).click();
  await page.getByRole('dialog', { name: 'Journal calendar' })
    .getByRole('button', { name: new Date(2026, 10, 10).toDateString() }).click();

  // Wait for the slowest earlier day (index 13 = 2026-11-11) and the target to finish rendering.
  await expect(page.locator('[data-journal-day="2026-11-11"]')).toContainText('Paragraph 14 of 2026-11-11');
  const target = page.locator('[data-journal-day="2026-11-10"]');
  await expect(target).toContainText('Paragraph 14 of 2026-11-10');
  await expect(target).toBeInViewport();

  // Jumping back to a recent day must NOT unmount the already-expanded older days.
  await page.getByRole('button', { name: 'Go to a day' }).click();
  await page.getByRole('dialog', { name: 'Journal calendar' })
    .getByRole('button', { name: new Date(2026, 10, 23).toDateString() }).click();
  await expect(page.locator('[data-journal-day="2026-11-23"]')).toBeInViewport();
  await expect(target).toHaveCount(1);
});

test('Journals and Home roll over to the new day across midnight without reloading', async ({ page }) => {
  await page.setViewportSize({ width: 1200, height: 700 });
  await desktop(page, {
    now: new Date(2026, 10, 24, 23, 59, 57),
    installClock: true,
  });

  await page.goto(origin);
  // Home starts on Nov 24.
  await expect(page.locator('[data-journal-day="2026-11-24"]')).toBeVisible();

  // Advance clock across local midnight into Nov 25.
  await page.clock.runFor(6_000);
  await expect(page.locator('[data-journal-day="2026-11-25"]')).toBeVisible();

  // Journals stream also reflects Nov 25 as today.
  await page.getByRole('button', { name: /^Journals\s*\d*$/ }).first().click();
  await expect(page.locator('[data-journal-day="2026-11-25"]')).toBeVisible();
});
