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

test('maps the supported agents to their local CLI command', () => {
  assert.equal(agentInitialCommand('codex'), 'codex');
  assert.equal(agentInitialCommand('claude'), 'claude');
  assert.equal(agentInitialCommand('grok'), 'grok');
  assert.equal(agentInitialCommand('gemini'), 'gemini');
  assert.equal(agentInitialCommand('opencode'), 'opencode');
  assert.equal(agentInitialCommand('hermes'), 'hermes');
});

test('uses human-readable names for the selected default agent', () => {
  assert.equal(agentDisplayName('codex'), 'Codex');
  assert.equal(agentDisplayName('claude'), 'Claude Code');
  assert.equal(agentDisplayName('grok'), 'Grok');
  assert.equal(agentDisplayName('gemini'), 'Gemini CLI');
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
      { id: 'codex-1', agent: 'codex' },
      { id: 'codex-2', agent: 'codex' },
    ],
  });
  assert.equal(terminalTabLabel(second.tabs, second.tabs[0]), 'Codex 1');
  assert.equal(terminalTabLabel(second.tabs, second.tabs[1]), 'Codex 2');
});

test('closing the active tab selects its nearest remaining neighbor', () => {
  const state: AgentTerminalTabsState = {
    activeTabId: 'claude-2',
    tabs: [
      { id: 'codex-1', agent: 'codex' },
      { id: 'claude-2', agent: 'claude' },
      { id: 'codex-3', agent: 'codex' },
    ],
  };

  assert.deepEqual(closeAgentTab(state, 'claude-2'), {
    activeTabId: 'codex-3',
    tabs: [
      { id: 'codex-1', agent: 'codex' },
      { id: 'codex-3', agent: 'codex' },
    ],
  });
});

test('closing the final tab returns to the agent launch page', () => {
  assert.deepEqual(
    closeAgentTab({ activeTabId: 'codex-1', tabs: [{ id: 'codex-1', agent: 'codex' }] }, 'codex-1'),
    { activeTabId: null, tabs: [] },
  );
});
