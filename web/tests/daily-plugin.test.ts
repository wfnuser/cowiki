import assert from 'node:assert/strict';
import test, { type TestContext } from 'node:test';
import { mkdtemp, mkdir, readFile, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { createPluginHost, type BuiltinPlugin, type PluginSnapshot } from '../src/plugins/host.ts';
import { dailyNotesPlugin } from '../src/plugins/daily-notes.ts';
import { createSpacePageAdapter, type SpacePagePorts } from '../src/plugins/space-pages.ts';
import { parseSpaceFeatures, saveSpaceFeatures, DEFAULT_SPACE_FEATURES, type SpaceFeatures } from '../src/lib/space-features.ts';
import { ensureDatePage } from '../src/lib/date-page.ts';

const now = () => new Date(2026, 10, 2, 9, 15);
function snapshot(features: SpaceFeatures | null = structuredClone(DEFAULT_SPACE_FEATURES)): PluginSnapshot {
  return { space: { id: 'local-a', local: true }, features, preferences: { openJournalOnStartup: false, autoCheckpoint: true } };
}

async function fixture(t: TestContext) {
  const root = await mkdtemp(join(tmpdir(), 'cowiki-daily-plugin-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  let current = snapshot();
  const opened: string[] = [];
  let refreshes = 0;
  let calls = 0;
  const ports: SpacePagePorts = {
    read: async (path) => { calls++; return readFile(join(root, path), 'utf8'); },
    createOnly: async (path, body) => {
      calls++;
      await mkdir(dirname(join(root, path)), { recursive: true });
      await writeFile(join(root, path), body, { flag: 'wx' });
    },
    refresh: async () => { calls++; refreshes++; },
    open: async (path) => { calls++; opened.push(path); },
  };
  const host = createPluginHost({ definitions: [dailyNotesPlugin], getSnapshot: () => current, pages: () => ports, now });
  return { root, ports, host, opened, get calls() { return calls; }, get refreshes() { return refreshes; },
    get current() { return current; }, set current(value) { current = value; } };
}

test('host lists and dispatches a second built-in contribution without a shell branch', async () => {
  let current = snapshot();
  const opened: string[] = [];
  const example: BuiltinPlugin = {
    id: 'example', label: 'Example', description: 'A trusted test definition',
    enabled: (features) => features.modules.example !== false,
    setEnabled: (features, enabled) => ({ ...features, modules: { ...features.modules, example: enabled } }),
    actions: [{ id: 'open', label: 'Example action', icon: 'file', run: async ({ pages }) => { await pages.open('Example.md'); } }],
  };
  const host = createPluginHost({ definitions: [dailyNotesPlugin, example], getSnapshot: () => current,
    pages: () => ({ read: async () => '', createOnly: async () => {}, refresh: async () => {}, open: async (path) => { opened.push(path); } }), now });
  assert.deepEqual(host.actions().map((action) => action.id), ['journal:today', 'example:open']);
  await host.dispatch('example:open');
  assert.deepEqual(opened, ['Example.md']);
  current = { ...current, features: parseSpaceFeatures({ features: { modules: { example: false } } }) };
  assert.deepEqual(host.actions().map((action) => action.id), ['journal:today']);
  assert.equal(await host.dispatch('example:open'), false);
  assert.deepEqual(opened, ['Example.md']);
  assert.equal(await host.dispatch('not-registered:open'), false);
  current = { ...current, features: null };
  assert.deepEqual(host.actions(), []);
  assert.equal(await host.dispatch('example:open'), false);
  current = { ...snapshot(), space: { id: 'remote', local: false } };
  assert.deepEqual(host.actions(), []);
  assert.equal(await host.dispatch('example:open'), false);
  assert.throws(() => createPluginHost({ definitions: [example, example], getSnapshot: () => current, pages: () => { throw Error('must not run'); }, now }), /duplicate/i);
  assert.throws(() => createPluginHost({ definitions: [{ ...example, actions: [example.actions[0], example.actions[0]] }], getSnapshot: () => current, pages: () => { throw Error('must not run'); }, now }), /duplicate/i);
});

test('disabled, pending and nonlocal dispatch/startup do no page I/O', async (t) => {
  const disabled = structuredClone(DEFAULT_SPACE_FEATURES);
  disabled.modules.journal = false;
  for (const state of [snapshot(disabled), snapshot(null), { ...snapshot(), space: { id: 'remote', local: false } }]) {
    const f = await fixture(t);
    f.current = { ...state, preferences: { openJournalOnStartup: true, autoCheckpoint: true } };
    assert.deepEqual(f.host.actions(), []);
    assert.equal(await f.host.dispatch('journal:today'), false);
    await f.host.launch({ workspacesLoaded: true, initialPathname: '/' });
    assert.equal(f.calls, 0);
  }
});

test('Today reopens the same existing Markdown without rewriting it', async (t) => {
  const f = await fixture(t);
  await mkdir(join(f.root, 'journal'));
  const original = '# My existing note\n\n- [ ] keep this text exactly\n';
  await writeFile(join(f.root, 'journal/2026-11-02.md'), original);
  await f.host.dispatch('journal:today');
  await f.host.dispatch('journal:today');
  assert.deepEqual(f.opened, ['journal/2026-11-02.md', 'journal/2026-11-02.md']);
  assert.equal(await readFile(join(f.root, 'journal/2026-11-02.md'), 'utf8'), original);
  assert.equal(f.refreshes, 0);
});

test('Today creates a configured date path from its optional template', async (t) => {
  const f = await fixture(t);
  const features = structuredClone(DEFAULT_SPACE_FEATURES);
  features.journal = { folder: 'Daily', format: 'YYYY/MM/YYYY-MM-DD', template: 'Templates/Daily.md' };
  f.current = snapshot(features);
  await mkdir(join(f.root, 'Templates'));
  await writeFile(join(f.root, 'Templates/Daily.md'), '---\ntitle: Template\n---\n# {{title}}\n{{date}} {{time}}\n');
  await f.host.dispatch('journal:today');
  assert.equal(await readFile(join(f.root, 'Daily/2026/11/2026-11-02.md'), 'utf8'), '---\ntitle: "2026-11-02"\n---\n# 2026-11-02\n2026-11-02 09:15\n');
  assert.deepEqual(f.opened, ['Daily/2026/11/2026-11-02.md']);
  assert.equal(f.refreshes, 1);
});

test('existing colon template names and moment time formats remain usable', async (t) => {
  const f = await fixture(t);
  f.current.features!.journal = { folder: 'Daily', format: 'YYYY-MM-DD HH:mm', template: 'Templates/Daily: Notes.md' };
  await mkdir(join(f.root, 'Templates'));
  await writeFile(join(f.root, 'Templates/Daily: Notes.md'), '# {{title}}\nKept compatible');
  await f.host.dispatch('journal:today');
  assert.equal(await readFile(join(f.root, 'Daily/2026-11-02 09:15.md'), 'utf8'), '# 2026-11-02 09:15\nKept compatible');
  assert.deepEqual(f.opened, ['Daily/2026-11-02 09:15.md']);
});

test('the shared date-page helper remains usable by Tasks with the plugin disabled', async (t) => {
  const f = await fixture(t);
  f.current.features!.modules.journal = false;
  assert.equal(await f.host.dispatch('journal:today'), false);
  const path = await ensureDatePage(createSpacePageAdapter(f.ports, () => true), f.current.features!.journal, now(), now());
  assert.equal(path, 'journal/2026-11-02.md');
  assert.match(await readFile(join(f.root, path), 'utf8'), /type: Journal/);
  assert.deepEqual(f.opened, [], 'Tasks prepares a file without launching the plugin or navigating');
});

test('Today defaults only on missing template and does not swallow actual page/template read errors', async (t) => {
  const f = await fixture(t);
  f.current.features!.journal.template = 'Templates/Missing.md';
  await f.host.dispatch('journal:today');
  assert.match(await readFile(join(f.root, 'journal/2026-11-02.md'), 'utf8'), /## Plan[\s\S]*## Notes/);
  for (const errorPath of ['journal/2026-11-02.md', 'Templates/Missing.md']) {
    const opened: string[] = [];
    const host = createPluginHost({ definitions: [dailyNotesPlugin], getSnapshot: () => f.current, now,
      pages: () => ({ ...f.ports, read: async (path) => { if (path === errorPath) throw Error('Permission denied (os error 13)'); throw Object.assign(Error('missing'), { code: 'ENOENT' }); },
        createOnly: async () => { assert.fail('read error must not create'); }, open: async (path) => { opened.push(path); } }) });
    await assert.rejects(host.dispatch('journal:today'), /Permission denied/);
    assert.deepEqual(opened, []);
  }
});

test('an actual filesystem read failure is not mistaken for an absent page', async (t) => {
  const f = await fixture(t);
  await mkdir(join(f.root, 'journal/2026-11-02.md'), { recursive: true });
  await assert.rejects(f.host.dispatch('journal:today'), { code: 'EISDIR' });
  assert.deepEqual(f.opened, []);
  assert.equal(f.refreshes, 0);
});

test('create-only races keep the winner text and open it normally', async (t) => {
  const f = await fixture(t);
  const winner = '# Created concurrently by another editor\n';
  f.ports.createOnly = async (path, body) => {
    await mkdir(dirname(join(f.root, path)), { recursive: true });
    await writeFile(join(f.root, path), winner, { flag: 'wx' });
    await writeFile(join(f.root, path), body, { flag: 'wx' });
  };
  await f.host.dispatch('journal:today');
  assert.equal(await readFile(join(f.root, 'journal/2026-11-02.md'), 'utf8'), winner);
  assert.deepEqual(f.opened, ['journal/2026-11-02.md']);
});

test('pending Today cannot create or open after a Space switch or disabling the plugin', async (t) => {
  for (const change of ['space', 'disabled'] as const) {
    const f = await fixture(t);
    let release!: () => void;
    let started = false;
    const barrier = new Promise<void>((resolve) => { release = resolve; });
    f.ports.read = async () => { started = true; await barrier; throw Object.assign(Error('missing'), { code: 'ENOENT' }); };
    const action = f.host.dispatch('journal:today');
    assert.equal(started, true, 'the action must actually reach the held read');
    if (change === 'space') f.current = { ...snapshot(), space: { id: 'local-b', local: true } };
    else f.current = snapshot({ ...DEFAULT_SPACE_FEATURES, modules: { ...DEFAULT_SPACE_FEATURES.modules, journal: false } });
    release();
    assert.equal(await action, false);
    assert.deepEqual(f.opened, []);
    await assert.rejects(readFile(join(f.root, 'journal/2026-11-02.md')), { code: 'ENOENT' });
  }
});

test('launch waits for configuration and only honors explicit root-start preference once', async (t) => {
  for (const pathname of ['/', '/local-a/Existing', '/local-a/reviews']) {
    const f = await fixture(t);
    f.current = snapshot(null);
    await f.host.launch({ workspacesLoaded: true, initialPathname: pathname });
    assert.equal(f.calls, 0);
    f.current = snapshot();
    f.current.preferences.openJournalOnStartup = true;
    await f.host.launch({ workspacesLoaded: true, initialPathname: pathname });
    await f.host.launch({ workspacesLoaded: true, initialPathname: '/' });
    assert.deepEqual(f.opened, pathname === '/' ? ['journal/2026-11-02.md'] : []);
  }
  const f = await fixture(t);
  await f.host.launch({ workspacesLoaded: false, initialPathname: '/' });
  assert.equal(f.calls, 0);
  await f.host.launch({ workspacesLoaded: true, initialPathname: '/' });
  f.current.preferences.openJournalOnStartup = true;
  await f.host.launch({ workspacesLoaded: true, initialPathname: '/' });
  assert.equal(f.calls, 0, 'changing a preference after launch is not a new launch');
});

test('page adapter bounds paths and distinguishes native missing-file and create-only errors', async () => {
  let reads = 0;
  const ports: SpacePagePorts = { read: async () => { reads++; throw 'No such file or directory (os error 2)'; },
    createOnly: async () => { throw 'a page with this name already exists'; }, refresh: async () => {}, open: async () => {} };
  const pages = createSpacePageAdapter(ports, () => true);
  assert.equal(await pages.read('Daily.md'), null);
  assert.equal(await pages.createOnly('Daily.md', '# Day'), 'exists');
  for (const path of ['../outside.md', '/absolute.md', 'C:/outside.md', '.cowiki/config.md', 'log.md', 'file.js']) {
    await assert.rejects(pages.read(path), /path/i);
  }
  assert.equal(reads, 1);
  ports.read = async () => { throw 'could not find repository at path'; };
  await assert.rejects(pages.read('Daily.md'), (error) => error === 'could not find repository at path');
});

test('saving through the public config adapter retains unknown settings and rejects a read failure', async () => {
  const original = { customRoot: { keep: [1, 2] }, features: { futurePlugin: { n: 7 }, modules: { journal: true, custom: false },
    journal: { folder: 'Daily', format: 'YYYY_MM_DD', template: 'Templates/Daily.md', future: 'keep' }, templates: { folder: 'Meta', future: 3 } } };
  let stored: unknown = structuredClone(original);
  const features = structuredClone(DEFAULT_SPACE_FEATURES);
  features.modules.tasks = false;
  features.journal = { folder: 'Daily', format: 'YYYY_MM_DD', template: 'Templates/Daily.md' };
  features.templates.folder = 'Meta';
  await saveSpaceFeatures({ read: async () => stored, write: async (next) => { stored = next; } }, features);
  const result = stored as typeof original;
  assert.deepEqual(result.customRoot, original.customRoot);
  assert.deepEqual(result.features.futurePlugin, original.features.futurePlugin);
  assert.equal(result.features.modules.custom, original.features.modules.custom);
  assert.deepEqual(result.features.journal, original.features.journal);
  assert.deepEqual(result.features.templates, original.features.templates);
  await assert.rejects(saveSpaceFeatures({ read: async () => { throw Error('unreadable config'); }, write: async () => { assert.fail('must not overwrite unreadable config'); } }, features), /unreadable config/);
});

test('the real sidebar renders contributed commands rather than a hardwired Today entry', async () => {
  const { createServer } = await import('vite');
  const { fileURLToPath } = await import('node:url');
  const React = await import('react');
  const { renderToStaticMarkup } = await import('react-dom/server');
  const vite = await createServer({ root: fileURLToPath(new URL('../', import.meta.url)), appType: 'custom', logLevel: 'silent', server: { middlewareMode: true } });
  try {
    const { SpacePanel } = await vite.ssrLoadModule('/src/components/layout/SpacePanel.tsx');
    const props = { workspace: { id: 'a', slug: 'a', name: 'A' }, activeTab: 'wiki', pages: [], sources: [],
      isPersonal: true, modules: ['journal'], onCollapse: () => {} };
    const withoutContribution = renderToStaticMarkup(React.createElement(SpacePanel, props));
    assert.doesNotMatch(withoutContribution, />Today</);
    const contributed = renderToStaticMarkup(React.createElement(SpacePanel, { ...props,
      pluginActions: [{ id: 'example:open', label: 'Example action', icon: 'file' }], onPluginAction: () => {} }));
    assert.match(contributed, />Example action</);
  } finally { await vite.close(); }
});

test('plugin-owned settings render another definition with no Journal-specific fields', async () => {
  const { createServer } = await import('vite');
  const { fileURLToPath } = await import('node:url');
  const React = await import('react');
  const { renderToStaticMarkup } = await import('react-dom/server');
  const vite = await createServer({ root: fileURLToPath(new URL('../', import.meta.url)), appType: 'custom', logLevel: 'silent', server: { middlewareMode: true } });
  try {
    const { PluginSettings } = await vite.ssrLoadModule('/src/plugins/PluginSettings.tsx');
    const example: BuiltinPlugin = { id: 'example', label: 'Example plugin', description: 'Example settings', enabled: () => true,
      setEnabled: (features) => features, actions: [], settings: { title: 'Example fields', help: 'Example help', fields: [
        { id: 'example-value', label: 'Example field', value: () => 'Kept value', update: (features) => features },
      ], preferences: [{ id: 'example-personal', label: 'Example personal preference', value: () => true, update: (preferences) => preferences }] } };
    const html = renderToStaticMarkup(React.createElement(PluginSettings, { plugins: [example], features: snapshot().features,
      preferences: snapshot().preferences, onFeatures: () => {}, onPreferences: () => {} }));
    assert.match(html, /Example plugin/);
    assert.match(html, /Example field/);
    assert.match(html, /Kept value/);
    assert.match(html, /Example personal preference/);
    assert.doesNotMatch(html, /Journal|journal-folder|journal-startup/);
  } finally { await vite.close(); }
});
