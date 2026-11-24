import { expect, test, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { createServer, type ViteDevServer } from 'vite';
import type { AgentChange } from '../../src/api';
import type { GovernanceReview } from '../../src/components/review/governance-model';

// Real LocalEngine output from CMake development cases aeee390db252024e and
// 36d6acb0d77730d8, including the new worktree's diff. Only IPC is mocked.
const fixture: { change: AgentChange; staged: AgentChange; report: GovernanceReview } =
  JSON.parse(readFileSync(new URL('../fixtures/governance-cmake.json', import.meta.url), 'utf8'));
let vite: ViteDevServer;
let origin: string;
test.beforeAll(async () => {
  vite = await createServer({ root: fileURLToPath(new URL('../../', import.meta.url)), logLevel: 'silent', server: { host: '127.0.0.1', port: 0 } });
  await vite.listen();
  const port = vite.httpServer!.address();
  if (!port || typeof port === 'string') throw new Error('Missing test port');
  origin = `http://127.0.0.1:${port.port}`;
});
test.afterAll(async () => { await vite.close(); });

async function desktop(page: Page, { stale = false, report = fixture.report }: {
  stale?: boolean;
  report?: GovernanceReview;
} = {}) {
  const calls: { cmd: string; args: Record<string, unknown> }[] = [];
  let staged = false;
  await page.route('**/*', route => route.request().url().startsWith(origin) ? route.continue() : route.abort());
  await page.exposeFunction('governanceFixtureInvoke', (cmd: string, args: Record<string, unknown>) => {
    calls.push({ cmd, args });
    switch (cmd) {
      case 'local_list_spaces':
        return [{ name: 'CMake handbook', slug: 'handbook', id: 'handbook', visibility: 'private', role: 'owner', localPath: '/fixture/handbook' }];
      case 'local_list_agent_changes': return staged ? [fixture.staged, fixture.change] : [fixture.change];
      case 'local_review_governance': return report;
      case 'stage_governance_as_new_change':
        if (stale) throw new Error('Repair preview changed. Refresh the audit.');
        staged = true;
        return fixture.staged;
      case 'local_read_space_config': return { features: { modules: {} } };
      case 'local_list_pages':
        return [
          ['build-message-navigation', 'Build message navigation'], ['recently-produced-inputs', 'Recently produced inputs'],
          ['file-3.28.0', 'File operations · 3.28.0'], ['reference-index', 'Reference index'],
        ].map(([slug, title]) => ({ slug, title, path: slug + '.md', branch: 'main', kind: 'page', summary: '', children: [] }));
      case 'local_list_sources': case 'local_working_diff': case 'agent_list_sessions': return [];
      case 'local_set_auto_checkpoint': case 'local_auto_checkpoint': return null;
      case 'cloud_get_status': return { state: 'unlinked', message: '', conflicts: [], pullRequest: null };
      case 'plugin:event|listen': return 1;
      case 'plugin:event|unlisten': return null;
      default: throw new Error(`Unexpected fixture command: ${cmd}`);
    }
  });
  await page.addInitScript(() => {
    Object.assign(window, {
      __TAURI_INTERNALS__: { invoke: Reflect.get(window, 'governanceFixtureInvoke'), transformCallback: () => 1 },
      __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => {} },
    });
    localStorage.setItem('cowiki.personal.preferences', JSON.stringify({ autoCheckpoint: false }));
  });
  return calls;
}

async function screenshot(page: Page, filename: string) {
  const dir = process.env.GOVERNANCE_SCREENSHOTS;
  if (dir) await page.screenshot({ path: `${dir}/${filename}.png`, animations: 'disabled' });
}

test('review findings, defer a conflict, preview a diff and stage only the selected repair', async ({ page }) => {
  const calls = await desktop(page);
  await page.setViewportSize({ width: 1440, height: 1080 });
  await page.goto(origin + '/user/handbook/reviews');
  await page.getByRole('button', { name: /Update the CMake build handbook/ }).click();
  await page.getByRole('tab', { name: 'Governance' }).click();
  await expect(page.getByRole('list', { name: 'Findings' }).getByRole('listitem')).toHaveCount(fixture.report.findings.length);
  await screenshot(page, 'governance-findings');
  await page.getByRole('button', { name: /contradicts_verified/ }).click();
  await expect(page.getByText('Offline rules · contradiction')).toBeVisible();
  await expect(page.getByText('[file-3.28.0.md:L17-L17]', { exact: false })).toBeVisible();
  await page.getByRole('button', { name: 'Leave for human review' }).click();
  expect(calls.filter(c => c.cmd === 'stage_governance_as_new_change')).toHaveLength(0);

  await page.getByRole('button', { name: /inbound_redirect/ }).click();
  const repair = page.getByRole('region', { name: /Proposed repair for/ });
  await expect(repair.getByRole('group', { name: 'Review display' })).toBeVisible();
  await expect(page.getByText('Structural audit', { exact: true })).toBeVisible();
  await page.getByRole('checkbox', { name: 'Select repair' }).check();
  await page.getByRole('button', { name: 'Stage 1 repair as new change' }).scrollIntoViewIfNeeded();
  await screenshot(page, 'governance-repair-diff');
  await page.getByRole('button', { name: 'Stage 1 repair as new change' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Repairs staged in a new Agent Change' })).toBeVisible();
  await screenshot(page, 'governance-staged');
  expect(calls.filter(c => c.cmd === 'stage_governance_as_new_change').map(c => c.args)).toEqual([{
    spaceSlug: 'handbook', changeId: fixture.change.id, asOf: fixture.report.asOf, proposalIds: [fixture.report.proposals[0].id],
  }]);
  expect(calls.some(c => ['local_merge_agent_change', 'local_discard_agent_change', 'local_keep_working_diff'].includes(c.cmd))).toBe(false);
  await page.getByRole('button', { name: 'Open change' }).click();
  await expect(page).toHaveURL(new RegExp(fixture.staged.id + '$'));
  await expect(page.getByRole('heading', { name: 'Governance repair' })).toBeVisible();
});

test('Draft review handles stale evidence and refresh clears the selection', async ({ page }) => {
  const calls = await desktop(page, { stale: true });
  await page.goto(origin + '/user/handbook/reviews/draft');
  await page.getByRole('tab', { name: 'Governance' }).click();
  await page.getByRole('button', { name: /inbound_redirect/ }).click();
  await page.getByRole('checkbox', { name: 'Select repair' }).check();
  await page.getByRole('button', { name: 'Stage 1 repair as new change' }).click();
  await expect(page.getByRole('alert')).toContainText('Refresh the audit');
  expect(calls.find(c => c.cmd === 'stage_governance_as_new_change')?.args.changeId).toBe(null);
  await page.getByRole('button', { name: 'Refresh audit' }).click();
  await expect(page.getByRole('list', { name: 'Findings' })).toBeVisible();
  await expect(page.getByRole('alert')).toHaveCount(0);
  await expect(page.getByRole('button', { name: /Stage 1 repair/ })).toHaveCount(0);
});

test('selection survives navigating between findings and stages both proposal IDs once', async ({ page }) => {
  // A second page links to the same stub; only the renderer selection is under test.
  const report = structuredClone(fixture.report);
  const issue = structuredClone(report.findings.find(f => f.code === 'inbound_redirect')!);
  const proposal = structuredClone(report.proposals[0]);
  issue.id = 'second-finding';
  issue.path = 'generator-guide.md';
  issue.location!.path = issue.path;
  proposal.id = 'second-proposal';
  proposal.findingIds = [issue.id];
  proposal.diff.path = issue.path;
  report.findings.push(issue);
  report.proposals.push(proposal);
  const calls = await desktop(page, { report });
  await page.goto(origin + '/user/handbook/reviews/agent/' + fixture.change.id);
  await page.getByRole('tab', { name: 'Governance' }).click();
  for (const name of [/inbound_redirect.*build-message-navigation/, /inbound_redirect.*generator-guide/]) {
    await page.getByRole('button', { name }).click();
    await page.getByRole('checkbox', { name: 'Select repair' }).check();
    await page.getByRole('button', { name: 'All findings' }).click();
  }
  await page.getByRole('button', { name: 'Stage 2 repairs as new change' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Repairs staged in a new Agent Change' })).toBeVisible();
  expect(calls.filter(c => c.cmd === 'stage_governance_as_new_change').map(c => c.args.proposalIds))
    .toEqual([[report.proposals[0].id, proposal.id]]);
});
