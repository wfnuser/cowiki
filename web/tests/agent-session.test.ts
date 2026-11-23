import assert from 'node:assert/strict';
import test from 'node:test';

import {
  acceptSnapshot,
  applyEvent,
  canResume,
  canSend,
  mergeListed,
  modelLabel,
  orderedSessions,
  sessionGroups,
  shortAgo,
  shortVersion,
  statusLabel,
  transcriptBlocks,
  workSummary,
  type AgentSnapshot,
  type AgentToolItem,
  type ModelOption,
  type SessionMap,
} from '../src/lib/agent-session.ts';

const initial = (id: string, overrides: Partial<AgentSnapshot> = {}): AgentSnapshot => ({
  sessionId: id,
  provider: 'codex',
  title: 'Task',
  spaceSlug: 'demo',
  mode: 'live',
  changeId: null,
  threadId: null,
  model: null,
  seq: 2,
  status: 'idle',
  items: [],
  approvals: [],
  error: null,
  notice: null,
  closed: false,
  createdAt: 1,
  updatedAt: 1,
  ...overrides,
});

const event = (snapshot: AgentSnapshot) => ({ sessionId: snapshot.sessionId, seq: snapshot.seq, snapshot });

test('rejects another session and older snapshots', () => {
  const a = initial('a');
  assert.equal(acceptSnapshot(a, { sessionId: 'b', seq: 3, snapshot: initial('b', { seq: 3 }) }), a);
  assert.equal(acceptSnapshot(a, { sessionId: 'a', seq: 1, snapshot: { ...a, seq: 1 } }), a);
  assert.equal(acceptSnapshot(a, event({ ...a })), a, 'a duplicate is ignored');
  const newer = { ...a, seq: 3, status: 'running' as const };
  assert.equal(acceptSnapshot(a, event(newer)), newer);
});

test('rejects envelopes whose fields disagree with the snapshot', () => {
  const a = initial('a');
  assert.equal(acceptSnapshot(a, { sessionId: 'a', seq: 9, snapshot: { ...a, seq: 3 } }), a);
  assert.equal(acceptSnapshot(null, { sessionId: 'a', seq: 3, snapshot: initial('b', { seq: 3 }) }), null);
});

test('a finished turn is never revived by a late event', () => {
  let sessions: SessionMap = new Map();
  const running = initial('a', { seq: 5, status: 'running' });
  const idle = initial('a', { seq: 6, status: 'idle' });
  sessions = applyEvent(sessions, 'demo', event(running));
  sessions = applyEvent(sessions, 'demo', event(idle));
  sessions = applyEvent(sessions, 'demo', event(running));
  assert.equal(sessions.get('a')?.status, 'idle');
});

test('events of another Space never enter this one', () => {
  const sessions = applyEvent(new Map(), 'demo', event(initial('x', { spaceSlug: 'other' })));
  assert.equal(sessions.size, 0);
});

test('a listed snapshot does not overwrite a newer event', () => {
  let sessions: SessionMap = applyEvent(new Map(), 'demo', event(initial('a', { seq: 8, status: 'running' })));
  sessions = mergeListed(sessions, 'demo', [initial('a', { seq: 4 }), initial('b', { seq: 1 })]);
  assert.equal(sessions.get('a')?.seq, 8);
  assert.equal(sessions.get('b')?.seq, 1);
});

test('orders sessions newest first and labels their state', () => {
  const sessions: SessionMap = new Map([
    ['old', initial('old', { updatedAt: 1 })],
    ['new', initial('new', { updatedAt: 5 })],
  ]);
  assert.deepEqual(orderedSessions(sessions).map((session) => session.sessionId), ['new', 'old']);
  assert.equal(statusLabel(initial('a', { status: 'waiting' })), 'Needs approval');
  assert.equal(statusLabel(initial('a', { closed: true })), 'Closed');
});

test('only idle, open sessions accept a message; interrupted ones can be continued', () => {
  assert.equal(canSend(initial('a')), true);
  assert.equal(canSend(initial('a', { status: 'running' })), false);
  assert.equal(canSend(initial('a', { status: 'stopping' })), false);
  assert.equal(canSend(initial('a', { closed: true })), false);
  assert.equal(canResume(initial('a', { status: 'interrupted', threadId: 't' })), true);
  assert.equal(canResume(initial('a', { status: 'interrupted' })), false, 'nothing to resume without a thread');
});

const step = (id: string, toolKind: AgentToolItem['toolKind'], status: AgentToolItem['status'] = 'completed'): AgentToolItem => ({
  type: 'tool', id, turnId: 't', name: id, toolKind, status, output: '', truncated: false,
});

test('consecutive tool steps fold into one block between messages', () => {
  const blocks = transcriptBlocks([
    { type: 'message', id: 'u', role: 'user', text: 'hi', turnId: null },
    step('a', 'think'),
    step('b', 'command'),
    { type: 'message', id: 'm', role: 'assistant', text: 'done', turnId: 't' },
    step('c', 'file'),
  ]);
  assert.deepEqual(blocks.map((block) => block.type === 'work' ? block.steps.map((item) => item.id).join('+') : block.item.id), ['u', 'a+b', 'm', 'c']);
});

test('a run of steps reads as one short summary', () => {
  assert.equal(workSummary([step('a', 'command'), step('b', 'command'), step('c', 'mcp'), step('d', 'think')]).text, '2 commands · 1 tool call');
  assert.equal(workSummary([step('a', 'think'), step('b', 'think')]).text, 'Thought 2 times');
  const summary = workSummary([step('a', 'command', 'failed'), step('b', 'file', 'declined')]);
  assert.deepEqual([summary.failed, summary.declined], [1, 1]);
  assert.equal(shortVersion('2.1.296 (Claude Code)'), '2.1.296');
  assert.equal(shortVersion('codex-cli 0.166.0'), '0.166.0');
  assert.equal(shortVersion(null), null);
});

test('a model reads by the name the Agent gave it, aliases by what they run', () => {
  const claude: ModelOption[] = [
    { id: 'default', name: 'Default', description: 'Opus · Best', isDefault: true, resolved: 'claude-opus' },
    { id: 'opus', name: 'Opus', description: null, isDefault: false, resolved: 'claude-opus' },
    { id: 'sonnet', name: 'Sonnet', description: null, isDefault: false, resolved: 'claude-sonnet' },
  ];
  assert.equal(modelLabel(claude, null), 'Opus', 'the default reads as the model it runs');
  assert.equal(modelLabel(claude, 'sonnet'), 'Sonnet');
  assert.equal(modelLabel(claude, null, 'claude-sonnet'), 'Sonnet', 'a reported concrete model maps back');
  assert.equal(modelLabel(claude, 'unknown-model'), 'unknown-model');
  assert.equal(modelLabel([], null, 'gpt-x'), 'gpt-x');
  assert.equal(modelLabel([], null), null);
});

test('sessions group by day of last activity, newest first', () => {
  const now = new Date(2026, 10, 23, 15, 0).getTime();
  const at = (days: number, hour = 10) => new Date(2026, 10, 23 - days, hour, 0).getTime();
  const session = (id: string, updatedAt: number) => ({ sessionId: id, updatedAt }) as AgentSnapshot;
  const groups = sessionGroups([session('old', at(30)), session('a', at(0, 9)), session('b', at(0, 14)), session('y', at(1)), session('w', at(4))], now);
  assert.deepEqual(groups.map((group) => [group.label, group.sessions.map((item) => item.sessionId)]), [
    ['Today', ['b', 'a']],
    ['Yesterday', ['y']],
    ['Previous 7 days', ['w']],
    ['Older', ['old']],
  ]);
  assert.equal(shortAgo(now - 30_000, now), 'now');
  assert.equal(shortAgo(now - 5 * 60_000, now), '5m');
  assert.equal(shortAgo(now - 3 * 3_600_000, now), '3h');
});
