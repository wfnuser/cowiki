import assert from 'node:assert/strict';
import test from 'node:test';

import {
  agentDisplayName,
  agentInitialCommand,
  belongsToTerminalSession,
  normalizeTerminalSize,
} from '../src/components/terminal/terminal-contract.ts';
import {
  addAgentTab,
  closeAgentTab,
  terminalTabLabel,
  type AgentTerminalTabsState,
} from '../src/components/terminal/terminal-tabs.ts';
import {
  localReviewActionRefreshesDraft,
  orderedLocalReviewRows,
} from '../src/components/review/local-review-model.ts';

test('maps the supported agents to their local CLI command', () => {
  assert.equal(agentInitialCommand('codex'), 'codex');
  assert.equal(agentInitialCommand('claude'), 'claude');
  assert.equal(agentInitialCommand('grok'), 'grok');
  assert.equal(agentInitialCommand('antigravity'), 'agy');
  assert.equal(agentInitialCommand('opencode'), 'opencode');
  assert.equal(agentInitialCommand('hermes'), 'hermes');
});

test('uses human-readable names for the selected default agent', () => {
  assert.equal(agentDisplayName('codex'), 'Codex');
  assert.equal(agentDisplayName('claude'), 'Claude Code');
  assert.equal(agentDisplayName('grok'), 'Grok');
  assert.equal(agentDisplayName('antigravity'), 'Antigravity CLI');
  assert.equal(agentDisplayName('opencode'), 'OpenCode');
  assert.equal(agentDisplayName('hermes'), 'Hermes Agent');
});

test('normalizes terminal dimensions to the PTY contract', () => {
  assert.deepEqual(normalizeTerminalSize(0, Number.NaN), { cols: 20, rows: 24 });
  assert.deepEqual(normalizeTerminalSize(900, 999), { cols: 500, rows: 200 });
  assert.deepEqual(normalizeTerminalSize(101.8, 42.9), { cols: 101, rows: 42 });
});

test('routes terminal events only to their owning session', () => {
  assert.equal(belongsToTerminalSession({ sessionId: 'terminal-a' }, 'terminal-a'), true);
  assert.equal(belongsToTerminalSession({ sessionId: 'terminal-b' }, 'terminal-a'), false);
  assert.equal(belongsToTerminalSession(null, 'terminal-a'), false);
});

test('opens multiple independent tabs for the same agent', () => {
  const empty: AgentTerminalTabsState = { activeTabId: null, tabs: [] };
  const first = addAgentTab(empty, 'codex', 'codex-1');
  const second = addAgentTab(first, 'codex', 'codex-2');

  assert.deepEqual(second, {
    activeTabId: 'codex-2',
    tabs: [
      { id: 'codex-1', agent: 'codex', mode: 'live' },
      { id: 'codex-2', agent: 'codex', mode: 'live' },
    ],
  });
  assert.equal(terminalTabLabel(second.tabs, second.tabs[0]), 'Codex 1');
  assert.equal(terminalTabLabel(second.tabs, second.tabs[1]), 'Codex 2');
});

test('closing the active tab selects its nearest remaining neighbor', () => {
  const state: AgentTerminalTabsState = {
    activeTabId: 'claude-2',
    tabs: [
      { id: 'codex-1', agent: 'codex', mode: 'live' },
      { id: 'claude-2', agent: 'claude', mode: 'live' },
      { id: 'codex-3', agent: 'codex', mode: 'live' },
    ],
  };

  assert.deepEqual(closeAgentTab(state, 'claude-2'), {
    activeTabId: 'codex-3',
    tabs: [
      { id: 'codex-1', agent: 'codex', mode: 'live' },
      { id: 'codex-3', agent: 'codex', mode: 'live' },
    ],
  });
});

test('closing the final tab returns to the agent launch page', () => {
  assert.deepEqual(
    closeAgentTab({
      activeTabId: 'codex-1',
      tabs: [{ id: 'codex-1', agent: 'codex', mode: 'live' }],
    }, 'codex-1'),
    { activeTabId: null, tabs: [] },
  );
});

test('agent tabs keep live and background execution identities separate', () => {
  const empty: AgentTerminalTabsState = { activeTabId: null, tabs: [] };
  const live = addAgentTab(empty, 'codex', 'live-1', 'live');
  const background = addAgentTab(live, 'codex', 'background-1', 'background', {
    changeId: 'change-1',
    worktreePath: '/managed/change-1',
  });

  assert.deepEqual(background.tabs, [
    { id: 'live-1', agent: 'codex', mode: 'live' },
    {
      id: 'background-1',
      agent: 'codex',
      mode: 'background',
      changeId: 'change-1',
      worktreePath: '/managed/change-1',
    },
  ]);
  assert.equal(terminalTabLabel(background.tabs, background.tabs[0]), 'Codex 1');
  assert.equal(terminalTabLabel(background.tabs, background.tabs[1]), 'Codex 2 · Background');
});

test('local Reviews always order Current Draft before Agent Changes', () => {
  assert.deepEqual(orderedLocalReviewRows([]), [{ kind: 'draft', id: 'current-draft' }]);
  assert.deepEqual(
    orderedLocalReviewRows([
      { id: 'older', createdAt: 10 },
      { id: 'newer', createdAt: 20 },
    ]),
    [
      { kind: 'draft', id: 'current-draft' },
      { kind: 'agent', id: 'newer', change: { id: 'newer', createdAt: 20 } },
      { kind: 'agent', id: 'older', change: { id: 'older', createdAt: 10 } },
    ],
  );
});

test('only merging an Agent Change refreshes the Draft navigation trees', () => {
  assert.equal(localReviewActionRefreshesDraft('merge'), true);
  assert.equal(localReviewActionRefreshesDraft('discard'), false);
  assert.equal(localReviewActionRefreshesDraft('commit'), false);
});
