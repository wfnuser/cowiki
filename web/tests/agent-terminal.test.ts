import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
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
  openOrActivateAgentChangeTab,
  terminalTabLabel,
  type AgentTerminalTabsState,
} from '../src/components/terminal/terminal-tabs.ts';
import {
  agentMergeResult,
  localReviewActionRefreshesDraft,
  localReviewSelectionForRow,
  orderedLocalReviewRows,
} from '../src/components/review/local-review-model.ts';
import {
  parseReviewRoute,
  reviewRoute,
} from '../src/components/review/review-navigation.ts';

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

test('continuing an Agent Change activates its existing tab instead of duplicating it', () => {
  const initial: AgentTerminalTabsState = {
    activeTabId: 'live-1',
    tabs: [
      { id: 'live-1', agent: 'codex', mode: 'live' },
      {
        id: 'change-tab',
        agent: 'claude',
        mode: 'background',
        changeId: 'change-1',
        worktreePath: '/managed/change-1',
      },
    ],
  };

  assert.deepEqual(
    openOrActivateAgentChangeTab(initial, 'codex', 'new-tab', {
      changeId: 'change-1',
      worktreePath: '/managed/change-1',
    }),
    { ...initial, activeTabId: 'change-tab' },
  );
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

test('local Review rows navigate to dedicated Draft and Agent Change details', () => {
  assert.deepEqual(
    localReviewSelectionForRow({ kind: 'draft', id: 'current-draft' }),
    { kind: 'local-draft' },
  );
  assert.deepEqual(
    localReviewSelectionForRow({
      kind: 'agent',
      id: 'change-1',
      change: { id: 'change-1', createdAt: 20 },
    }),
    { kind: 'local-agent', changeId: 'change-1' },
  );
});

test('Review routes round-trip draft, Agent Change, and cloud detail targets', () => {
  const workspaces = ['general'];
  const targets = [
    { kind: 'local-draft' as const },
    { kind: 'local-agent' as const, changeId: 'change/with spaces' },
    { kind: 'cloud' as const, submissionId: 'submission-1' },
  ];

  for (const target of targets) {
    const path = reviewRoute('qinghao', 'general', target);
    assert.deepEqual(parseReviewRoute(path, workspaces), {
      workspaceSlug: 'general',
      target,
    });
  }
  assert.deepEqual(parseReviewRoute('/qinghao/general/reviews', workspaces), {
    workspaceSlug: 'general',
    target: null,
  });
  assert.equal(parseReviewRoute('/general/concepts/test', workspaces), null);
});

test('Agent merge feedback distinguishes a merged Draft from a conflict', () => {
  assert.deepEqual(agentMergeResult('merged'), {
    draftChanged: true,
    message: null,
  });
  assert.deepEqual(agentMergeResult('needsResolution'), {
    draftChanged: false,
    message: 'Merge needs resolution. Current Draft was left unchanged. Continue with Agent to resolve it.',
  });
});

test('the local Review inbox stays a list and renders diffs only in detail', () => {
  const inbox = readFileSync(
    new URL('../src/components/review/LocalReviewInbox.tsx', import.meta.url),
    'utf8',
  );
  const detail = readFileSync(
    new URL('../src/components/review/LocalReviewDetail.tsx', import.meta.url),
    'utf8',
  );

  assert.equal(inbox.includes("from './DiffView'"), false);
  assert.match(detail, /<DiffView diffs=\{diffs\}/);
  assert.match(detail, /Create Checkpoint/);
});
