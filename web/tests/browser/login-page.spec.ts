import { expect, test } from '@playwright/test';
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

test('desktop GitHub sign-in shows pending state, explains failures and cancellation, and retries cleanly', async ({ page }) => {
  await page.route('**/*', (route) => route.request().url().startsWith(origin) ? route.continue() : route.abort());

  let attempt = 0;
  let releaseFirst!: () => void;
  const firstPending = new Promise<void>((resolve) => { releaseFirst = resolve; });

  await page.exposeFunction('fixtureInvoke', async (cmd: string) => {
    switch (cmd) {
      case 'start_desktop_oauth': {
        attempt += 1;
        if (attempt === 1) {
          await firstPending;
          throw new Error('Cloud sign-in timed out');
        }
        if (attempt === 2) {
          throw new Error('Cloud sign-in timed out');
        }
        return { apiKey: 'cw_test_key', userName: 'alice', userId: 'u-1' };
      }
      case 'local_list_spaces':
      case 'local_working_diff':
      case 'local_list_agent_changes':
      case 'local_list_sources':
      case 'local_list_backlinks':
      case 'local_recent_pages':
        return [];
      default:
        return null;
    }
  });

  await page.addInitScript(() => {
    Object.assign(window, { __TAURI_INTERNALS__: { invoke: (cmd: string, args: unknown) => Reflect.get(window, 'fixtureInvoke')(cmd, args) } });
  });

  await page.goto(origin + '/login');
  const githubBtn = page.getByRole('link', { name: 'Continue with GitHub' });
  await expect(githubBtn).toBeVisible();

  // Attempt 1: shows pending state, user cancels cleanly.
  await githubBtn.click();
  await expect(page.getByRole('link', { name: 'Waiting for GitHub…' })).toHaveAttribute('aria-busy', 'true');
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('Sign-in was cancelled');
  releaseFirst();

  // Attempt 2: fails with timeout, explains why, and allows retry.
  await page.getByRole('link', { name: 'Continue with GitHub' }).click();
  await expect(page.getByRole('alert')).toContainText('Sign-in timed out before GitHub responded');

  // Attempt 3: succeeds, stores credential, and enters the app.
  await page.getByRole('link', { name: 'Continue with GitHub' }).click();
  await expect(page).toHaveURL(origin + '/');
  expect(await page.evaluate(() => JSON.parse(localStorage.getItem('cowiki_user') || 'null')?.name)).toBe('alice');
});
