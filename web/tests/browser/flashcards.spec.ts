import { test, expect, type Page } from '@playwright/test';
import { createServer, type ViteDevServer } from 'vite';
import { fileURLToPath } from 'node:url';

// Flashcards are sorted into decks, as in Anki: the card's tags, else the
// page's tags, else its folder (lib/flashcards.ts).
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

async function setup(page: Page, body: string, more: Array<[string, string]> = []) {
  const space = { name: 'Personal', slug: 'personal', id: 'personal', visibility: 'private', role: 'owner', localPath: '/fixture/personal' };
  const files = new Map([['Doc', body], ...more]);
  let reviews: unknown = null;
  const meta = (slug: string) => ({ slug, path: slug + '.md', title: slug, summary: '', branch: 'main', kind: 'page', children: [] });
  await page.route('**/*', (route) => route.request().url().startsWith(origin) ? route.continue() : route.abort());
  await page.exposeFunction('fixtureInvoke', async (cmd: string, args: { pageSlug: string; content: string }) => {
    switch (cmd) {
      case 'local_list_spaces': return [space];
      case 'local_list_pages': return Array.from(files.keys(), meta);
      case 'local_get_page': return { ...meta(args.pageSlug), body: files.get(args.pageSlug), edited_by: 'wfnuser', edited_at: 1700000000 };
      case 'local_write_page': files.set(args.pageSlug, args.content); return;
      case 'local_read_space_config': return { features: { modules: { journal: true }, journal: { format: 'YYYY-MM-DD', folder: 'Daily', template: null } } };
      case 'local_list_page_comments': return { comments: [], snapshots: [] };
      case 'local_list_card_sources': return [
        { path: 'Languages/Japanese.md', content: '- 食べる #card #Japanese/N2\n  - to eat\n- 飲む #card #Japanese/N2\n  - to drink\n- こんにちは #card #Japanese/N5\n  - hello\n' },
        { path: 'Biology/Cells.md', content: '---\ntags: [Biology]\n---\n- What is a ribosome? #card\n  - It makes proteins\n- What does the {{cloze mitochondria}} do? #card\n' },
        { path: 'Inbox.md', content: '- Year Rome fell? #card\n  - 476\n' },
      ];
      case 'local_read_card_reviews': return reviews;
      case 'local_write_card_reviews': reviews = (args as unknown as { state: unknown }).state; return null;
      case 'local_working_diff': case 'local_list_agent_changes': case 'local_list_sources': case 'local_list_backlinks': case 'local_recent_pages': case 'local_list_task_sources': return [];
      case 'cloud_get_status': return { state: 'unlinked', conflicts: [], message: '', pullRequest: null, committed: false, cloudSpaceId: null, cloudBaseUrl: null };
      case 'local_set_auto_checkpoint': case 'local_auto_checkpoint': return null;
      default: throw new Error('Unsupported fixture IPC: ' + cmd);
    }
  });
  await page.addInitScript(() => {
    Object.assign(window, { __TAURI_INTERNALS__: { invoke: (cmd: string, args: unknown) => Reflect.get(window, 'fixtureInvoke')(cmd, args) } });
    localStorage.setItem('cowiki.personal.preferences', JSON.stringify({ autoCheckpoint: false }));
  });
}


test('one line says what is due today; a deck studies only its cards', async ({ page }) => {
  await setup(page, '# Doc\n');
  await page.goto(origin);
  await page.getByRole('button', { name: 'Flashcards', exact: true }).click();
  const decks = page.getByRole('list', { name: 'Decks' });
  await expect(page.getByText('6 cards to review today.')).toBeVisible();
  await expect(decks.locator('.deck-open')).toHaveText(['Biology2 to review', 'Japanese3 to review', 'N22 to review', 'N51 to review', 'Unsorted1 to review']);
  await page.getByRole('button', { name: 'Collapse Japanese' }).click();
  await expect(decks.locator('.deck-open')).toHaveText(['Biology2 to review', 'Japanese3 to review', 'Unsorted1 to review']);

  // The whole row opens a deck; its own line starts only its cards.
  await page.getByRole('button', { name: /^Japanese/ }).click();
  await expect(page.getByRole('heading', { name: 'Japanese' })).toBeVisible();
  await expect(page.getByText('Languages/Japanese')).toBeVisible();
  await expect(page.getByText('3 cards to review today.')).toBeVisible();
  await page.getByRole('button', { name: 'Start', exact: true }).click();
  await expect(page.getByText('3 left')).toBeVisible();
  await expect(page.getByText('Japanese', { exact: true })).toBeVisible();
  await expect(page.getByRole('progressbar', { name: 'Session progress' })).toHaveAttribute('aria-valuenow', '0');
  for (let index = 0; index < 3; index += 1) {
    await page.keyboard.press('Space');
    await page.keyboard.press('r');
  }
  // Back on the deck: all three were Japanese cards; nothing left today.
  await expect(page.getByText('All done for today.')).toBeVisible();
  await page.getByRole('button', { name: 'All decks' }).click();
  await expect(decks.locator('.deck-open')).toHaveText(['Biology2 to review', 'JapaneseDone', 'Unsorted1 to review']);
  await expect(page.getByText('3 cards to review today.')).toBeVisible();

  // Hovering a deck swaps its count for Start in the same spot.
  const biology = decks.locator('.deck-line').first();
  const count = biology.locator('.deck-due');
  const before = await count.boundingBox();
  await biology.hover();
  await expect(count).toBeHidden();
  const start = page.getByRole('button', { name: 'Study Biology' });
  await expect(start).toBeVisible();
  const after = await start.boundingBox();
  expect(Math.abs((before!.x + before!.width) - (after!.x + after!.width))).toBeLessThan(10);
  await start.click();
  await expect(page.getByText('2 left')).toBeVisible();
  await expect(page.getByText('Biology', { exact: true })).toBeVisible();
});

test('studying uses one accent at most: plain text answers, no coloured dots', async ({ page }) => {
  await setup(page, '# Doc\n');
  await page.goto(origin);
  await page.getByRole('button', { name: 'Flashcards', exact: true }).click();
  await page.getByRole('button', { name: 'Start', exact: true }).click();
  await page.keyboard.press('Space');
  const ratings = page.locator('.study-ratings button');
  await expect(ratings).toHaveCount(3);
  await expect(ratings.nth(0)).toContainText('Forgotten');
  await expect(ratings.nth(2)).toContainText('Remembered');
  // No red/amber/green/blue anywhere in the study column.
  const colours = await page.locator('section').first().evaluate((root) => {
    const seen = new Set<string>();
    for (const el of root.querySelectorAll('*')) {
      const style = getComputedStyle(el);
      for (const value of [style.color, style.backgroundColor, style.borderTopColor]) seen.add(value);
    }
    return [...seen];
  });
  const vivid = colours.filter((value) => {
    const match = /rgba?\((\d+), (\d+), (\d+)(?:, ([\d.]+))?\)/.exec(value);
    if (!match || (match[4] !== undefined && Number(match[4]) === 0)) return false;
    const [r, g, b] = match.slice(1, 4).map(Number);
    return Math.max(r, g, b) - Math.min(r, g, b) > 60;
  });
  expect(vivid).toEqual([]);
});
