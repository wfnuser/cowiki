import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import {
  buildDesktopGithubLoginUrl,
  buildLoopbackGithubLoginUrl,
  buildWebGithubLoginUrl,
  createWebAuthBootstrap,
  parseWebOAuthFragment,
  parseDesktopOAuthCallback,
  safeAuthReturnPath,
  validateWebCredential,
} from '../src/auth-flow.ts';

const appSource = readFileSync(new URL('../src/App.tsx', import.meta.url), 'utf8');
const loginPage = readFileSync(new URL('../src/pages/LoginPage.tsx', import.meta.url), 'utf8');
const spaceRail = readFileSync(new URL('../src/components/layout/SpaceRail.tsx', import.meta.url), 'utf8');

test('web GitHub login keeps the existing browser URL', () => {
  assert.equal(
    buildWebGithubLoginUrl('https://api-test.cowiki.app/api'),
    'https://api-test.cowiki.app/api/auth/github',
  );
});

test('desktop GitHub login carries a loopback callback to the backend', () => {
  assert.equal(
    buildDesktopGithubLoginUrl(
      'http://localhost:3000/api',
      'http://127.0.0.1:39281/auth/callback',
    ),
    'http://localhost:3000/api/auth/github?client=desktop&callback=http%3A%2F%2F127.0.0.1%3A39281%2Fauth%2Fcallback',
  );
});

test('desktop callback accepts credential query params from the loopback listener', () => {
  assert.deepEqual(
    parseDesktopOAuthCallback(
      'http://127.0.0.1:39281/auth/callback?api_key=cw_123&user_name=octo-cat&user_id=user-1',
    ),
    { apiKey: 'cw_123', userName: 'octo-cat', userId: 'user-1' },
  );
});

test('browser OAuth exchanges a short-lived fragment code and accepts only local return paths', () => {
  assert.equal(parseWebOAuthFragment('#auth_code=cw_once_test'), 'cw_once_test');
  assert.equal(parseWebOAuthFragment('#api_key=cw_key_secret'), null);
  assert.equal(safeAuthReturnPath('/invite/cw_invite_test'), '/invite/cw_invite_test');
  assert.equal(safeAuthReturnPath('https://evil.example/invite/test'), '/cloud');
  assert.equal(safeAuthReturnPath('//evil.example/invite/test'), '/cloud');
});

test('web auth bootstrap is single-flight when React StrictMode starts it twice', async () => {
  let exchanges = 0;
  let stores = 0;
  let clears = 0;
  const run = createWebAuthBootstrap({
    readOAuthCode: () => 'cw_once_test',
    exchangeOAuthCode: async () => {
      exchanges += 1;
      return { apiKey: 'cw_key_test', userName: 'octo-cat', userId: 'user-1' };
    },
    storeCredential: () => { stores += 1; },
    hasStoredCredential: () => true,
    validateStoredCredential: async () => new Response(null, { status: 200 }),
    clearCredential: () => { clears += 1; },
    finishOAuth: () => {},
    failOAuth: () => {},
  });

  const first = run();
  const second = run();

  assert.strictEqual(first, second);
  await Promise.all([first, second]);
  assert.equal(exchanges, 1);
  assert.equal(stores, 1);
  assert.equal(clears, 0);
});

test('web auth bootstrap does not probe desktop local login without a Cloud credential', async () => {
  let validations = 0;
  const run = createWebAuthBootstrap({
    readOAuthCode: () => null,
    exchangeOAuthCode: async () => {
      throw new Error('OAuth exchange should not run');
    },
    storeCredential: () => {},
    hasStoredCredential: () => false,
    validateStoredCredential: async () => {
      validations += 1;
      return new Response(null, { status: 200 });
    },
    clearCredential: () => {},
    finishOAuth: () => {},
    failOAuth: () => {},
  });

  await run();

  assert.equal(validations, 0);
  assert.doesNotMatch(appSource, /tryLocalLogin/);
});

test('stored web credentials are validated against the Cloud /api/me endpoint', async () => {
  const calls: string[] = [];
  const response = await validateWebCredential(
    'http://localhost:5173/api',
    { Authorization: 'Bearer cw_key_test' },
    async (input) => {
      calls.push(String(input));
      return new Response(null, { status: 200 });
    },
  );

  assert.equal(response.status, 200);
  assert.deepEqual(calls, ['http://localhost:5173/api/me']);
  assert.doesNotMatch(appSource, /\/auth\/me/);
});

test('CLI OAuth uses the same exact loopback boundary as desktop', () => {
  assert.equal(
    buildLoopbackGithubLoginUrl(
      'https://cloud.cowiki.test/api',
      'cli',
      'http://127.0.0.1:40821/auth/callback',
    ),
    'https://cloud.cowiki.test/api/auth/github?client=cli&callback=http%3A%2F%2F127.0.0.1%3A40821%2Fauth%2Fcallback',
  );
});

test('desktop sign in follows the local-first shell design', () => {
  assert.match(loginPage, /LOCAL FIRST/);
  assert.match(loginPage, /Continue locally/);
  assert.match(loginPage, /Signing in does not upload a local Space/);
  assert.match(spaceRail, /Not signed in/);
  assert.match(spaceRail, />\s*Sign in\s*</);
});
