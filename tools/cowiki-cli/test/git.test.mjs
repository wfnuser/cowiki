import assert from 'node:assert/strict';
import { mkdtemp, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import test from 'node:test';

import {
  assertMarkdownOnly,
  commitMarkdown,
  gitAuthEnvironment,
  ignoreCloudConfig,
  listDirtyPaths,
} from '../../../skills/cowiki-space/scripts/lib/git.mjs';

test('submission commits Markdown but refuses unsupported dirty files', async () => {
  const repo = await mkdtemp(path.join(os.tmpdir(), 'cowiki-git-'));
  run(repo, ['init', '-b', 'main']);
  run(repo, ['config', 'user.name', 'Test']);
  run(repo, ['config', 'user.email', 'test@cowiki.local']);
  await writeFile(path.join(repo, 'index.md'), '# One\n');
  run(repo, ['add', 'index.md']);
  run(repo, ['commit', '-m', 'initial']);

  await writeFile(path.join(repo, 'index.md'), '# Two\n');
  await writeFile(path.join(repo, 'notes.txt'), 'unsupported\n');
  const dirty = listDirtyPaths(repo);
  assert.deepEqual(dirty.sort(), ['index.md', 'notes.txt']);
  assert.throws(() => assertMarkdownOnly(dirty), /notes\.txt/);

  run(repo, ['clean', '-f', '--', 'notes.txt']);
  const committed = commitMarkdown(repo, 'Update page', {
    userId: '11111111-1111-4111-8111-111111111111',
    userName: 'Ada',
  });
  assert.equal(committed, true);
  assert.equal(run(repo, ['show', '--format=', '--name-only', 'HEAD']).trim(), 'index.md');
});

test('Git bearer credentials are passed through environment configuration, not arguments', () => {
  const env = gitAuthEnvironment('cw_key_secret', {});
  assert.equal(env.GIT_CONFIG_COUNT, '1');
  assert.equal(env.GIT_CONFIG_KEY_0, 'http.extraHeader');
  assert.equal(env.GIT_CONFIG_VALUE_0, 'Authorization: Bearer cw_key_secret');
});

test('local Cloud linkage is excluded from Git submissions', async () => {
  const repo = await mkdtemp(path.join(os.tmpdir(), 'cowiki-ignore-'));
  run(repo, ['init', '-b', 'main']);
  await writeFile(path.join(repo, 'index.md'), '# Space\n');
  run(repo, ['add', 'index.md']);
  run(repo, ['-c', 'user.name=Test', '-c', 'user.email=test@cowiki.local', 'commit', '-m', 'initial']);
  ignoreCloudConfig(repo);
  const ignored = spawnSync('git', ['check-ignore', '.cowiki/cloud.json'], {
    cwd: repo,
    encoding: 'utf8',
  });
  assert.equal(ignored.status, 0, ignored.stderr);
});

function run(cwd, args) {
  const result = spawnSync('git', args, { cwd, encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
  return result.stdout;
}
