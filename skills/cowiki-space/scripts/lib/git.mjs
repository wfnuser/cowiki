import {
  appendFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
} from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

export function gitAuthEnvironment(apiKey, base = process.env) {
  if (!apiKey?.startsWith('cw_key_')) throw new Error('A valid CoWiki credential is required');
  return {
    ...base,
    GIT_CONFIG_COUNT: '1',
    GIT_CONFIG_KEY_0: 'http.extraHeader',
    GIT_CONFIG_VALUE_0: `Authorization: Bearer ${apiKey}`,
  };
}

export function runGit(cwd, args, { apiKey, allowFailure = false, env = process.env } = {}) {
  const result = spawnSync('git', args, {
    cwd,
    encoding: 'utf8',
    env: apiKey ? gitAuthEnvironment(apiKey, env) : env,
  });
  if (result.error) throw new Error(`Could not start Git: ${result.error.message}`);
  if (!allowFailure && result.status !== 0) {
    throw new Error(result.stderr.trim() || `Git ${args[0]} failed`);
  }
  return result;
}

export function listDirtyPaths(cwd) {
  const output = runGit(cwd, ['status', '--porcelain=v1', '-z', '--untracked-files=all']).stdout;
  const records = output.split('\0');
  const paths = [];
  for (let index = 0; index < records.length; index += 1) {
    const record = records[index];
    if (!record) continue;
    const status = record.slice(0, 2);
    paths.push(record.slice(3));
    if (/[RC]/.test(status) && records[index + 1]) {
      paths.push(records[index + 1]);
      index += 1;
    }
  }
  return [...new Set(paths)];
}

export function assertMarkdownOnly(paths) {
  const unsupported = paths.filter((value) => !isAllowedMarkdownPath(value));
  if (unsupported.length > 0) {
    throw new Error(
      `Cloud submission supports Markdown only; unsupported files: ${unsupported.join(', ')}`,
    );
  }
}

export function commitMarkdown(cwd, message, credential) {
  const title = message?.trim();
  if (!title) throw new Error('A commit message is required');
  const dirty = listDirtyPaths(cwd);
  if (dirty.length === 0) return false;
  assertMarkdownOnly(dirty);
  runGit(cwd, ['add', '-A', '--', ...dirty]);
  const result = runGit(cwd, [
    '-c',
    `user.name=${credential.userName}`,
    '-c',
    `user.email=${credential.userId}@users.cowiki.app`,
    'commit',
    '-m',
    title,
  ]);
  return result.status === 0;
}

export function setupCowikiRemote(cwd, space, credential, { bootstrap = false } = {}) {
  if (space.userRef !== `user/${credential.userId}`) {
    throw new Error('Cloud returned a user branch for another account');
  }
  const existing = runGit(cwd, ['remote', 'get-url', 'cowiki'], { allowFailure: true });
  if (existing.status === 0 && existing.stdout.trim() !== space.gitUrl) {
    throw new Error("Git remote 'cowiki' points to a different Cloud Space");
  }
  if (existing.status !== 0) runGit(cwd, ['remote', 'add', 'cowiki', space.gitUrl]);
  ignoreCloudConfig(cwd);
  runGit(cwd, ['config', '--unset-all', 'remote.cowiki.fetch'], { allowFailure: true });
  runGit(cwd, [
    'config',
    '--add',
    'remote.cowiki.fetch',
    '+refs/heads/main:refs/remotes/cowiki/main',
  ]);
  const main = runGit(cwd, ['ls-remote', '--exit-code', 'cowiki', 'refs/heads/main'], {
    apiKey: credential.apiKey,
    allowFailure: true,
  });
  if (main.status === 2) {
    if (!bootstrap) {
      throw new Error('Cloud main is empty; use cowiki publish to create its first revision');
    }
    bootstrapCloudSpace(cwd, space, credential);
  } else if (main.status !== 0) {
    throw new Error(main.stderr.trim() || 'Could not inspect Cloud main');
  }
  fetchCloudRefs(cwd, space, credential);
}

export function ignoreCloudConfig(cwd) {
  const raw = runGit(cwd, ['rev-parse', '--git-path', 'info/exclude']).stdout.trim();
  const filename = path.isAbsolute(raw) ? raw : path.join(cwd, raw);
  mkdirSync(path.dirname(filename), { recursive: true });
  const existing = existsSync(filename) ? readFileSync(filename, 'utf8') : '';
  const rule = '.cowiki/cloud.json';
  if (!existing.split(/\r?\n/).includes(rule)) {
    appendFileSync(filename, `${existing && !existing.endsWith('\n') ? '\n' : ''}${rule}\n`, 'utf8');
  }
}

export async function submitRepository({
  cwd,
  message,
  body = '',
  space,
  credential,
  cloud,
}) {
  ensureMain(cwd);
  ensureNoRebase(cwd);
  const committed = commitMarkdown(cwd, message, credential);
  fetchCloudRefs(cwd, space, credential);
  const rebase = runGit(cwd, ['rebase', 'refs/remotes/cowiki/main'], { allowFailure: true });
  if (rebase.status !== 0) {
    const conflicts = runGit(cwd, ['diff', '--name-only', '--diff-filter=U'], {
      allowFailure: true,
    }).stdout.trim();
    throw new Error(
      conflicts
        ? `Rebase stopped with conflicts in: ${conflicts.split('\n').join(', ')}`
        : rebase.stderr.trim() || 'Could not rebase onto Cloud main',
    );
  }
  const tracking = `refs/remotes/cowiki/${space.userRef}`;
  const tracked = runGit(cwd, ['rev-parse', '--verify', tracking], { allowFailure: true });
  const expected = tracked.status === 0 ? tracked.stdout.trim() : '';
  runGit(cwd, [
    'push',
    `--force-with-lease=refs/heads/${space.userRef}:${expected}`,
    'cowiki',
    `HEAD:refs/heads/${space.userRef}`,
  ], { apiKey: credential.apiKey });
  fetchCloudRefs(cwd, space, credential);
  const title = message?.trim() || runGit(cwd, ['log', '-1', '--pretty=%s']).stdout.trim();
  const pullRequest = await cloud.createOrUpdatePullRequest(space.id, title, body.trim());
  return { committed, pullRequest };
}

function fetchCloudRefs(cwd, space, credential) {
  runGit(cwd, [
    'fetch',
    '--prune',
    'cowiki',
    '+refs/heads/main:refs/remotes/cowiki/main',
  ], { apiKey: credential.apiKey });
  runGit(cwd, [
    'fetch',
    'cowiki',
    `+refs/heads/${space.userRef}:refs/remotes/cowiki/${space.userRef}`,
  ], { apiKey: credential.apiKey, allowFailure: true });
}

function bootstrapCloudSpace(cwd, space, credential) {
  if (space.role !== 'owner') {
    throw new Error('Cloud main is empty; only the Space owner can publish the first revision');
  }
  ensureMain(cwd);
  ensureNoRebase(cwd);
  const dirty = listDirtyPaths(cwd);
  if (dirty.length > 0) {
    throw new Error('Commit or discard local changes before publishing the first Cloud revision');
  }
  const tracked = runGit(cwd, ['ls-tree', '-r', '--name-only', '-z', 'HEAD'])
    .stdout
    .split('\0')
    .filter(Boolean);
  assertMarkdownOnly(tracked);
  runGit(cwd, [
    'push',
    '--atomic',
    'cowiki',
    'HEAD:refs/heads/main',
    `HEAD:refs/heads/${space.userRef}`,
  ], { apiKey: credential.apiKey });
}

export function repositoryStatus(cwd) {
  ensureMain(cwd);
  const dirty = listDirtyPaths(cwd);
  const head = runGit(cwd, ['rev-parse', 'HEAD']).stdout.trim();
  const cloudMain = runGit(cwd, ['rev-parse', '--verify', 'refs/remotes/cowiki/main'], {
    allowFailure: true,
  });
  return {
    branch: 'main',
    dirty,
    head,
    cloudMain: cloudMain.status === 0 ? cloudMain.stdout.trim() : null,
  };
}

function ensureMain(cwd) {
  const branch = runGit(cwd, ['branch', '--show-current']).stdout.trim();
  if (branch !== 'main') {
    throw new Error('Cloud submission requires the local main branch');
  }
}

function ensureNoRebase(cwd) {
  const gitDir = runGit(cwd, ['rev-parse', '--git-dir']).stdout.trim();
  const absolute = path.isAbsolute(gitDir) ? gitDir : path.join(cwd, gitDir);
  if (existsSync(path.join(absolute, 'rebase-merge'))
      || existsSync(path.join(absolute, 'rebase-apply'))) {
    throw new Error('A Git rebase is already in progress; resolve or abort it before submitting');
  }
}

function isAllowedMarkdownPath(value) {
  const normalized = value.replaceAll('\\', '/');
  if (!normalized.toLowerCase().endsWith('.md')) return false;
  const parts = normalized.split('/');
  if (parts.some((part) => !part || part === '.' || part === '..')) return false;
  if (!parts.some((part) => part.startsWith('.'))) return true;
  return parts[0] === '.cowiki'
    && parts[1] === 'sources'
    && parts.slice(2).every((part) => !part.startsWith('.'));
}
