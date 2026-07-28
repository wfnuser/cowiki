import { chmod, mkdir, readFile, rename, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';

export function normalizeServerOrigin(value) {
  let url;
  try {
    url = new URL(value);
  } catch {
    throw new Error('Cloud server must be a valid URL');
  }
  if (!['http:', 'https:'].includes(url.protocol)
      || url.username
      || url.password
      || !url.hostname
      || !['', '/'].includes(url.pathname)
      || url.search
      || url.hash) {
    throw new Error('Cloud server must be a root HTTP(S) origin without credentials');
  }
  return url.origin;
}

export function credentialPath({
  env = process.env,
  platform = process.platform,
  home = os.homedir(),
} = {}) {
  if (platform === 'win32') {
    const root = env.APPDATA || path.join(home, 'AppData', 'Roaming');
    return path.join(root, 'CoWiki', 'credentials.json');
  }
  if (platform === 'darwin') {
    return path.join(home, 'Library', 'Application Support', 'CoWiki', 'credentials.json');
  }
  return path.join(env.XDG_CONFIG_HOME || path.join(home, '.config'), 'cowiki', 'credentials.json');
}

export async function writeCredential(credential, options = {}) {
  const server = normalizeServerOrigin(credential.server);
  if (!credential.apiKey?.startsWith('cw_key_') || !credential.userId || !credential.userName) {
    throw new Error('Cloud returned an invalid credential');
  }
  const filename = credentialPath(options);
  await mkdir(path.dirname(filename), { recursive: true, mode: 0o700 });
  const temporary = `${filename}.${process.pid}.tmp`;
  await writeFile(temporary, `${JSON.stringify({ ...credential, server }, null, 2)}\n`, {
    encoding: 'utf8',
    mode: 0o600,
  });
  await chmod(temporary, 0o600).catch(() => {});
  await rename(temporary, filename);
  await chmod(filename, 0o600).catch(() => {});
  return filename;
}

export async function readCredential(server, options = {}) {
  const filename = credentialPath(options);
  let payload;
  try {
    payload = JSON.parse(await readFile(filename, 'utf8'));
  } catch (error) {
    if (error?.code === 'ENOENT') return null;
    throw new Error(`Could not read CoWiki credential: ${error.message}`);
  }
  const expected = normalizeServerOrigin(server);
  if (normalizeServerOrigin(payload.server) !== expected) return null;
  if (!payload.apiKey?.startsWith('cw_key_') || !payload.userId || !payload.userName) {
    throw new Error('Stored CoWiki credential is invalid; run login again');
  }
  return {
    server: expected,
    apiKey: payload.apiKey,
    userId: payload.userId,
    userName: payload.userName,
  };
}

export function spaceConfigPath(cwd = process.cwd()) {
  return path.join(cwd, '.cowiki', 'cloud.json');
}

export async function writeSpaceConfig(cwd, space) {
  const filename = spaceConfigPath(cwd);
  await mkdir(path.dirname(filename), { recursive: true });
  const value = {
    server: normalizeServerOrigin(space.server),
    spaceId: space.spaceId,
    gitUrl: space.gitUrl,
    userRef: space.userRef,
  };
  await writeFile(filename, `${JSON.stringify(value, null, 2)}\n`, 'utf8');
  return filename;
}

export async function readSpaceConfig(cwd = process.cwd()) {
  let payload;
  try {
    payload = JSON.parse(await readFile(spaceConfigPath(cwd), 'utf8'));
  } catch (error) {
    if (error?.code === 'ENOENT') {
      throw new Error('This repository is not linked to CoWiki Cloud; run setup first');
    }
    throw new Error(`Could not read .cowiki/cloud.json: ${error.message}`);
  }
  if (!payload.spaceId || !payload.gitUrl || !payload.userRef) {
    throw new Error('.cowiki/cloud.json is invalid; run setup again');
  }
  return {
    server: normalizeServerOrigin(payload.server),
    spaceId: payload.spaceId,
    gitUrl: payload.gitUrl,
    userRef: payload.userRef,
  };
}
