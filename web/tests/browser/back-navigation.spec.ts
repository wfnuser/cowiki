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

const files = new Map<string, string>([
  ['Welcome', '---\ntype: Note\ntitle: Welcome\n---\n\n# Welcome\n\nSee [[Rust ownership]].\n'],
  ['Rust ownership', '---\ntype: Note\ntitle: Rust ownership\n---\n\n# Rust ownership\n\nMoves and borrows.\n'],
]);

async function desktop(page: Page) {
  const space = { name: 'My Space', slug: 'personal', id: 'personal', visibility: 'private', role: 'owner', localPath: '/Users/me/cowiki/personal' };
  const meta = (slug: string) => ({ slug, path: slug + '.md', title: slug, summary: '', branch: 'main', kind: 'page', children: [] });
  await page.route('**/*', (route) => route.request().url().startsWith(origin) ? route.continue() : route.abort());
  await page.exposeFunction('fixtureInvoke', async (cmd: string, args: Record<string, unknown>) => {
    switch (cmd) {
      case 'local_list_spaces': return [space];
      case 'local_list_pages': return Array.from(files.keys(), meta);
      case 'local_get_page': return { ...meta(String(args.pageSlug)), body: files.get(String(args.pageSlug)) ?? '' };
      case 'local_read_space_config': return { features: { modules: { journal: true, tasks: true, flashcards: true }, journal: { format: 'YYYY-MM-DD', folder: 'journal', template: null } } };
      case 'local_list_page_comments': return { comments: [], snapshots: [] };
      case 'local_list_task_sources': case 'local_list_card_sources': case 'local_recent_pages': return [];
      case 'local_list_sources': case 'local_list_backlinks': return [];
      case 'cloud_get_status': return { state: 'unlinked', conflicts: [], message: '', pullRequest: null, committed: false, cloudSpaceId: null, cloudBaseUrl: null };
      default: return null;
    }
  });
  await page.addInitScript(() => {
    Object.assign(window, { __TAURI_INTERNALS__: { invoke: (cmd: string, args: unknown) => Reflect.get(window, 'fixtureInvoke')(cmd, args) } });
    localStorage.setItem('cowiki.personal.preferences', JSON.stringify({ autoCheckpoint: false }));
  });
}

const mod = process.platform === 'darwin' ? 'Meta' : 'Control';

test('back and forward step through pages, views and shortcuts', async ({ page }) => {
  await desktop(page);
  await page.goto(origin);
  const crumb = page.locator('.app-breadcrumb');
  const back = page.getByRole('button', { name: 'Back', exact: true });
  const forward = page.getByRole('button', { name: 'Forward', exact: true });

  await expect(back).toBeDisabled();
  await expect(forward).toBeDisabled();

  await page.getByText('Welcome', { exact: true }).first().click();
  await expect(crumb).toContainText('Welcome');
  await page.locator('.prose').getByText('Rust ownership').click();
  await expect(crumb).toContainText('Rust ownership');
  await page.getByRole('button', { name: /^Tasks\s*\d*$/ }).first().click();
  await expect(crumb).toContainText('Tasks');

  await back.click();
  await expect(crumb).toContainText('Rust ownership');
  await back.click();
  await expect(crumb).toContainText('Welcome');
  await expect(forward).toBeEnabled();
  await forward.click();
  await expect(crumb).toContainText('Rust ownership');

  // ⌘[ outside text, ⌘⌥→ anywhere.
  await page.locator('body').click({ position: { x: 5, y: 400 } });
  await page.keyboard.press(`${mod}+BracketLeft`);
  await expect(crumb).toContainText('Welcome');
  await page.keyboard.press(`${mod}+Alt+ArrowRight`);
  await expect(crumb).toContainText('Rust ownership');

  // Opening something new after going back drops the steps ahead.
  await back.click();
  await expect(crumb).toContainText('Welcome');
  await page.getByRole('button', { name: /^Journals\s*\d*$/ }).first().click();
  await expect(crumb).toContainText('Journals');
  await expect(forward).toBeDisabled();
  await back.click();
  await expect(crumb).toContainText('Welcome');
  await back.click();
  await expect(crumb).not.toContainText('Welcome');
  await expect(back).toBeDisabled();
});
