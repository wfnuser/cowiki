#!/usr/bin/env node

import path from 'node:path';

import { CloudClient } from './lib/cloud.mjs';
import {
  normalizeServerOrigin,
  readCredential,
  readSpaceConfig,
  writeCredential,
  writeSpaceConfig,
} from './lib/config.mjs';
import {
  repositoryStatus,
  runGit,
  setupCowikiRemote,
  submitRepository,
} from './lib/git.mjs';
import { loginWithBrowser } from './lib/oauth.mjs';

const DEFAULT_SERVER = process.env.COWIKI_CLOUD_URL || 'https://cloud.cowiki.app';

export async function main(argv = process.argv.slice(2)) {
  const [command, ...rest] = argv;
  if (!command || ['--help', '-h', 'help'].includes(command)) {
    printHelp();
    return;
  }
  const options = parseOptions(rest);
  switch (command) {
    case 'login':
      await loginCommand(options);
      return;
    case 'setup':
      await setupCommand(options);
      return;
    case 'clone':
      await cloneCommand(options);
      return;
    case 'status':
      await statusCommand(options);
      return;
    case 'submit':
      await submitCommand(options);
      return;
    default:
      throw new Error(`Unknown command: ${command}`);
  }
}

async function loginCommand(options) {
  const server = serverOption(options);
  process.stdout.write(`Opening ${server} for GitHub sign-in…\n`);
  const credential = await loginWithBrowser({ server });
  const filename = await writeCredential(credential);
  process.stdout.write(`Signed in as ${credential.userName}. Credential saved to ${filename}.\n`);
}

async function setupCommand(options) {
  const cwd = path.resolve(options.cwd || process.cwd());
  const server = serverOption(options);
  const spaceId = requiredOption(options, 'space');
  const credential = await credentialFor(server);
  const cloud = new CloudClient(credential);
  const remote = await cloud.getSpace(spaceId);
  setupCowikiRemote(cwd, remote, credential);
  await writeSpaceConfig(cwd, {
    server,
    spaceId: remote.id,
    gitUrl: remote.gitUrl,
    userRef: remote.userRef,
  });
  process.stdout.write(`Linked ${remote.name} to ${cwd}.\n`);
}

async function cloneCommand(options) {
  const server = serverOption(options);
  const spaceId = requiredOption(options, 'space');
  const credential = await credentialFor(server);
  const cloud = new CloudClient(credential);
  const remote = await cloud.getSpace(spaceId);
  const destination = path.resolve(options.directory || remote.slug);
  runGit(process.cwd(), [
    'clone',
    '--origin',
    'cowiki',
    '--branch',
    'main',
    remote.gitUrl,
    destination,
  ], { apiKey: credential.apiKey });
  setupCowikiRemote(destination, remote, credential);
  await writeSpaceConfig(destination, {
    server,
    spaceId: remote.id,
    gitUrl: remote.gitUrl,
    userRef: remote.userRef,
  });
  process.stdout.write(`Cloned ${remote.name} to ${destination}.\n`);
}

async function statusCommand(options) {
  const cwd = path.resolve(options.cwd || process.cwd());
  const space = await readSpaceConfig(cwd);
  const credential = await requiredCredential(space.server);
  const status = repositoryStatus(cwd);
  process.stdout.write(`${JSON.stringify({
    server: space.server,
    spaceId: space.spaceId,
    userId: credential.userId,
    ...status,
  }, null, 2)}\n`);
}

async function submitCommand(options) {
  const cwd = path.resolve(options.cwd || process.cwd());
  const message = requiredOption(options, 'message');
  const spaceConfig = await readSpaceConfig(cwd);
  const credential = await requiredCredential(spaceConfig.server);
  const cloud = new CloudClient(credential);
  const remote = await cloud.getSpace(spaceConfig.spaceId);
  if (remote.userRef !== spaceConfig.userRef || remote.gitUrl !== spaceConfig.gitUrl) {
    throw new Error('Stored Space link no longer matches Cloud; run setup again');
  }
  const result = await submitRepository({
    cwd,
    message,
    body: options.body || '',
    space: remote,
    credential,
    cloud,
  });
  process.stdout.write(
    `Submitted pull request #${result.pullRequest.number}: ${result.pullRequest.title}\n`,
  );
}

async function credentialFor(server) {
  const existing = await readCredential(server);
  if (existing) return existing;
  process.stdout.write('No credential found; opening browser sign-in…\n');
  const credential = await loginWithBrowser({ server });
  await writeCredential(credential);
  return credential;
}

async function requiredCredential(server) {
  const credential = await readCredential(server);
  if (!credential) throw new Error(`Not signed in to ${server}; run cowiki login --server ${server}`);
  return credential;
}

function serverOption(options) {
  return normalizeServerOrigin(options.server || DEFAULT_SERVER);
}

function parseOptions(args) {
  const options = {};
  for (let index = 0; index < args.length; index += 1) {
    const value = args[index];
    if (!value.startsWith('--')) throw new Error(`Unexpected argument: ${value}`);
    const key = value.slice(2);
    const next = args[index + 1];
    if (!next || next.startsWith('--')) throw new Error(`Missing value for --${key}`);
    options[key === 'm' ? 'message' : key] = next;
    index += 1;
  }
  return options;
}

function requiredOption(options, name) {
  const value = options[name]?.trim();
  if (!value) throw new Error(`--${name} is required`);
  return value;
}

function printHelp() {
  process.stdout.write(`CoWiki local-first Cloud command

Usage:
  cowiki login [--server URL]
  cowiki setup --space UUID [--server URL] [--cwd PATH]
  cowiki clone --space UUID [--server URL] [--directory PATH]
  cowiki status [--cwd PATH]
  cowiki submit --message TEXT [--body TEXT] [--cwd PATH]
`);
}

function redactSecrets(value) {
  return String(value).replace(/cw_(?:key|once|invite)_[A-Za-z0-9_-]+/g, 'cw_[redacted]');
}

if (import.meta.url === `file://${process.argv[1]}`) {
  main().catch((error) => {
    process.stderr.write(`CoWiki: ${redactSecrets(error?.message || error)}\n`);
    process.exitCode = 1;
  });
}
