import assert from 'node:assert/strict';
import { mkdtemp, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import test from 'node:test';

import {
  setupCowikiRemote,
  submitRepository,
} from '../../../skills/cowiki-space/scripts/lib/git.mjs';

test('submit rebases, pushes only the user branch with a lease, and creates a PR', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'cowiki-submit-'));
  const remote = path.join(root, 'remote.git');
  const repo = path.join(root, 'work');
  run(root, ['init', '--bare', '--initial-branch=main', remote]);
  run(root, ['init', '-b', 'main', repo]);
  run(repo, ['config', 'user.name', 'Test']);
  run(repo, ['config', 'user.email', 'test@cowiki.local']);
  await writeFile(path.join(repo, 'index.md'), '# One\n');
  run(repo, ['add', 'index.md']);
  run(repo, ['commit', '-m', 'initial']);
  run(repo, ['remote', 'add', 'seed', remote]);
  run(repo, ['push', 'seed', 'main']);

  const credential = {
    server: 'https://cloud.cowiki.test',
    apiKey: 'cw_key_secret',
    userId: '11111111-1111-4111-8111-111111111111',
    userName: 'Ada',
  };
  const space = {
    id: '22222222-2222-4222-8222-222222222222',
    gitUrl: remote,
    userRef: `user/${credential.userId}`,
  };
  setupCowikiRemote(repo, space, credential);
  await writeFile(path.join(repo, 'index.md'), '# Two\n');
  const requests = [];
  const cloud = {
    async createOrUpdatePullRequest(spaceId, title, body) {
      requests.push({ spaceId, title, body });
      return { id: 'pr-id', number: 1, title, status: 'open' };
    },
  };

  const result = await submitRepository({
    cwd: repo,
    message: 'Share solution',
    body: 'Ready',
    space,
    credential,
    cloud,
  });
  assert.equal(result.pullRequest.number, 1);
  assert.deepEqual(requests, [{
    spaceId: space.id,
    title: 'Share solution',
    body: 'Ready',
  }]);
  assert.equal(
    run(root, ['--git-dir', remote, 'rev-parse', `refs/heads/${space.userRef}`]).trim(),
    run(repo, ['rev-parse', 'HEAD']).trim(),
  );
  assert.equal(run(root, ['--git-dir', remote, 'rev-parse', 'refs/heads/main']).trim() !== '', true);
});

function run(cwd, args) {
  const result = spawnSync('git', args, {
    cwd,
    encoding: 'utf8',
    env: { ...process.env, COWIKI_INTERNAL: '1' },
  });
  assert.equal(result.status, 0, result.stderr);
  return result.stdout;
}
