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


test('decks list what is new and due; a deck studies only its cards', async ({ page }) => {
  await setup(page, '# Doc\n');
  await page.goto(origin);
  await page.getByRole('button', { name: 'Flashcards', exact: true }).click();
  const decks = page.getByRole('list', { name: 'Decks' });
  await expect(decks.locator('.deck-row-main')).toHaveText(['Biology20', 'Japanese30', 'N220', 'N510', 'Unsorted10']);
  await expect(page.getByText('Nothing to review')).toHaveCount(0);
  await page.getByRole('button', { name: 'Collapse Japanese' }).click();
  await expect(decks.locator('.deck-row-main')).toHaveText(['Biology20', 'Japanese30', 'Unsorted10']);

  await page.getByRole('button', { name: /^Japanese/ }).click();
  await expect(page.getByRole('heading', { name: 'Japanese' })).toBeVisible();
  await expect(page.getByText('Languages/Japanese')).toBeVisible();
  await page.getByRole('button', { name: 'Study now' }).click();
  await expect(page.getByText('Japanese · 3 left · New')).toBeVisible();
  for (let index = 0; index < 3; index += 1) {
    await page.keyboard.press('Space');
    await page.keyboard.press('r');
  }
  // Back on the deck: all three were Japanese cards; nothing left today.
  await expect(page.getByText('All done for today.')).toBeVisible();
  await page.getByRole('button', { name: 'All decks' }).click();
  await expect(decks.locator('.deck-row-main')).toHaveText(['Biology20', 'Japanese00', 'Unsorted10']);
  await expect(page.getByRole('button', { name: 'Study all · 3' })).toBeVisible();
});
