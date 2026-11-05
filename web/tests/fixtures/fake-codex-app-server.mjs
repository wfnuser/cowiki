#!/usr/bin/env node
// Deterministic stand-in for `codex app-server --listen stdio://` (protocol
// shapes from codex-cli 0.166.0 `app-server generate-ts`). No network, no
// credentials. Usage: node fake-codex-app-server.mjs <scenario> [codex args]
// Every message received is appended to $FAKE_LOG as one JSON line.
import { appendFileSync } from 'node:fs';
import { createInterface } from 'node:readline';

const scenario = process.argv[2] ?? 'normal';
const log = process.env.FAKE_LOG;
const cwd = process.cwd();
let nextServerRequest = 1000;
let turnCount = 0;
let activeTurn = null;
const waiting = new Map();

const send = (message) => process.stdout.write(`${JSON.stringify(message)}\n`);
const notify = (method, params) => send({ method, params });
const record = (message) => { if (log) appendFileSync(log, `${JSON.stringify(message)}\n`); };
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const ask = (method, params) => new Promise((resolve) => {
  const id = nextServerRequest++;
  waiting.set(id, resolve);
  send({ id, method, params });
});

const threadId = 'thread-1';
const threadResponse = (sandbox = { type: 'workspaceWrite', writableRoots: [cwd], networkAccess: false }) => ({
  thread: { id: threadId, sessionId: 's', preview: '' },
  model: 'gpt-fake',
  modelProvider: 'openai',
  cwd,
  approvalPolicy: scenario === 'never-ask' ? 'never' : 'on-request',
  sandbox: scenario === 'full-access' ? { type: 'dangerFullAccess' } : sandbox,
});

function completeTurn(turnId, status = 'completed', error = null) {
  if (activeTurn !== turnId) return;
  activeTurn = null;
  notify('turn/completed', { threadId, turn: { id: turnId, status, error, items: [] } });
}

async function runTurn(turnId, text) {
  if (scenario === 'start-delay') await sleep(300);
  activeTurn = turnId;
  notify('turn/started', { threadId, turn: { id: turnId, status: 'inProgress', items: [] } });
  if (scenario === 'crash') {
    process.stderr.write('fake codex: boom\n');
    process.exit(3);
  }
  if (scenario === 'oversize') process.stdout.write(`${'x'.repeat(9 * 1024 * 1024)}\n`);
  if (scenario === 'long' || scenario === 'start-delay') {
    notify('item/started', { threadId, turnId, item: { type: 'agentMessage', id: 'msg-long', text: '' } });
    while (activeTurn === turnId) {
      notify('item/agentMessage/delta', { threadId, turnId, itemId: 'msg-long', delta: '.' });
      await sleep(30);
    }
    return;
  }
  // Another thread's events must not reach this session.
  notify('item/agentMessage/delta', { threadId: 'other-thread', turnId: 'x', itemId: 'foreign', delta: 'FOREIGN' });
  if (scenario === 'approval') {
    notify('item/started', { threadId, turnId, item: { type: 'commandExecution', id: 'cmd-1', command: 'rm -rf build', cwd, status: 'inProgress' } });
    const answer = await ask('item/commandExecution/requestApproval', { threadId, turnId, itemId: 'cmd-1', command: 'rm -rf build', cwd, reason: 'clean the build folder' });
    const accepted = answer.result?.decision === 'accept';
    notify('item/completed', { threadId, turnId, item: { type: 'commandExecution', id: 'cmd-1', command: 'rm -rf build', cwd, status: accepted ? 'completed' : 'declined', aggregatedOutput: accepted ? 'removed' : '' } });
    notify('serverRequest/resolved', { threadId, requestId: answer.id });
  }
  if (scenario === 'unknown-request') {
    await ask('item/permissions/requestApproval', { threadId, turnId, itemId: 'p', cwd, reason: 'more', permissions: { network: true } });
    await ask('item/tool/requestUserInput', { threadId, turnId, itemId: 'q', questions: [] });
    await ask('item/fileChange/requestApproval', { threadId, turnId, itemId: 'f', grantRoot: '/' });
  }
  notify('item/started', { threadId, turnId, item: { type: 'agentMessage', id: 'msg-1', text: '' } });
  for (const delta of ['Hello ', 'from ', 'Codex']) notify('item/agentMessage/delta', { threadId, turnId, itemId: 'msg-1', delta });
  notify('item/completed', { threadId, turnId, item: { type: 'agentMessage', id: 'msg-1', text: `Hello from Codex (${text})` } });
  notify('item/started', { threadId, turnId, item: { type: 'commandExecution', id: 'cmd-ls', command: 'ls', cwd, status: 'inProgress' } });
  notify('item/commandExecution/outputDelta', { threadId, turnId, itemId: 'cmd-ls', delta: 'README.md\n' });
  notify('item/completed', { threadId, turnId, item: { type: 'commandExecution', id: 'cmd-ls', command: 'ls', cwd, status: 'completed', aggregatedOutput: 'README.md\n', exitCode: 0 } });
  completeTurn(turnId);
}

const lines = createInterface({ input: process.stdin });
lines.on('line', async (line) => {
  let message;
  try { message = JSON.parse(line); } catch { return; }
  record(message);
  if (message.method === undefined && message.id !== undefined) {
    waiting.get(message.id)?.(message);
    waiting.delete(message.id);
    return;
  }
  const { id, method, params = {} } = message;
  switch (method) {
    case 'initialize':
      return send({ id, result: { userAgent: 'fake-codex/0.166.0' } });
    case 'initialized':
      return undefined;
    case 'account/read':
      return send({ id, result: scenario === 'signed-out'
        ? { account: null, requiresOpenaiAuth: true }
        : { account: { type: 'chatgpt', email: 'tester@example.com', planType: 'plus' }, requiresOpenaiAuth: true } });
    case 'thread/start':
      return send({ id, result: threadResponse() });
    case 'thread/resume':
      if (params.threadId !== threadId) return send({ id, error: { code: -32600, message: `thread not found: ${params.threadId}` } });
      return send({ id, result: threadResponse() });
    case 'turn/start': {
      turnCount += 1;
      const turnId = `turn-${turnCount}`;
      const text = params.input?.[0]?.text ?? '';
      if (scenario === 'timeout') return undefined;
      if (scenario === 'early-complete') {
        activeTurn = turnId;
        notify('turn/started', { threadId, turn: { id: turnId, status: 'inProgress', items: [] } });
        notify('item/completed', { threadId, turnId, item: { type: 'agentMessage', id: 'msg-early', text: 'done early' } });
        completeTurn(turnId);
        return send({ id, result: { turn: { id: turnId, status: 'inProgress', items: [] } } });
      }
      send({ id, result: { turn: { id: turnId, status: 'inProgress', items: [] } } });
      return runTurn(turnId, text);
    }
    case 'turn/interrupt':
      // Like codex-cli: no interrupt before turn/started names the turn.
      if (activeTurn !== params.turnId) return send({ id, error: { code: -32600, message: 'no active turn' } });
      send({ id, result: {} });
      return completeTurn(params.turnId, 'interrupted');
    default:
      return send({ id, error: { code: -32601, message: `fake does not implement ${method}` } });
  }
});
lines.on('close', () => process.exit(0));
