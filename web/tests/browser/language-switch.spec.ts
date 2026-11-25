import { test, expect, type Page } from '@playwright/test';
import { createServer, type ViteDevServer } from 'vite';
import { fileURLToPath } from 'node:url';

let server: ViteDevServer;
let origin: string;

test.beforeAll(async () => {
  server = await createServer({
    root: fileURLToPath(new URL('../../', import.meta.url)),
    logLevel: 'silent',
    server: { host: '127.0.0.1', port: 0 },
  });
  await server.listen();
  const address = server.httpServer!.address();
  if (!address || typeof address === 'string') throw new Error('Missing test port');
  origin = 'http://127.0.0.1:' + address.port;
});

test.afterAll(async () => {
  await server.close();
});

async function setup(page: Page) {
  const space = {
    name: 'Personal',
    slug: 'personal',
    id: 'personal',
    visibility: 'private',
    role: 'owner',
    localPath: '/fixture/personal',
  };
  await page.route('**/*', (route) => (
    route.request().url().startsWith(origin) ? route.continue() : route.abort()
  ));
  await page.exposeFunction('fixtureInvoke', async (cmd: string) => {
    switch (cmd) {
      case 'local_list_spaces': return [space];
      case 'local_list_pages': return [];
      case 'local_read_space_config': return {
        features: { modules: { journal: true, tasks: true, flashcards: true }, journal: { format: 'YYYY-MM-DD', folder: 'Daily', template: null } },
      };
      case 'local_list_card_sources': return [
        { path: 'Languages/Japanese.md', content: '- 食べる #card #Japanese\n  - to eat\n' },
      ];
      case 'local_read_card_reviews': return null;
      case 'local_working_diff':
      case 'local_list_agent_changes':
      case 'local_list_sources':
      case 'local_list_backlinks':
      case 'local_recent_pages':
      case 'local_list_task_sources':
      case 'local_inspect_links':
        return [];
      case 'cloud_get_status':
        return { state: 'unlinked', conflicts: [], message: '', pullRequest: null, committed: false, cloudSpaceId: null, cloudBaseUrl: null };
      case 'local_set_auto_checkpoint':
      case 'local_auto_checkpoint':
        return null;
      default:
        throw new Error('Unsupported fixture IPC: ' + cmd);
    }
  });
  await page.addInitScript(() => {
    Object.assign(window, {
      __TAURI_INTERNALS__: {
        invoke: (cmd: string, args: unknown) => Reflect.get(window, 'fixtureInvoke')(cmd, args),
      },
    });
    localStorage.setItem('cowiki.personal.preferences', JSON.stringify({ autoCheckpoint: false }));
  });
}

test('Settings switches instantly between English and Simplified Chinese and persists on reload', async ({ page }) => {
  await setup(page);
  await page.goto(origin);

  // Starts in English by default.
  await expect(page.getByRole('button', { name: 'Home', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Tasks', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Flashcards', exact: true })).toBeVisible();
  await expect(page.locator('html')).toHaveAttribute('lang', 'en');

  // Open Settings → App → General via the sidebar Settings button.
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await expect(page.getByRole('dialog')).toBeVisible();
  await page.getByRole('navigation', { name: 'Settings' }).getByRole('button', { name: 'General' }).nth(1).click();

  const langGroup = page.getByRole('radiogroup', { name: 'Language' });
  await expect(langGroup.getByRole('radio', { name: 'English' })).toHaveAttribute('aria-checked', 'true');
  await expect(langGroup.getByRole('radio', { name: '中文' })).toHaveAttribute('aria-checked', 'false');

  // Click 中文 — Settings dialog, sidebar, and document language switch immediately without reload.
  await langGroup.getByRole('radio', { name: '中文' }).click();
  await expect(page.locator('html')).toHaveAttribute('lang', 'zh-CN');
  await expect(page.getByRole('heading', { name: '语言' })).toBeVisible();
  await expect(page.getByRole('heading', { name: '外观' })).toBeVisible();
  await expect(page.getByRole('radiogroup', { name: '语言' }).getByRole('radio', { name: '中文' })).toHaveAttribute('aria-checked', 'true');

  // Close Settings and verify sidebar + views render in Chinese immediately.
  await page.keyboard.press('Escape');
  await expect(page.getByRole('button', { name: '首页', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: '任务', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: '闪卡', exact: true })).toBeVisible();

  await page.getByRole('button', { name: '任务', exact: true }).click();
  await expect(page.getByRole('heading', { name: '任务' })).toBeVisible();
  await expect(page.getByRole('tab', { name: /今天/ })).toBeVisible();

  await page.getByRole('button', { name: '闪卡', exact: true }).click();
  await expect(page.getByRole('heading', { name: '闪卡' })).toBeVisible();
  await expect(page.getByRole('button', { name: '全部学习 · 1' })).toBeVisible();

  // Journal days are dated the Chinese way, not "Nov 25th, 2026".
  await page.getByRole('button', { name: '日记', exact: true }).click();
  await expect(page.locator('.journal-day-title').first()).toHaveText(/^\d{4}年\d{1,2}月\d{1,2}日$/);

  // Persists across reload.
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('lang', 'zh-CN');
  await expect(page.getByRole('button', { name: '首页', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: '设置', exact: true })).toBeVisible();

  // Switch back to English immediately.
  await page.getByRole('button', { name: '设置', exact: true }).click();
  await page.getByRole('navigation', { name: '设置' }).getByRole('button', { name: '通用' }).nth(1).click();
  await page.getByRole('radiogroup', { name: '语言' }).getByRole('radio', { name: 'English' }).click();
  await expect(page.locator('html')).toHaveAttribute('lang', 'en');
  await expect(page.getByRole('heading', { name: 'Language' })).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('button', { name: 'Home', exact: true })).toBeVisible();
});
