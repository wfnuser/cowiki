import assert from 'node:assert/strict';
import test from 'node:test';

import {
  acceptSnapshot,
  applyEvent,
  canResume,
  canSend,
  mergeListed,
  orderedSessions,
  statusLabel,
  type AgentSnapshot,
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
