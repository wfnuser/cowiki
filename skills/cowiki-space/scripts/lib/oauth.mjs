import { spawn } from 'node:child_process';
import http from 'node:http';

import { normalizeServerOrigin } from './config.mjs';

export async function loginWithBrowser({
  server,
  fetchImpl = fetch,
  openBrowser = openSystemBrowser,
  timeoutMs = 120_000,
}) {
  const origin = normalizeServerOrigin(server);
  let complete;
  let fail;
  const callbackResult = new Promise((resolve, reject) => {
    complete = resolve;
    fail = reject;
  });
  const listener = http.createServer((request, response) => {
    try {
      const url = new URL(request.url, 'http://127.0.0.1');
      if (url.pathname !== '/auth/callback') {
        response.writeHead(404).end('Not found');
        return;
      }
      const code = url.searchParams.get('code');
      if (!code) throw new Error('Cloud callback did not include a code');
      response.writeHead(200, { 'Content-Type': 'text/plain; charset=utf-8' });
      response.end('CoWiki sign-in complete. You can return to your terminal.');
      complete(code);
    } catch (error) {
      response.writeHead(400).end('CoWiki sign-in failed.');
      fail(error);
    }
  });
  await new Promise((resolve, reject) => {
    listener.once('error', reject);
    listener.listen(0, '127.0.0.1', resolve);
  });
  const address = listener.address();
  const callback = `http://127.0.0.1:${address.port}/auth/callback`;
  const loginUrl = new URL('/api/auth/github', origin);
  loginUrl.searchParams.set('client', 'cli');
  loginUrl.searchParams.set('callback', callback);
  await openBrowser(loginUrl.toString());
  const timer = setTimeout(() => fail(new Error('Cloud sign-in timed out')), timeoutMs);
  try {
    const code = await callbackResult;
    const response = await fetchImpl(`${origin}/api/auth/exchange`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', Accept: 'application/json' },
      body: JSON.stringify({ code, client: 'cli' }),
    });
    if (!response.ok) {
      const payload = await response.json().catch(() => null);
      throw new Error(payload?.error || `Cloud sign-in failed (${response.status})`);
    }
    const payload = await response.json();
    return {
      server: origin,
      apiKey: payload.apiKey,
      userId: payload.userId,
      userName: payload.userName,
    };
  } finally {
    clearTimeout(timer);
    await new Promise((resolve) => listener.close(resolve));
  }
}

export function openSystemBrowser(url) {
  return new Promise((resolve, reject) => {
    let command;
    let args;
    if (process.platform === 'darwin') {
      command = 'open';
      args = [url];
    } else if (process.platform === 'win32') {
      command = 'rundll32';
      args = ['url.dll,FileProtocolHandler', url];
    } else {
      command = 'xdg-open';
      args = [url];
    }
    const child = spawn(command, args, { detached: true, stdio: 'ignore' });
    child.once('error', reject);
    child.once('spawn', () => {
      child.unref();
      resolve();
    });
  });
}
