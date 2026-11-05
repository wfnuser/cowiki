import { test, expect, type Page } from '@playwright/test';
import { createServer, type ViteDevServer } from 'vite';
import { fileURLToPath } from 'node:url';

// The real desktop UI against a disposable native IPC boundary that also
// delivers `agent:event` the way Tauri does. No account, no model, no PTY.
let server: ViteDevServer;
let origin: string;
test.beforeAll(async () => {
  server = await createServer({ root: fileURLToPath(new URL('../../', import.meta.url)), logLevel: 'silent', server: { host: '127.0.0.1', port: 0 } });
  await server.listen();
  const address = server.httpServer!.address();
  if (!address || typeof address === 'string') throw new Error('Missing test port');
  origin = 'http://127.0.0.1:' + address.port;
});
test.afterAll(async () => { await server.close(); });

type Snapshot = Record<string, unknown> & { sessionId: string; seq: number; spaceSlug: string; status: string; items: unknown[]; approvals: unknown[] };

async function desktop(page: Page, options: { approval?: boolean; panelWidth?: number } = {}) {
  const calls: Array<{ cmd: string; args: Record<string, unknown> }> = [];
  const sessions = new Map<string, Snapshot>();
  const space = { name: 'Personal', slug: 'personal', id: 'personal', visibility: 'private', role: 'owner', localPath: '/fixture/personal' };
  const work = { ...space, name: 'Work', slug: 'work', id: 'work', localPath: '/fixture/work' };
  const meta = (slug: string) => ({ slug, path: slug + '.md', title: slug, summary: '', branch: 'main', kind: 'page', children: [] });
  let counter = 0;
  const emit = async (snapshot: Snapshot) => {
    await page.evaluate((value) => Reflect.get(window, '__fixtureEmit')('agent:event', { sessionId: value.sessionId, seq: value.seq, snapshot: value }), snapshot);
  };
  const update = async (id: string, change: Partial<Snapshot>) => {
    const current = sessions.get(id)!;
    const next = { ...current, ...change, seq: current.seq + 1, updatedAt: Date.now() } as Snapshot;
    sessions.set(id, next);
    await emit(next);
    return next;
  };
  await page.route('**/*', route => route.request().url().startsWith(origin) ? route.continue() : route.abort());
  await page.exposeFunction('fixtureInvoke', async (cmd: string, args: Record<string, unknown>) => {
    calls.push({ cmd, args });
    switch (cmd) {
      case 'local_list_spaces': return [space, work];
      case 'local_list_pages': return [meta('Welcome')];
      case 'local_get_page': return { ...meta(String(args.pageSlug)), body: '# Welcome\n' };
      case 'local_read_space_config': return { features: { modules: {} } };
      case 'local_list_page_comments': return { comments: [], snapshots: [] };
      case 'local_working_diff': case 'local_list_agent_changes': case 'local_list_sources': case 'local_list_backlinks': return [];
      case 'cloud_get_status': return { state: 'unlinked', conflicts: [], message: '', pullRequest: null, committed: false, cloudSpaceId: null, cloudBaseUrl: null };
      case 'local_set_auto_checkpoint': case 'local_auto_checkpoint': return null;
      case 'agent_probe_native': return { agent: args.agent, status: 'available', protocol: 'codex-app-server', version: 'codex-cli 0.166.0', account: 'ChatGPT', message: null, detail: null, canSignIn: true };
      case 'agent_list_sessions': return [...sessions.values()].filter((session) => session.spaceSlug === args.spaceSlug);
      case 'agent_create_session': {
        const request = args.request as { spaceSlug: string; mode: string; changeId: string | null; agent: string; title: string | null };
        counter += 1;
        const snapshot: Snapshot = { sessionId: `agent-${counter}`, provider: request.agent, title: request.title ?? 'Task', spaceSlug: request.spaceSlug, mode: request.mode, changeId: request.changeId, threadId: null, model: null, seq: 1, status: 'idle', items: [], approvals: [], error: null, notice: null, closed: false, createdAt: Date.now(), updatedAt: Date.now() };
        sessions.set(snapshot.sessionId, snapshot);
        return snapshot;
      }
      case 'agent_send': {
        const id = String(args.sessionId);
        const current = sessions.get(id)!;
        const started = await update(id, { status: 'running', threadId: 'thread-1', model: 'gpt-fake', items: [...current.items, { type: 'message', id: `user-${current.seq}`, role: 'user', text: args.text, turnId: null }] });
        setTimeout(() => {
          void update(id, {
            status: options.approval ? 'waiting' : 'running',
            items: [...started.items,
              { type: 'tool', id: 't1', turnId: 'turn-1', name: 'rg "note" .', toolKind: 'command', status: 'completed', output: 'Welcome.md:1:# Welcome', truncated: false },
              { type: 'message', id: 'm1', role: 'assistant', text: 'Reading **Welcome.md** <script>window.__pwned = 1</script>', turnId: 'turn-1' }],
            approvals: options.approval ? [{ id: 'approval-1', turnId: 'turn-1', kind: 'command', title: 'Run a command', description: 'Update the index', command: 'touch index.md', cwd: '/fixture/personal', paths: [] }] : [],
          });
        }, 50);
        return started;
      }
      case 'agent_answer_approval': {
        const id = String(args.sessionId);
        return update(id, { status: 'idle', approvals: [], notice: null });
      }
      case 'agent_stop': return update(String(args.sessionId), { status: 'stopped' });
      default: throw new Error('Unsupported fixture IPC: ' + cmd);
    }
  });
  await page.addInitScript((width: number | null) => {
    const callbacks = new Map<number, (message: unknown) => void>();
    const listeners = new Map<string, Array<{ id: number; handler: number }>>();
    let nextId = 1;
    const ipc = {
      pending: 0,
      transformCallback(callback: (message: unknown) => void) {
        const id = nextId++;
        callbacks.set(id, callback);
        return id;
      },
      async invoke(cmd: string, args: Record<string, unknown>) {
        if (cmd === 'plugin:event|listen') {
          const id = nextId++;
          const list = listeners.get(String(args.event)) ?? [];
          list.push({ id, handler: Number(args.handler) });
          listeners.set(String(args.event), list);
          return id;
        }
        if (cmd === 'plugin:event|unlisten') return null;
        ipc.pending++;
        try { return await Reflect.get(window, 'fixtureInvoke')(cmd, args); }
        finally { ipc.pending--; }
      },
    };
    Object.assign(window, {
      __TAURI_INTERNALS__: ipc,
      __TAURI_EVENT_PLUGIN_INTERNALS__: {
        unregisterListener(event: string, id: number) {
          listeners.set(event, (listeners.get(event) ?? []).filter((listener) => listener.id !== id));
        },
      },
      __fixtureEmit(event: string, payload: unknown) {
        for (const listener of listeners.get(event) ?? []) callbacks.get(listener.handler)?.({ event, id: listener.id, payload });
      },
    });
    localStorage.setItem('cowiki.personal.preferences', JSON.stringify({ openJournalOnStartup: false, autoCheckpoint: false }));
    if (width) localStorage.setItem('cowiki.agentPanelWidth', String(width));
  }, options.panelWidth ?? null);
  const count = (cmd: string) => calls.filter((call) => call.cmd === cmd).length;
  return { calls, sessions, count };
}

// AGENT_SCREENSHOTS=<dir> saves screenshots for visual review.
const shot = async (page: Page, name: string) => {
  const directory = process.env.AGENT_SCREENSHOTS;
  if (directory) await page.screenshot({ path: `${directory}/${name}.png` });
};

async function openAgent(page: Page) {
  await page.goto(origin + '/personal/Welcome');
  await expect(page.locator('article')).toContainText('Welcome');
  await page.getByRole('button', { name: 'Agent', exact: true }).click();
}

test('collapsing the panel does not stop a running task', async ({ page }) => {
  const state = await desktop(page);
  await openAgent(page);
  await page.getByRole('textbox', { name: 'Message agent' }).fill('Summarize the test note');
  await page.getByRole('button', { name: 'Send message' }).click();
  await expect(page.getByText('Reading')).toBeVisible();
  await page.getByRole('button', { name: 'Close agent panel' }).click();
  await page.getByRole('button', { name: 'Agent', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Stop task' })).toBeVisible();
  expect(state.count('agent_send')).toBe(1);
  expect(state.count('agent_stop')).toBe(0);
  expect(state.count('terminal_create')).toBe(0);
  // Agent text cannot run script or inject HTML.
  expect(await page.evaluate(() => Reflect.get(window, '__pwned'))).toBeUndefined();
  await expect(page.locator('aside[aria-label="Agent"] script')).toHaveCount(0);
  // Tool steps are collapsed until asked for.
  const step = page.getByRole('button', { name: /rg "note"/ });
  await expect(step).toHaveAttribute('aria-expanded', 'false');
  await step.click();
  await expect(page.getByText('Welcome.md:1:# Welcome')).toBeVisible();
  await shot(page, 'running');
  await page.getByRole('button', { name: 'Stop task' }).click();
  await expect(page.getByRole('button', { name: 'Send message' })).toBeVisible();
  expect(state.count('agent_stop')).toBe(1);
});

test('an approval is answered once from the keyboard', async ({ page }) => {
  const state = await desktop(page, { approval: true });
  await openAgent(page);
  await page.getByRole('textbox', { name: 'Message agent' }).fill('Update the index');
  await page.keyboard.press('Enter');
  const card = page.getByRole('group', { name: 'Approval needed: Run a command' });
  await expect(card).toBeVisible();
  await expect(card).toContainText('touch index.md');
  await shot(page, 'approval');
  await expect(page.getByRole('status')).toHaveText('Needs approval');
  const allow = card.getByRole('button', { name: 'Allow once: Run a command' });
  await allow.focus();
  await page.keyboard.press('Enter');
  await expect(card).toHaveCount(0);
  const answers = state.calls.filter((call) => call.cmd === 'agent_answer_approval');
  expect(answers).toEqual([{ cmd: 'agent_answer_approval', args: { sessionId: 'agent-1', approvalId: 'approval-1', decision: 'allow' } }]);
});

test('a narrow panel fits and the top bar gains no controls', async ({ page }) => {
  await desktop(page, { panelWidth: 320 });
  await page.goto(origin + '/personal/Welcome');
  await expect(page.locator('article')).toContainText('Welcome');
  const topBarButtons = () => page.locator('.app-header-actions button').count();
  const before = await topBarButtons();
  await page.getByRole('button', { name: 'Agent', exact: true }).click();
  const panel = page.locator('aside[aria-label="Agent"]');
  await expect(panel.getByRole('textbox', { name: 'Message agent' })).toBeVisible();
  await expect(panel.getByText('codex-cli 0.166.0')).toBeVisible();
  await shot(page, 'new-task-320');
  expect(await topBarButtons()).toBe(before);
  const overflow = await panel.evaluate((element) => element.scrollWidth - element.clientWidth);
  expect(overflow).toBeLessThanOrEqual(0);
  await page.getByRole('textbox', { name: 'Message agent' }).fill('x'.repeat(400));
  await expect(page.getByRole('button', { name: 'Send message' })).toBeInViewport();
});

test('switching Spaces shows only that Space’s sessions and keeps the other running', async ({ page }) => {
  const state = await desktop(page);
  await openAgent(page);
  await page.getByRole('textbox', { name: 'Message agent' }).fill('Personal task');
  await page.getByRole('button', { name: 'Send message' }).click();
  await expect(page.getByText('Reading')).toBeVisible();
  await page.getByRole('button', { name: 'W', exact: true }).click();
  await expect(page.locator('article')).toContainText('Welcome');
  await expect(page.getByText('Personal task')).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Stop task' })).toHaveCount(0);
  await page.getByRole('button', { name: 'P', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Choose Agent session' })).toContainText('Personal task');
  await expect(page.locator('aside[aria-label="Agent"]').getByText('Reading')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Stop task' })).toBeVisible();
  expect(state.count('agent_send')).toBe(1);
  expect(state.count('agent_stop')).toBe(0);
});
