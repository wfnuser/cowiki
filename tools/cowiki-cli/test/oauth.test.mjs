import assert from 'node:assert/strict';
import test from 'node:test';

import { loginWithBrowser } from '../../../skills/cowiki-space/scripts/lib/oauth.mjs';

test('CLI login accepts one loopback code and exchanges it without exposing the API key', async () => {
  const calls = [];
  const credential = await loginWithBrowser({
    server: 'https://cloud.cowiki.test',
    timeoutMs: 2_000,
    openBrowser: async (loginUrl) => {
      const url = new URL(loginUrl);
      calls.push(url);
      assert.equal(url.searchParams.get('client'), 'cli');
      const callback = new URL(url.searchParams.get('callback'));
      assert.equal(callback.hostname, '127.0.0.1');
      assert.equal(callback.pathname, '/auth/callback');
      callback.searchParams.set('code', 'cw_once_test');
      await fetch(callback);
    },
    fetchImpl: async (input, init) => {
      calls.push({ input: String(input), init });
      return Response.json({
        apiKey: 'cw_key_secret',
        userId: '11111111-1111-4111-8111-111111111111',
        userName: 'Ada',
      });
    },
  });
  assert.equal(credential.apiKey, 'cw_key_secret');
  assert.equal(calls[1].input, 'https://cloud.cowiki.test/api/auth/exchange');
  assert.deepEqual(JSON.parse(calls[1].init.body), {
    code: 'cw_once_test',
    client: 'cli',
  });
});
