import assert from 'node:assert/strict';
import { mkdtemp, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import test from 'node:test';

import {
  refreshRepositoryStatus,
  setupCowikiRemote,
} from '../../../skills/cowiki-space/scripts/lib/git.mjs';

test('status refreshes Cloud main after an administrator merges a pull request', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'cowiki-status-'));
  const remote = path.join(root, 'remote.git');
  const repo = path.join(root, 'participant');
  const admin = path.join(root, 'admin');
  run(root, ['init', '--bare', '--initial-branch=main', remote]);
  run(root, ['init', '-b', 'main', repo]);
  configure(repo);
  await writeFile(path.join(repo, 'index.md'), '# Initial\n');
  run(repo, ['add', 'index.md']);
  run(repo, ['commit', '-m', 'initial']);
  run(repo, ['remote', 'add', 'seed', remote]);
  run(repo, ['push', 'seed', 'main']);

  const credential = {
    apiKey: 'cw_key_test',
    userId: '11111111-1111-4111-8111-111111111111',
  };
  const space = {
    gitUrl: remote,
    userRef: `user/${credential.userId}`,
  };
  setupCowikiRemote(repo, space, credential);
  const initial = run(repo, ['rev-parse', 'refs/remotes/cowiki/main']).trim();

  run(root, ['clone', remote, admin]);
  configure(admin);
  await writeFile(path.join(admin, 'index.md'), '# Merged\n');
  run(admin, ['add', 'index.md']);
  run(admin, ['commit', '-m', 'merge project']);
  run(admin, ['push', 'origin', 'main']);
  const merged = run(admin, ['rev-parse', 'HEAD']).trim();
  assert.notEqual(merged, initial);

  const status = refreshRepositoryStatus(repo, space, credential);

  assert.equal(status.head, initial);
  assert.equal(status.cloudMain, merged);
});

function configure(cwd) {
  run(cwd, ['config', 'user.name', 'Test']);
  run(cwd, ['config', 'user.email', 'test@cowiki.local']);
}

function run(cwd, args) {
  const result = spawnSync('git', args, {
    cwd,
    encoding: 'utf8',
    env: { ...process.env, COWIKI_INTERNAL: '1' },
  });
  assert.equal(result.status, 0, result.stderr);
  return result.stdout;
}
