import assert from 'node:assert/strict';
import test from 'node:test';

import {
  buildDesktopGithubLoginUrl,
  buildWebGithubLoginUrl,
  parseDesktopOAuthCallback,
} from '../src/auth-flow.ts';

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
