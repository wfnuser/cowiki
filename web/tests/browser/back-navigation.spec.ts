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

test('slow page responses never overwrite a newer navigation and Reviews waits for edit flush', async ({ page }) => {
  const space = { name: 'My Space', slug: 'personal', id: 'personal', visibility: 'private', role: 'owner', localPath: '/Users/me/cowiki/personal' };
  const store = new Map<string, string>([
    ['Alpha', '---\ntype: Note\ntitle: Alpha\n---\n\n# Alpha\n\nAlpha body unique.\n'],
    ['Beta', '---\ntype: Note\ntitle: Beta\n---\n\n# Beta\n\nBeta body slow.\n'],
    ['Gamma', '---\ntype: Note\ntitle: Gamma\n---\n\n# Gamma\n\nGamma body third.\n'],
  ]);
  const meta = (slug: string) => ({ slug, path: slug + '.md', title: slug, summary: '', branch: 'main', kind: 'page', children: [] });
  let slowBeta = false;
  const writes: string[] = [];
  let diffSeenAfterWrite = false;

  await page.route('**/*', (route) => route.request().url().startsWith(origin) ? route.continue() : route.abort());
  await page.exposeFunction('fixtureInvoke', async (cmd: string, args: Record<string, unknown>) => {
    switch (cmd) {
      case 'local_list_spaces': return [space];
      case 'local_list_pages': return Array.from(store.keys(), meta);
      case 'local_get_page': {
        const slug = String(args.pageSlug);
        if (slug === 'Beta' && slowBeta) {
          await new Promise((resolve) => setTimeout(resolve, 250));
        }
        return { ...meta(slug), body: store.get(slug) ?? '' };
      }
      case 'local_write_page': {
        const slug = String(args.pageSlug);
        const content = String(args.content ?? '');
        await new Promise((resolve) => setTimeout(resolve, 150));
        store.set(slug, content);
        writes.push(content);
        return null;
      }
      case 'local_working_diff':
        if (writes.some((text) => text.includes('flushed before review'))) diffSeenAfterWrite = true;
        return [];
      case 'local_list_agent_changes':
      case 'local_list_sources':
      case 'local_list_backlinks':
      case 'local_list_task_sources':
      case 'local_list_card_sources':
      case 'local_recent_pages':
      case 'local_list_broken_links':
        return [];
      case 'local_read_space_config': return { features: { modules: { journal: true, tasks: true, flashcards: true }, journal: { format: 'YYYY-MM-DD', folder: 'journal', template: null } } };
      case 'local_list_page_comments': return { comments: [], snapshots: [] };
      case 'cloud_get_status': return { state: 'unlinked', conflicts: [], message: '', pullRequest: null, committed: false, cloudSpaceId: null, cloudBaseUrl: null };
      default: return null;
    }
  });
  await page.addInitScript(() => {
    Object.assign(window, { __TAURI_INTERNALS__: { invoke: (cmd: string, args: unknown) => Reflect.get(window, 'fixtureInvoke')(cmd, args) } });
    localStorage.setItem('cowiki.personal.preferences', JSON.stringify({ autoCheckpoint: false }));
  });

  await page.goto(origin);
  await page.getByText('Alpha', { exact: true }).first().click();
  await expect(page.getByText('Alpha body unique.')).toBeVisible();
  await page.getByText('Beta', { exact: true }).first().click();
  await expect(page.getByText('Beta body slow.')).toBeVisible();
  await page.getByText('Gamma', { exact: true }).first().click();
  await expect(page.getByText('Gamma body third.')).toBeVisible();

  // Make Beta's IPC resolve AFTER Alpha, then double-click Back rapidly (Gamma -> Beta -> Alpha).
  slowBeta = true;
  const back = page.getByRole('button', { name: 'Back', exact: true });
  await back.click();
  await back.click();
  await expect(page.getByText('Alpha body unique.')).toBeVisible();
  // Wait past Beta's delayed IPC and verify Beta did NOT overwrite Alpha.
  await page.waitForTimeout(350);
  await expect(page.getByText('Alpha body unique.')).toBeVisible();
  await expect(page.getByText('Beta body slow.')).toHaveCount(0);

  // Click to edit Alpha, type unsaved text, and immediately click Reviews.
  await page.getByText('Alpha body unique.').click();
  await expect(page.locator('.cm-content')).toBeVisible();
  await page.keyboard.type(' flushed before review');
  await page.getByRole('button', { name: /^Reviews\s*\d*$/ }).first().click();
  await expect(page.locator('.app-breadcrumb')).toContainText('Reviews');
  expect(writes.some((text) => text.includes('flushed before review'))).toBe(true);
  expect(diffSeenAfterWrite).toBe(true);

  // Returning to Alpha should open in reading mode (editingPage was cleared on Reviews tab).
  await page.getByText('Alpha', { exact: true }).first().click();
  await expect(page.locator('.cm-content')).toHaveCount(0);
  await expect(page.getByText(/Alpha body unique\./)).toBeVisible();
});
