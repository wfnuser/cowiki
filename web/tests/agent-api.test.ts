import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import { createAgentApi, type AgentTransport } from '../src/agent-api.ts';
import type { AgentEvent, AgentSnapshot, SessionMap } from '../src/lib/agent-session.ts';

const snapshot = (id: string, seq: number, spaceSlug = 'demo'): AgentSnapshot => ({
  sessionId: id,
  provider: 'codex',
  title: id,
  spaceSlug,
  mode: 'live',
  changeId: null,
  threadId: null,
  model: null,
  seq,
  status: 'idle',
  items: [],
  approvals: [],
  error: null,
  notice: null,
  closed: false,
  createdAt: seq,
  updatedAt: seq,
});

function fakeTransport() {
  const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
  const listeners = new Map<string, Set<(payload: unknown) => void>>();
  const order: string[] = [];
  let resolveList: ((value: AgentSnapshot[]) => void) | null = null;
  const transport: AgentTransport = {
    invoke: <T>(command: string, args?: Record<string, unknown>) => {
      calls.push({ command, args });
      order.push(`invoke:${command}`);
      if (command === 'agent_list_sessions') {
        return new Promise<T>((resolve) => { resolveList = resolve as (value: AgentSnapshot[]) => void; });
      }
      return Promise.resolve(undefined as T);
    },
    listen: async <T>(eventName: string, handler: (payload: T) => void) => {
      order.push(`listen:${eventName}`);
      const set = listeners.get(eventName) ?? new Set();
      set.add(handler as (payload: unknown) => void);
      listeners.set(eventName, set);
      return () => set.delete(handler as (payload: unknown) => void);
    },
  };
  return {
    transport,
    calls,
    order,
    emit: (payload: AgentEvent) => listeners.get('agent:event')?.forEach((listener) => listener(payload)),
    listenerCount: () => listeners.get('agent:event')?.size ?? 0,
    resolveList: (value: AgentSnapshot[]) => resolveList?.(value),
  };
}

const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
const event = (value: AgentSnapshot): AgentEvent => ({ sessionId: value.sessionId, seq: value.seq, snapshot: value });

test('listens before listing, and events during the list are not lost', async () => {
  const fake = fakeTransport();
  const api = createAgentApi(fake.transport);
  const seen: SessionMap[] = [];
  const subscription = api.subscribe('demo', (sessions) => seen.push(sessions));
  await tick();
  assert.deepEqual(fake.order.slice(0, 2), ['listen:agent:event', 'invoke:agent_list_sessions']);
  fake.emit(event(snapshot('a', 7)));
  fake.emit(event(snapshot('a', 7)));
  fake.emit(event(snapshot('x', 1, 'other')));
  fake.resolveList([snapshot('a', 3), snapshot('b', 2)]);
  await tick();
  const latest = seen.at(-1)!;
  assert.equal(latest.get('a')?.seq, 7, 'the older listed snapshot did not win');
  assert.equal(latest.get('b')?.seq, 2);
  assert.equal(latest.has('x'), false);
  subscription.dispose();
  await tick();
  assert.equal(fake.listenerCount(), 0);
});

test('subscribing never sends, stops or resumes anything', async () => {
  const fake = fakeTransport();
  const api = createAgentApi(fake.transport);
  const subscription = api.subscribe('demo', () => undefined);
  fake.resolveList([snapshot('a', 1)]);
  await tick();
  subscription.dispose();
  const commands = fake.calls.map((call) => call.command);
  assert.deepEqual(commands, ['agent_list_sessions']);
});

test('an empty Space still reports that it finished loading', async () => {
  const fake = fakeTransport();
  const api = createAgentApi(fake.transport);
  let reports = 0;
  api.subscribe('demo', () => { reports += 1; });
  await tick();
  fake.resolveList([]);
  await tick();
  assert.equal(reports, 1);
});

test('commands carry ids and text only, never directories', async () => {
  const fake = fakeTransport();
  const api = createAgentApi(fake.transport);
  await api.create({ spaceSlug: 'demo', mode: 'background', changeId: 'c1', agent: 'grok', title: 'T' });
  await api.send('agent-1', 'hello');
  await api.answer('agent-1', 'approval-1', 'deny');
  assert.deepEqual(fake.calls[0], {
    command: 'agent_create_session',
    args: { request: { spaceSlug: 'demo', mode: 'background', changeId: 'c1', agent: 'grok', title: 'T' } },
  });
  assert.deepEqual(fake.calls[1], { command: 'agent_send', args: { sessionId: 'agent-1', text: 'hello' } });
  assert.deepEqual(fake.calls[2], { command: 'agent_answer_approval', args: { sessionId: 'agent-1', approvalId: 'approval-1', decision: 'deny' } });
  assert.equal(JSON.stringify(fake.calls).includes('cwd'), false);
});

test('the sidebar no longer starts a terminal', () => {
  const layout = readFileSync(new URL('../src/pages/MainLayout.tsx', import.meta.url), 'utf8');
  const panel = readFileSync(new URL('../src/components/agent/AgentPanel.tsx', import.meta.url), 'utf8');
  assert.match(layout, /<AgentPanel\b/);
  assert.doesNotMatch(layout, /<AgentTerminalPanel\b/);
  for (const source of [panel, readFileSync(new URL('../src/agent-api.ts', import.meta.url), 'utf8')]) {
    assert.doesNotMatch(source, /terminal_create|@xterm/);
  }
  // Agent text is untrusted: no raw HTML rendering.
  const transcript = readFileSync(new URL('../src/components/agent/AgentTranscript.tsx', import.meta.url), 'utf8');
  assert.match(transcript, /skipHtml/);
  assert.doesNotMatch(transcript, /dangerouslySetInnerHTML|rehype-raw/);
});
