import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import test from 'node:test';

import {
  agentDisplayName,
  agentInitialCommand,
  agentReadinessAction,
  belongsToTerminalSession,
  normalizeTerminalSize,
} from '../src/components/terminal/terminal-contract.ts';
import {
  addAgentTab,
  addOrFocusCodexLoginTab,
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

test('routes signed-out Codex sessions through the single login flow', () => {
  assert.equal(agentReadinessAction('codex', 'available'), 'run');
  assert.equal(agentReadinessAction('codex', 'signedOut'), 'login');
  assert.equal(agentReadinessAction('codex', 'broken'), 'blocked');
  assert.equal(agentReadinessAction('claude', 'signedOut'), 'blocked');
});

test('the launch page uses one stateful primary action and no execution-mode promotion', () => {
  const panel = readFileSync(
    new URL('../src/components/terminal/AgentTerminalPanel.tsx', import.meta.url),
    'utf8',
  );

  assert.match(panel, /const buttonLabel = checking[\s\S]*Sign in to/);
  assert.match(panel, /<Button[\s\S]*disabled=\{busy \|\| \(!available && !signedOut\)\}/);
  assert.doesNotMatch(panel, /function ExecutionModeControl/);
  assert.doesNotMatch(panel, /agentTerminalModeDetails/);
});

test('the add menu keeps readiness checks local and prevents duplicate launches', () => {
  const panel = readFileSync(
    new URL('../src/components/terminal/AgentTerminalPanel.tsx', import.meta.url),
    'utf8',
  );

  assert.match(panel, /pendingLaunchesRef\.current\.has\(launchKey\)/);
  assert.match(panel, /event\.preventDefault\(\);[\s\S]*attemptOpen\(agent, 'live'\)/);
  assert.match(panel, /const attemptedFeedback = attemptedAgent \? feedback\[attemptedAgent\]/);
  assert.match(panel, /checking \|\| pending[\s\S]*LoaderCircle/);
  assert.match(panel, /tab\.intent === 'login'[\s\S]*checkAgent\(tab\.agent, true\)/);
});

test('agent probes and terminal startup run outside the Tauri command thread', () => {
  const terminal = readFileSync(
    new URL('../src-tauri/src/terminal.rs', import.meta.url),
    'utf8',
  );

  assert.match(terminal, /pub async fn agent_probe[\s\S]*spawn_blocking/);
  assert.match(terminal, /pub async fn terminal_create[\s\S]*spawn_blocking/);
  assert.match(terminal, /fn cached_readiness[\s\S]*Duration::from_secs\(30\)/);
  assert.match(terminal, /fn resolve_agent_lookup_shell/);
});

test('routes terminal events only to their owning session', () => {
  assert.equal(belongsToTerminalSession({ sessionId: 'terminal-a' }, 'terminal-a'), true);
  assert.equal(belongsToTerminalSession({ sessionId: 'terminal-b' }, 'terminal-a'), false);
  assert.equal(belongsToTerminalSession(null, 'terminal-a'), false);
});

test('collapsing the Agent panel hides it without unmounting terminal sessions', () => {
  const mainLayout = readFileSync(
    new URL('../src/pages/MainLayout.tsx', import.meta.url),
    'utf8',
  );
  assert.match(
    mainLayout,
    /\{desktop && activeWorkspace\?\.localPath && \(\s*<div hidden=\{!agentPanelOpen\}/,
  );
  assert.match(mainLayout, /hidden=\{!agentPanelOpen\}[\s\S]*<AgentTerminalPanel/);
  assert.doesNotMatch(
    mainLayout,
    /desktop && agentPanelOpen && activeWorkspace\?\.localPath/,
  );
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

test('Codex login tabs remain separate, unique, and live-only', () => {
  const empty: AgentTerminalTabsState = { activeTabId: null, tabs: [] };
  const login = addOrFocusCodexLoginTab(empty, 'login-1');

  assert.deepEqual(login.tabs, [
    { id: 'login-1', agent: 'codex', mode: 'live', intent: 'login' },
  ]);
  assert.equal(terminalTabLabel(login.tabs, login.tabs[0]), 'Codex · Sign in');
  assert.deepEqual(
    addOrFocusCodexLoginTab({
      activeTabId: 'claude-1',
      tabs: [
        ...login.tabs,
        { id: 'claude-1', agent: 'claude', mode: 'live' },
      ],
    }, 'login-2'),
    {
      activeTabId: 'login-1',
      tabs: [
        { id: 'login-1', agent: 'codex', mode: 'live', intent: 'login' },
        { id: 'claude-1', agent: 'claude', mode: 'live' },
      ],
    },
  );
  assert.throws(
    () => addAgentTab(empty, 'codex', 'login-2', 'background', {
      changeId: 'change-2',
      worktreePath: '/managed/change-2',
    }, 'login'),
    /Only Codex/,
  );
  assert.throws(
    () => addAgentTab(empty, 'claude', 'login-3', 'live', undefined, 'login'),
    /Only Codex/,
  );
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

test('desktop review source branches use the public agent namespace', () => {
  const api = readFileSync(resolve(import.meta.dirname, '../src/api.ts'), 'utf8');
  assert.match(api, /source_branch: `agent\/\$\{change\.id\}`/);
  assert.doesNotMatch(api, /source_branch: `cowiki\/agent\//);
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
