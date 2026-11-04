import { test, expect, type Page } from '@playwright/test';
import { createServer, type ViteDevServer } from 'vite';
import { fileURLToPath } from 'node:url';

// Exercise the actual desktop UI with a disposable native IPC boundary, not a
// replacement React component. Files/settings survive a reload in each test.
let server: ViteDevServer;
let origin: string;
test.use({ timezoneId: 'America/New_York' });
test.beforeAll(async () => {
  server = await createServer({ root: fileURLToPath(new URL('../../', import.meta.url)), logLevel: 'silent', server: { host: '127.0.0.1', port: 0 } });
  await server.listen();
  const address = server.httpServer!.address();
  if (!address || typeof address === 'string') throw new Error('Missing test port');
  origin = 'http://127.0.0.1:' + address.port;
});
test.afterAll(async () => { await server.close(); });

const today = 'Daily/2026-11-01'; // local day at 02:30 UTC on November 2
const unknownSettings = { version: 7, data: ['preserve'] };
interface FixtureOptions {
  beforeConfig?: (space: string) => Promise<void>;
  beforePage?: (space: string, slug: string) => Promise<void>;
  disabled?: boolean;
  secondSpace?: boolean;
}
async function desktop(page: Page, openOnStartup = false, options: FixtureOptions = {}) {
  const files = new Map([['Welcome', '# Welcome\n'], ['Template', '# {{title}}\n\nFrom my template {{date}}']]);
  const space = { name: 'Personal', slug: 'personal', id: 'personal', visibility: 'private', role: 'owner', localPath: '/fixture/personal' };
  const work = { ...space, name: 'Work', slug: 'work', id: 'work', localPath: '/fixture/work' };
  const otherFiles = new Map([['Other', '# Other notebook\n']]);
  let config = { features: { modules: { journal: !options.disabled, later: false }, journal: { format: 'YYYY-MM-DD', folder: 'Daily', template: 'Template.md' }, nextPlugin: unknownSettings } };
  const creates: string[] = [];
  const meta = (slug: string) => ({ slug, path: slug + '.md', title: slug, summary: '', branch: 'main', kind: 'page', children: [] });
  await page.clock.setFixedTime(new Date('2026-11-02T02:30:00Z'));
  await page.route('**/*', route => route.request().url().startsWith(origin) ? route.continue() : route.abort());
  await page.exposeFunction('fixtureInvoke', async (cmd: string, args: { spaceSlug: string; pageSlug: string; content: string; createOnly: boolean; expectedContent?: string; config: typeof config }) => {
    const notes = args?.spaceSlug === 'work' ? otherFiles : files;
    switch (cmd) {
      case 'local_list_spaces': return options.secondSpace ? [space, work] : [space];
      case 'local_list_pages': return Array.from(notes.keys(), meta);
      case 'local_get_page': {
        await options.beforePage?.(args.spaceSlug, args.pageSlug);
        if (!notes.has(args.pageSlug)) throw new Error('No such file or directory (os error 2)');
        return { ...meta(args.pageSlug), body: notes.get(args.pageSlug) };
      }
      case 'local_write_page':
        if (args.createOnly && notes.has(args.pageSlug)) throw new Error('a page with this name already exists');
        if (args.expectedContent != null && args.expectedContent !== notes.get(args.pageSlug)) throw new Error('page changed');
        notes.set(args.pageSlug, args.content);
        if (args.createOnly) creates.push(args.pageSlug);
        return;
      case 'local_read_space_config': await options.beforeConfig?.(args.spaceSlug); return config;
      case 'local_write_space_config': config = args.config; return;
      case 'local_list_page_comments': return { comments: [], snapshots: [] };
      case 'local_working_diff': case 'local_list_agent_changes': case 'local_list_sources': case 'local_list_backlinks': return [];
      case 'cloud_get_status': return { state: 'unlinked', conflicts: [], message: '', pullRequest: null, committed: false, cloudSpaceId: null, cloudBaseUrl: null };
      case 'local_set_auto_checkpoint': case 'local_auto_checkpoint': return null;
      default: throw new Error('Unsupported fixture IPC: ' + cmd);
    }
  });
  await page.addInitScript((startup: boolean) => {
    const ipc = { pending: 0, async invoke(cmd: string, args: unknown) {
      ipc.pending++;
      try { return await Reflect.get(window, 'fixtureInvoke')(cmd, args); }
      finally { ipc.pending--; }
    } };
    Object.assign(window, { __TAURI_INTERNALS__: ipc });
    if (!localStorage.getItem('cowiki.personal.preferences'))
      localStorage.setItem('cowiki.personal.preferences', JSON.stringify({ openJournalOnStartup: startup, autoCheckpoint: false }));
  }, openOnStartup);
  return { files, creates, config: () => config };
}

test('Today creates one local-date Markdown note; disabling preserves it and unrelated settings', async ({ page }) => {
  const state = await desktop(page);
  await page.goto(origin);
  const action = page.getByRole('button', { name: 'Today', exact: true });
  await action.click();
  await expect(page.locator('.cm-content')).toContainText('From my template 2026-11-01');
  await action.click();
  expect(state.creates).toEqual([today]);
  const savedBody = state.files.get(today);
  await page.getByRole('button', { name: /Modules & settings/i }).click();
  const dialog = page.getByRole('dialog');
  await dialog.getByRole('checkbox', { name: /Daily journal|Daily Notes/i }).uncheck();
  await dialog.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(action).toHaveCount(0);
  expect(state.config().features.modules.later).toBe(false);
  expect(state.config().features.nextPlugin).toEqual(unknownSettings);
  expect(state.files.get(today)).toBe(savedBody);
  await page.goto(origin + '/personal/' + today);
  await expect(page.locator('article')).toContainText('From my template 2026-11-01');
  await expect(action).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Tasks', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Flashcards', exact: true })).toBeVisible();
});

test('device start preference applies on root launch but never takes over a deep link', async ({ page }) => {
  const state = await desktop(page, true);
  await page.goto(origin + '/personal/Welcome');
  await expect(page.locator('article')).toContainText('Welcome');
  await expect(page.getByRole('button', { name: 'Today', exact: true })).toBeVisible();
  expect(state.creates).toEqual([]);
  await page.goto(origin);
  await expect(page.locator('.cm-content')).toContainText('From my template 2026-11-01');
  expect(state.creates).toEqual([today]);
});

test('a disabled Space never offers Today while its settings are still loading', async ({ page }) => {
  const gate = Promise.withResolvers<void>();
  const reading = Promise.withResolvers<void>();
  const state = await desktop(page, true, { disabled: true, beforeConfig: () => { reading.resolve(); return gate.promise; } });
  try {
    await page.goto(origin);
    await reading.promise;
    await expect(page.getByRole('button', { name: 'Wiki', exact: true })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Today', exact: true })).toHaveCount(0);
  } finally { gate.resolve(); }
  await page.getByRole('button', { name: /Modules & settings/i }).click();
  await expect(page.getByRole('checkbox', { name: /Daily journal|Daily Notes/i })).not.toBeChecked();
  expect(state.creates).toEqual([]);
});

for (const switchBack of [false, true]) test(`a slow Today request stays cancelled after switching ${switchBack ? 'away and back' : 'away'}`, async ({ page }) => {
  const gate = Promise.withResolvers<void>();
  const started = Promise.withResolvers<void>();
  const state = await desktop(page, false, { secondSpace: true, beforePage: async (space, slug) => {
    if (space === 'personal' && slug === today) { started.resolve(); await gate.promise; }
  } });
  if (!switchBack) state.files.set(today, '# Private journal\n');
  let userSelection: string;
  try {
    await page.goto(origin + '/personal/Welcome');
    await expect(page.locator('article')).toContainText('Welcome');
    await page.getByRole('button', { name: 'Today', exact: true }).click();
    await started.promise;
    await page.getByRole('button', { name: 'W', exact: true }).click();
    await expect(page.locator('article')).toContainText('Other notebook');
    if (switchBack) {
      await page.getByRole('button', { name: 'P', exact: true }).click();
      await expect(page).toHaveURL(/\/personal\/(Welcome|Template)$/);
      await expect(page.locator('article')).toContainText(/Welcome|From my template/);
    }
    userSelection = page.url();
  } finally { gate.resolve(); }
  await page.waitForFunction(() => Reflect.get(window, '__TAURI_INTERNALS__').pending === 0);
  await expect(page).toHaveURL(userSelection);
  await expect(page.locator('article')).toContainText(switchBack ? /Welcome|From my template/ : 'Other notebook');
  await expect(page.locator('.cm-content')).toHaveCount(0);
  expect(state.creates).toEqual([]);
});

test('saving a journal folder does not revert an external update to another plugin', async ({ page }) => {
  const state = await desktop(page);
  await page.goto(origin);
  await expect(page.getByRole('button', { name: 'Today', exact: true })).toBeVisible();
  await page.getByRole('button', { name: /Modules & settings/i }).click();
  const dialog = page.getByRole('dialog');
  await dialog.locator('section').filter({ has: page.getByRole('heading', { name: 'Journal', exact: true }) }).getByLabel('Folder', { exact: true }).fill('Diary');
  state.config().features.modules.later = true; // another client after opening
  await dialog.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  expect(state.config().features.journal.folder).toBe('Diary');
  expect(state.config().features.modules.later).toBe(true);
});

for (const entry of ['/', '/personal/Welcome']) test(`unreadable settings recover without replaying startup (${entry})`, async ({ page }) => {
  let damaged = true;
  const state = await desktop(page, true, { beforeConfig: async () => {
    if (damaged) throw new Error('Unreadable settings document');
  } });
  await page.goto(origin + entry);
  await expect(page.locator('article')).toContainText(/Welcome|From my template/);
  const originalSelection = page.url();
  await page.getByRole('button', { name: /Modules & settings/i }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toContainText('Unreadable settings document');
  await expect(dialog.getByRole('button', { name: 'Save', exact: true })).toBeDisabled();
  expect(state.creates).toEqual([]);
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Today', exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: /Modules & settings/i }).click();
  damaged = false;
  await dialog.getByRole('button', { name: 'Reload settings', exact: true }).click();
  await expect(dialog.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('button', { name: 'Today', exact: true })).toBeVisible();
  await page.waitForFunction(() => Reflect.get(window, '__TAURI_INTERNALS__').pending === 0);
  await expect(page).toHaveURL(originalSelection);
  await expect(page.locator('article')).toContainText(/Welcome|From my template/);
  expect(state.creates).toEqual([]);
  await page.getByRole('button', { name: 'Today', exact: true }).click();
  await expect(page.locator('.cm-content')).toContainText('From my template 2026-11-01');
});

test('selecting a new Space cancels startup while the first Space is loading', async ({ page }) => {
  const gate = Promise.withResolvers<void>();
  const reading = Promise.withResolvers<void>();
  const state = await desktop(page, true, { secondSpace: true, beforeConfig: async slug => {
    if (slug === 'personal') { reading.resolve(); await gate.promise; }
  } });
  try {
    await page.goto(origin);
    await reading.promise;
    await page.getByRole('button', { name: 'W', exact: true }).click();
    await expect(page.getByRole('button', { name: 'Today', exact: true })).toBeVisible();
  } finally { gate.resolve(); }
  await page.waitForFunction(() => Reflect.get(window, '__TAURI_INTERNALS__').pending === 0);
  await expect(page).toHaveURL(/\/work\/Other$/);
  await expect(page.locator('article')).toContainText('Other notebook');
  expect(state.creates).toEqual([]);
});
