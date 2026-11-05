#!/usr/bin/env node
// Deterministic Agent Client Protocol (v1) agent over stdio, shaped like
// `grok agent stdio` (Grok Build 1.0.67). No network,
// no credentials. Usage: node fake-acp-agent.mjs <scenario> [agent args]
// Every message received is appended to $FAKE_LOG as one JSON line.
import { appendFileSync } from 'node:fs';
import { createInterface } from 'node:readline';

const scenario = process.argv[2] ?? 'normal';
const log = process.env.FAKE_LOG;
let nextRequest = 500;
const waiting = new Map();
const sessions = new Set(['acp-saved']);
let cancelled = false;

const send = (message) => process.stdout.write(`${JSON.stringify({ jsonrpc: '2.0', ...message })}\n`);
const record = (message) => { if (log) appendFileSync(log, `${JSON.stringify(message)}\n`); };
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const update = (sessionId, value) => send({ method: 'session/update', params: { sessionId, update: value } });
const ask = (method, params) => new Promise((resolve) => {
  const id = nextRequest++;
  waiting.set(id, resolve);
  send({ id, method, params });
});
const modes = (current) => ({
  currentModeId: current,
  availableModes: [{ id: 'default', name: 'Default' }, { id: 'bypassPermissions', name: 'Bypass' }],
});

async function prompt(id, params) {
  const { sessionId } = params;
  cancelled = false;
  update(sessionId, { sessionUpdate: 'agent_thought_chunk', content: { type: 'text', text: 'Considering' } });
  if (scenario === 'cancel') {
    while (!cancelled) {
      update(sessionId, { sessionUpdate: 'agent_message_chunk', content: { type: 'text', text: '.' } });
      await sleep(30);
    }
    return send({ id, result: { stopReason: 'cancelled' } });
  }
  if (scenario === 'fs-request') {
    await ask('fs/read_text_file', { sessionId, path: '/etc/hosts' });
  }
  if (scenario === 'permission') {
    update(sessionId, { sessionUpdate: 'tool_call', toolCallId: 'call-1', title: 'touch notes.md', kind: 'execute', status: 'pending', rawInput: { command: 'touch notes.md' } });
    const answer = await ask('session/request_permission', {
      sessionId,
      toolCall: { toolCallId: 'call-1', title: 'touch notes.md', kind: 'execute', rawInput: { command: 'touch notes.md' }, locations: [{ path: 'notes.md' }] },
      options: [
        { optionId: 'always', name: 'Always allow', kind: 'allow_always' },
        { optionId: 'once', name: 'Allow', kind: 'allow_once' },
        { optionId: 'reject', name: 'Reject', kind: 'reject_once' },
      ],
    });
    const allowed = answer.result?.outcome?.optionId === 'once';
    update(sessionId, { sessionUpdate: 'tool_call_update', toolCallId: 'call-1', status: allowed ? 'completed' : 'failed', content: [{ type: 'content', content: { type: 'text', text: allowed ? 'created' : 'denied' } }] });
  }
  update(sessionId, { sessionUpdate: 'agent_message_chunk', content: { type: 'text', text: 'Hello ' } });
  update(sessionId, { sessionUpdate: 'agent_message_chunk', content: { type: 'text', text: 'from ACP' } });
  update(sessionId, { sessionUpdate: 'tool_call', toolCallId: 'call-read', title: 'Read README.md', kind: 'read', status: 'in_progress' });
  update(sessionId, { sessionUpdate: 'tool_call_update', toolCallId: 'call-read', status: 'completed', content: [{ type: 'content', content: { type: 'text', text: '# Readme' } }] });
  update(sessionId, { sessionUpdate: 'plan', entries: [{ content: 'Read the note', status: 'completed', priority: 'high' }] });
  send({ id, result: { stopReason: 'end_turn' } });
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
      return send({ id, result: {
        protocolVersion: scenario === 'old-protocol' ? 0 : 1,
        agentCapabilities: { loadSession: true, sessionCapabilities: { resume: {} } },
        authMethods: [{ id: 'fake-login', name: 'Fake' }],
        _meta: { modelState: { currentModelId: 'fake-model' } },
      } });
    case 'session/new':
      if (scenario === 'auth') return send({ id, error: { code: -32000, message: 'Authentication required', data: 'no auth method id provided' } });
      sessions.add('acp-1');
      return send({ id, result: { sessionId: 'acp-1', modes: modes(scenario === 'bypass' ? 'bypassPermissions' : 'default') } });
    case 'session/resume':
      if (!sessions.has(params.sessionId)) return send({ id, error: { code: -32002, message: 'Resource not found' } });
      return send({ id, result: { modes: modes('default') } });
    case 'session/set_mode':
      return send({ id, result: {} });
    case 'session/prompt':
      return prompt(id, params);
    case 'session/cancel':
      cancelled = true;
      return undefined;
    case 'authenticate':
      return send({ id, result: {} });
    default:
      if (id !== undefined) return send({ id, error: { code: -32601, message: `fake does not implement ${method}` } });
      return undefined;
  }
});
lines.on('close', () => process.exit(0));
