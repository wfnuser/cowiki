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

// One open task today, enough finished ones to overflow the window, and one open task far down the page.
const done = Array.from({ length: 40 }, (_, index) => `- [x] Finished task ${index + 1} ✅ 2026-11-2${index % 4}`).join('\n');
const TASKS = `---\ntype: Note\ntitle: Plan\n---\n\n# Plan\n\n- [ ] Write the report 📅 2026-11-24\n${done}\n- [ ] Deep bottom task 📅 2026-11-24\n`;

async function desktop(page: Page) {
  const space = { name: 'My Space', slug: 'personal', id: 'personal', visibility: 'private', role: 'owner', localPath: '/Users/me/cowiki/personal' };
  const files = new Map<string, string>([['Plan.md', TASKS]]);
  await page.clock.setFixedTime(new Date(2026, 10, 24, 15, 0));
  await page.route('**/*', (route) => route.request().url().startsWith(origin) ? route.continue() : route.abort());
  await page.exposeFunction('fixtureInvoke', async (cmd: string, args?: { pageSlug?: string; content?: string }) => {
    switch (cmd) {
      case 'local_list_spaces': return [space];
      case 'local_list_pages': return [...files.keys()].map((path) => {
        const slug = path.replace(/\.md$/i, '');
        return { slug, path, title: slug.split('/').at(-1) ?? slug, summary: '', branch: 'main', kind: 'page', children: [] };
      });
      case 'local_get_page': {
        const path = `${args?.pageSlug ?? ''}.md`;
        const body = files.get(path);
        if (body === undefined) throw new Error('No such file or directory (os error 2)');
        return { slug: args?.pageSlug ?? '', path, title: path, summary: '', branch: 'main', kind: 'page', children: [], body };
      }
      case 'local_write_page': {
        const path = `${args?.pageSlug ?? ''}.md`;
        files.set(path, args?.content ?? '');
        return null;
      }
      case 'local_read_space_config': return { features: { modules: { journal: true, tasks: true, flashcards: true }, journal: { format: 'YYYY-MM-DD', folder: 'journal', template: null } } };
      case 'local_list_task_sources': return [...files.entries()].map(([path, content]) => ({ path, content }));
      case 'local_list_card_sources': case 'local_recent_pages': case 'local_list_sources': case 'local_list_backlinks': return [];
      case 'local_list_page_comments': return { comments: [], snapshots: [] };
      case 'local_working_diff': case 'local_list_agent_changes': return [];
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

test('every tab switches between list and board, and remembers it', async ({ page }) => {
  await page.setViewportSize({ width: 1200, height: 700 });
  await desktop(page);
  await page.goto(origin);
  await page.getByRole('button', { name: /^Tasks\s*\d*$/ }).first().click();
  await expect(page.getByText('Write the report')).toBeVisible();

  // Today starts as a list; as a board its open task is in To do.
  await expect(page.getByRole('list', { name: 'To do' })).toHaveCount(0);
  await page.getByRole('button', { name: 'Board', exact: true }).click();
  await expect(page.getByRole('list', { name: 'To do' }).getByText('Write the report')).toBeVisible();
  await expect(page.getByRole('list', { name: 'Doing' })).toBeVisible();

  // This week starts as a board and reads as a list by status too.
  await page.getByRole('tab', { name: /^This week/ }).click();
  await expect(page.getByRole('list', { name: 'Doing' })).toBeVisible();
  await page.getByRole('button', { name: 'List', exact: true }).click();
  await expect(page.getByRole('heading', { name: /^To do/ })).toBeVisible();
  await expect(page.getByRole('list', { name: 'Doing' })).toHaveCount(0);

  // Done is only a list.
  await page.getByRole('tab', { name: /^Done/ }).click();
  await expect(page.getByRole('group', { name: 'Layout' })).toHaveCount(0);

  // The choices survive a reload.
  await page.reload();
  await page.getByRole('button', { name: /^Tasks\s*\d*$/ }).first().click();
  await expect(page.getByRole('list', { name: 'To do' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Board', exact: true })).toHaveAttribute('aria-pressed', 'true');
});

test('adding a task on Upcoming defaults to tomorrow and appears in Upcoming', async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 700 });
  await desktop(page);
  await page.goto(origin);
  await page.getByRole('button', { name: /^Tasks\s*\d*$/ }).first().click();

  await page.getByRole('tab', { name: /^Upcoming/ }).click();
  await expect(page.getByRole('button', { name: 'Due date' })).toContainText('Tomorrow');
  await page.getByRole('textbox', { name: 'New task' }).fill('Book flight');
  await page.getByRole('button', { name: 'Add', exact: true }).click();
  await expect(page.getByText('Book flight')).toBeVisible();
  // Next quick-add in Upcoming keeps defaulting to Tomorrow.
  await expect(page.getByRole('button', { name: 'Due date' })).toContainText('Tomorrow');

  // No date hides Today-journal quick add until an undated inbox destination is decided.
  await page.getByRole('tab', { name: /^No date/ }).click();
  await expect(page.getByRole('textbox', { name: 'New task' })).toHaveCount(0);
});

test('board cards can return from Doing to To do and task text jumps to its line', async ({ page }) => {
  await page.setViewportSize({ width: 1200, height: 600 });
  await desktop(page);
  await page.goto(origin);
  await page.getByRole('button', { name: /^Tasks\s*\d*$/ }).first().click();
  await page.getByRole('button', { name: 'Board', exact: true }).click();

  const todoList = page.getByRole('list', { name: 'To do' });
  const doingList = page.getByRole('list', { name: 'Doing' });
  await expect(todoList.getByText('Write the report')).toBeVisible();

  // Start moves to Doing, and the symmetric return button moves it back to To do.
  await todoList.getByRole('button', { name: 'Start Write the report' }).click();
  await expect(doingList.getByText('Write the report')).toBeVisible();
  await doingList.getByRole('button', { name: 'Move Write the report back to To do' }).click();
  await expect(todoList.getByText('Write the report')).toBeVisible();

  // Clicking a task near the bottom of a long page opens that page scrolled to that line.
  await todoList.getByRole('button', { name: 'Deep bottom task', exact: true }).click();
  await expect(page.locator('article').getByText('Deep bottom task')).toBeInViewport();
});
