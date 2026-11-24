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

async function openModulesSettings(page: Page) {
  await page.getByRole('button', { name: 'Space menu' }).click();
  await page.getByRole('menuitem', { name: /Modules & settings/i }).click();
}

const today = 'Daily/2026-11-01'; // local day at 02:30 UTC on November 2
const unknownSettings = { version: 7, data: ['preserve'] };
interface FixtureOptions {
  beforeConfig?: (space: string) => Promise<void>;
  beforePage?: (space: string, slug: string) => Promise<void>;
  disabled?: boolean;
  secondSpace?: boolean;
}
async function desktop(page: Page, options: FixtureOptions = {}) {
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
      case 'local_working_diff': case 'local_list_agent_changes': case 'local_list_sources': case 'local_list_backlinks': case 'local_recent_pages': return [];
      case 'cloud_get_status': return { state: 'unlinked', conflicts: [], message: '', pullRequest: null, committed: false, cloudSpaceId: null, cloudBaseUrl: null };
      case 'local_set_auto_checkpoint': case 'local_auto_checkpoint': return null;
      default: throw new Error('Unsupported fixture IPC: ' + cmd);
    }
  });
  await page.addInitScript(() => {
    const ipc = { pending: 0, async invoke(cmd: string, args: unknown) {
      ipc.pending++;
      try { return await Reflect.get(window, 'fixtureInvoke')(cmd, args); }
      finally { ipc.pending--; }
    } };
    Object.assign(window, { __TAURI_INTERNALS__: ipc });
    if (!localStorage.getItem('cowiki.personal.preferences'))
      localStorage.setItem('cowiki.personal.preferences', JSON.stringify({ autoCheckpoint: false }));
  });
  return { files, creates, config: () => config };
}

const todayLabel = 'Sunday, November 1, 2026';
const journalsButton = (page: Page) => page.getByRole('button', { name: 'Journals', exact: true });
const todayEntry = (page: Page) => page.getByRole('region', { name: todayLabel });
// An empty day is one bullet ready for text, as in Logseq.
const emptyDay = (page: Page) => todayEntry(page).getByRole('button', { name: 'Write in this day' });

test('a personal Space opens on Journals; today is written where you click and survives disabling', async ({ page }) => {
  const state = await desktop(page);
  state.files.set('Daily/2026-10-30', '# 2026-10-30\n\nAn earlier day\n');
  await page.goto(origin);
  await expect(journalsButton(page)).toHaveAttribute('aria-current', 'page');
  await expect(page.getByRole('button', { name: 'Wiki', exact: true })).toHaveCount(0);
  // Today first, earlier days below; opening Journals writes nothing.
  await expect(emptyDay(page)).toBeVisible();
  await expect(page.getByRole('region', { name: 'Friday, October 30, 2026' })).toContainText('An earlier day');
  expect(state.creates).toEqual([]);

  await emptyDay(page).click();
  // The template's title is the day's heading, so only its body is edited.
  await expect(todayEntry(page).locator('.cm-content')).toHaveText('From my template 2026-11-01');
  await page.keyboard.press('End');
  await page.keyboard.type(' and more');
  await page.keyboard.press('Escape');
  await expect(todayEntry(page).locator('.cm-content')).toHaveCount(0);
  await expect(todayEntry(page)).toContainText('From my template 2026-11-01 and more');
  expect(state.creates).toEqual([today]);
  expect(state.files.get(today)).toBe('# 2026-11-01\n\nFrom my template 2026-11-01 and more');

  // Clicking an earlier day edits that day in place.
  await page.getByText('An earlier day').click();
  await expect(page.locator('.cm-content')).toHaveText('An earlier day');
  await page.keyboard.press('Escape');

  const savedBody = state.files.get(today);
  await openModulesSettings(page);
  const dialog = page.getByRole('dialog');
  await dialog.getByRole('checkbox', { name: /Daily journal|Daily Notes/i }).uncheck();
  await dialog.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(journalsButton(page)).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Home', exact: true })).toBeVisible();
  expect(state.config().features.modules.later).toBe(false);
  expect(state.config().features.nextPlugin).toEqual(unknownSettings);
  expect(state.files.get(today)).toBe(savedBody);
  await page.goto(origin + '/personal/' + today);
  await expect(page.locator('article')).toContainText('From my template 2026-11-01');
  await expect(page.getByRole('button', { name: 'Tasks', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Flashcards', exact: true })).toBeVisible();
});

test('a deep link opens its page; the root opens Journals without writing a note', async ({ page }) => {
  const state = await desktop(page);
  await page.goto(origin + '/personal/Welcome');
  await expect(page.locator('article')).toContainText('Welcome');
  await expect(journalsButton(page)).toBeVisible();
  await page.goto(origin);
  await expect(emptyDay(page)).toBeVisible();
  expect(state.creates).toEqual([]);
});

test('a page edits where it is clicked; selecting text does not edit', async ({ page }) => {
  const state = await desktop(page);
  state.files.set('Welcome', '# Welcome\n\nFirst paragraph here.\n\nSecond paragraph there.\n');
  await page.goto(origin + '/personal/Welcome');
  await expect(page.getByRole('button', { name: 'Edit', exact: true })).toHaveCount(0);
  const second = page.locator('article').getByText('Second paragraph there.');
  // A drag selection is for commenting, not editing.
  const box = (await second.boundingBox())!;
  await page.mouse.move(box.x + 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width - 2, box.y + box.height / 2, { steps: 5 });
  await page.mouse.up();
  await expect(page.locator('.cm-content')).toHaveCount(0);
  await page.evaluate(() => window.getSelection()?.removeAllRanges());
  // A click edits, with the caret where the click was.
  await second.click({ position: { x: 2, y: box.height / 2 } });
  await expect(page.locator('.cm-content')).toBeVisible();
  await page.keyboard.type('NEW ');
  await page.keyboard.press('Escape');
  await expect(page.locator('article')).toContainText('NEW Second paragraph there.');
  expect(state.files.get('Welcome')).toContain('\n\nNEW Second paragraph there.');
});

test('a disabled Space never offers Journals while its settings are still loading', async ({ page }) => {
  const gate = Promise.withResolvers<void>();
  const reading = Promise.withResolvers<void>();
  const state = await desktop(page, { disabled: true, beforeConfig: () => { reading.resolve(); return gate.promise; } });
  try {
    await page.goto(origin);
    await reading.promise;
    // The start page waits for the Space's settings instead of guessing.
    await expect(page.getByRole('button', { name: 'Tasks', exact: true })).toHaveCount(0);
    await expect(journalsButton(page)).toHaveCount(0);
    await expect(page.getByRole('button', { name: 'Home', exact: true })).toHaveCount(0);
  } finally { gate.resolve(); }
  await expect(page.getByRole('button', { name: 'Home', exact: true })).toHaveAttribute('aria-current', 'page');
  await expect(journalsButton(page)).toHaveCount(0);
  await openModulesSettings(page);
  await expect(page.getByRole('checkbox', { name: /Daily journal|Daily Notes/i })).not.toBeChecked();
  expect(state.creates).toEqual([]);
});

for (const switchBack of [false, true]) test(`a slow first write to today is dropped after switching ${switchBack ? 'away and back' : 'away'}`, async ({ page }) => {
  const gate = Promise.withResolvers<void>();
  const started = Promise.withResolvers<void>();
  const state = await desktop(page, { secondSpace: true, beforePage: async (space, slug) => {
    if (space === 'personal' && slug === today) { started.resolve(); await gate.promise; }
  } });
  try {
    await page.goto(origin);
    await emptyDay(page).click();
    await started.promise;
    await page.getByRole('button', { name: 'W', exact: true }).click();
    await expect(page.getByRole('button', { name: 'Space menu' })).toHaveAttribute('title', 'Work');
    await expect(todayEntry(page)).toBeVisible();
    if (switchBack) {
      await page.getByRole('button', { name: 'P', exact: true }).click();
      await expect(page.getByRole('button', { name: 'Space menu' })).toHaveAttribute('title', 'Personal');
    }
  } finally { gate.resolve(); }
  await page.waitForFunction(() => Reflect.get(window, '__TAURI_INTERNALS__').pending === 0);
  await expect(page).toHaveURL(origin + '/');
  await expect(page.locator('.cm-content')).toHaveCount(0);
  expect(state.creates).toEqual([]);
});

test('saving a journal folder does not revert an external update to another plugin', async ({ page }) => {
  const state = await desktop(page);
  await page.goto(origin);
  await expect(journalsButton(page)).toBeVisible();
  await openModulesSettings(page);
  const dialog = page.getByRole('dialog');
  await dialog.locator('section').filter({ has: page.getByRole('heading', { name: 'Journal', exact: true }) }).getByLabel('Folder', { exact: true }).fill('Diary');
  state.config().features.modules.later = true; // another client after opening
  await dialog.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  expect(state.config().features.journal.folder).toBe('Diary');
  expect(state.config().features.modules.later).toBe(true);
});

for (const entry of ['/', '/personal/Welcome']) test(`unreadable settings recover in place (${entry})`, async ({ page }) => {
  let damaged = true;
  const state = await desktop(page, { beforeConfig: async () => {
    if (damaged) throw new Error('Unreadable settings document');
  } });
  await page.goto(origin + entry);
  if (entry === '/') await expect(page.getByRole('button', { name: 'Home', exact: true })).toHaveAttribute('aria-current', 'page');
  else await expect(page.locator('article')).toContainText('Welcome');
  const originalSelection = page.url();
  await openModulesSettings(page);
  const dialog = page.getByRole('dialog');
  await expect(dialog).toContainText('Unreadable settings document');
  await expect(dialog.getByRole('button', { name: 'Save', exact: true })).toBeDisabled();
  expect(state.creates).toEqual([]);
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await expect(journalsButton(page)).toHaveCount(0);
  await openModulesSettings(page);
  damaged = false;
  await dialog.getByRole('button', { name: 'Reload settings', exact: true }).click();
  await expect(dialog.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
  await page.keyboard.press('Escape');
  await expect(journalsButton(page)).toBeVisible();
  await page.waitForFunction(() => Reflect.get(window, '__TAURI_INTERNALS__').pending === 0);
  await expect(page).toHaveURL(originalSelection);
  expect(state.creates).toEqual([]);
  if (entry === '/') {
    await emptyDay(page).click();
    await expect(todayEntry(page).locator('.cm-content')).toHaveText('From my template 2026-11-01');
  } else {
    await expect(page.locator('article')).toContainText('Welcome');
  }
});

test('selecting another Space while the first is loading shows the new Space', async ({ page }) => {
  const gate = Promise.withResolvers<void>();
  const reading = Promise.withResolvers<void>();
  const state = await desktop(page, { secondSpace: true, beforeConfig: async slug => {
    if (slug === 'personal') { reading.resolve(); await gate.promise; }
  } });
  try {
    await page.goto(origin);
    await reading.promise;
    await page.getByRole('button', { name: 'W', exact: true }).click();
    await expect(journalsButton(page)).toBeVisible();
  } finally { gate.resolve(); }
  await page.waitForFunction(() => Reflect.get(window, '__TAURI_INTERNALS__').pending === 0);
  await expect(page.getByRole('button', { name: 'Space menu' })).toHaveAttribute('title', 'Work');
  await expect(todayEntry(page)).toBeVisible();
  expect(state.creates).toEqual([]);
});

test('the settings dialog stays inside a short window with its actions visible', async ({ page }) => {
  await page.setViewportSize({ width: 1100, height: 600 });
  await desktop(page);
  await page.goto(origin);
  await openModulesSettings(page);
  const dialog = page.getByRole('dialog');
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole('button', { name: 'Save', exact: true })).toBeInViewport({ ratio: 1 });
  await expect(dialog.getByRole('heading', { name: 'Modules & settings' })).toBeInViewport({ ratio: 1 });
  const box = (await dialog.boundingBox())!;
  expect(box.y).toBeGreaterThanOrEqual(0);
  expect(box.y + box.height).toBeLessThanOrEqual(600);
  expect(Math.abs(box.x + box.width / 2 - 550)).toBeLessThanOrEqual(1);
  // Module switches are real checkboxes in the CoWiki style, not the platform's.
  const toggle = dialog.getByRole('checkbox', { name: /Daily journal/ });
  expect(await toggle.evaluate((element) => getComputedStyle(element).appearance)).toBe('none');
});
