import assert from 'node:assert/strict';
import { mkdtemp, readFile, stat } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

import {
  normalizeServerOrigin,
  readCredential,
  writeCredential,
} from '../../../skills/cowiki-space/scripts/lib/config.mjs';

test('credentials are scoped to one normalized server and stored user-only', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'cowiki-config-'));
  const env = { XDG_CONFIG_HOME: root };
  const credential = {
    server: 'https://cloud.cowiki.test',
    apiKey: 'cw_key_secret',
    userId: '11111111-1111-4111-8111-111111111111',
    userName: 'Ada',
  };
  const filename = await writeCredential(credential, { env, platform: 'linux' });
  assert.equal(filename, path.join(root, 'cowiki', 'credentials.json'));
  assert.deepEqual(await readCredential('https://cloud.cowiki.test/', { env, platform: 'linux' }), credential);
  assert.equal(JSON.parse(await readFile(filename, 'utf8')).apiKey, 'cw_key_secret');
  if (process.platform !== 'win32') {
    assert.equal((await stat(filename)).mode & 0o777, 0o600);
  }
});

test('server origins reject credentials, paths, and non-http schemes', () => {
  assert.equal(normalizeServerOrigin('https://cloud.cowiki.test/'), 'https://cloud.cowiki.test');
  for (const value of [
    'https://user:secret@cloud.cowiki.test',
    'https://cloud.cowiki.test/path',
    'file:///tmp/cloud',
  ]) {
    assert.throws(() => normalizeServerOrigin(value));
  }
});
