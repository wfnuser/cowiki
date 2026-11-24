import { test, type Page } from '@playwright/test';
import { createServer, type ViteDevServer } from 'vite';
import { fileURLToPath } from 'node:url';

// A gallery of every main screen, light and dark, for design review — not a
// test. Run it when polishing the UI and compare before/after:
//   SCREENS=/tmp/cowiki-screens npx playwright test screens --project=chromium
// (DESIGN_SYSTEM.md, "Reviewing the whole app").
const OUT = process.env.SCREENS;
test.skip(!OUT, 'set SCREENS=<folder> to capture the screen gallery');

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

const NOW = new Date(2026, 10, 24, 15, 0);
const secondsAgo = (seconds: number) => Math.floor(NOW.getTime() / 1000) - seconds;

const WELCOME = `---
type: Note
title: Welcome
---

# Welcome to CoWiki

CoWiki keeps your notes as plain Markdown files. Link pages with [[Rust ownership]] and they stay connected; edit by clicking anywhere in the text.

## This week

- [ ] Write the weekly summary 📅 2026-11-24
- [x] Review flashcards
- Read *Thinking, Fast and Slow*, chapter 3
  - System 1 is fast and automatic
  - System 2 is slow and deliberate

> [!note] Tip
> Press Esc to stop editing.

### Reading list

| Book | Author | Status |
| --- | --- | --- |
| Deep Work | Cal Newport | Reading |
| How to Take Smart Notes | Sönke Ahrens | Next |

1. Collect
2. Connect
3. Create

\`\`\`ts
const answer = await wiki.search('ownership');
\`\`\`
`;

const RUST = `---
type: Note
title: Rust ownership
tags: [Rust]
---

# Rust ownership

- Can a moved \`String\` still be used? #card
  - No. Ownership moved, so the old binding is invalid.
- What does \`&String\` mean in a parameter? #card
  - A borrow: the function gets a reference, not ownership.
`;

const JOURNAL = (day: string, text: string) => `---\ntype: Journal\ntitle: "${day}"\n---\n\n# ${day}\n\n${text}\n`;

const files = new Map<string, string>([
  ['Welcome', WELCOME],
  ['wiki/Rust ownership', RUST],
  ['wiki/Paper notes', '---\ntype: Note\ntitle: Paper notes\ntags: [Research]\n---\n\n# Paper notes\n\n- Why do Transformers train faster than RNNs? #card\n  - Attention does not wait for the previous step.\n'],
  ['journal/2026-11-24', JOURNAL('2026-11-24', '## Plan\n\n- [ ] Send the weekly report\n- [x] Morning review\n\n## Notes\n\n- Read [[Rust ownership]]: the borrow checker is a compile-time read/write lock.')],
  ['journal/2026-11-23', JOURNAL('2026-11-23', '- [ ] Reply to the advisor\n- Watched a talk on local-first software.')],
  ['journal/2026-11-21', JOURNAL('2026-11-21', '- Library: borrowed *Deep Work*.')],
]);

const diff = (path: string, added: string[], removed: string[] = []) => ({
  path, old_content: removed.join('\n'), new_content: added.join('\n'), additions: added.length, deletions: removed.length,
  hunks: [{ header: '@@ -1,2 +1,3 @@', lines: [
    { kind: 'ctx', old_line: 1, new_line: 1, text: '# ' + path.split('/').pop() },
    ...removed.map((text, index) => ({ kind: 'del', old_line: index + 2, new_line: null, text })),
    ...added.map((text, index) => ({ kind: 'add', old_line: null, new_line: index + 2, text })),
  ] }],
});

async function desktop(page: Page) {
  const space = { name: 'My Space', slug: 'personal', id: 'personal', visibility: 'private', role: 'owner', localPath: '/Users/me/cowiki/personal' };
  const meta = (slug: string) => ({ slug, path: slug + '.md', title: slug.split('/').pop(), summary: '', branch: 'main', kind: 'page', children: [] });
  await page.clock.setFixedTime(NOW);
  await page.route('**/*', (route) => route.request().url().startsWith(origin) ? route.continue() : route.abort());
  await page.exposeFunction('fixtureInvoke', async (cmd: string, args: Record<string, unknown>) => {
    switch (cmd) {
      case 'local_list_spaces': return [space];
      case 'local_list_pages': return Array.from(files.keys(), meta);
      case 'local_get_page': return { ...meta(String(args.pageSlug)), body: files.get(String(args.pageSlug)) ?? '', edited_by: 'wfnuser', edited_at: secondsAgo(600) };
      case 'local_write_page': files.set(String(args.pageSlug), String(args.content)); return null;
      case 'local_read_space_config': return { features: { modules: { journal: true, tasks: true, flashcards: true }, journal: { format: 'YYYY-MM-DD', folder: 'journal', template: null } } };
      case 'local_list_page_comments': return { comments: [], snapshots: [] };
      case 'local_list_task_sources': return [...files].filter(([, body]) => body.includes('- [')).map(([slug, content]) => ({ path: slug + '.md', content }));
      case 'local_list_card_sources': return [...files].filter(([, body]) => body.includes('#card')).map(([slug, content]) => ({ path: slug + '.md', content }));
      case 'local_read_card_reviews': return null;
      case 'local_recent_pages': return [
        { path: 'wiki/Rust ownership.md', title: 'Rust ownership', modifiedAt: secondsAgo(300) },
        { path: 'journal/2026-11-23.md', title: '2026-11-23', modifiedAt: secondsAgo(7200) },
        { path: 'Welcome.md', title: 'Welcome', modifiedAt: secondsAgo(90000) },
      ];
      case 'local_working_diff': return [diff('Welcome.md', ['CoWiki keeps your notes as plain Markdown files.', 'Edit by clicking anywhere.'], ['CoWiki keeps notes.']), diff('journal/2026-11-24.md', ['- [ ] Send the weekly report'])];
      case 'local_list_agent_changes': return [{ id: 'a1b2c3d4e5f6', title: 'Summarize this week’s papers', status: 'open', createdAt: secondsAgo(3600), worktreePath: '/x', diffs: [diff('wiki/Paper notes.md', ['- Chain-of-thought helps multi-step problems.'])] }];
      case 'local_history': return { currentDraft: { changedFiles: 2 }, checkpoints: [
        { id: 'c1', name: 'Auto checkpoint', createdAt: secondsAgo(3000) },
        { id: 'c2', name: 'Before importing Obsidian', createdAt: secondsAgo(90000) },
        { id: 'c3', name: 'Auto checkpoint', createdAt: secondsAgo(200000) },
      ] };
      case 'local_list_broken_links': return [
        { source_path: 'Welcome.md', source_title: 'Welcome', target: 'Thinking, Fast and Slow' },
        { source_path: 'journal/2026-11-24.md', source_title: '2026-11-24', target: 'Weekly report' },
      ];
      case 'local_search': return { keyword: [
        { slug: 'wiki/Rust ownership', path: 'wiki/Rust ownership.md', title: 'Rust ownership', snippet: 'Can a moved String still be used?', title_match: false },
        { slug: 'Welcome', path: 'Welcome.md', title: 'Welcome', snippet: 'Link pages with Rust ownership and they stay connected', title_match: false },
      ], semantic: [] };
      case 'local_list_sources': case 'local_list_backlinks': return [];
      case 'cloud_get_status': return { state: 'unlinked', conflicts: [], message: '', pullRequest: null, committed: false, cloudSpaceId: null, cloudBaseUrl: null };
      case 'agent_probe_native': return { agent: args.agent, status: 'available', protocol: 'claude-code', version: '2.1.0', account: 'Claude Pro', message: null, detail: null, canSignIn: true, models: [{ id: 'opus', name: 'Opus', description: '', isDefault: true, resolved: null }] };
      case 'agent_list_sessions': return [];
      default: return null;
    }
  });
  await page.addInitScript(() => {
    Object.assign(window, { __TAURI_INTERNALS__: { invoke: (cmd: string, args: unknown) => Reflect.get(window, 'fixtureInvoke')(cmd, args) } });
    localStorage.setItem('cowiki.personal.preferences', JSON.stringify({ autoCheckpoint: false }));
  });
}

// A sidebar entry; its name may end with a count ("Reviews 1").
const nav = (page: Page, name: string) => page.getByRole('button', { name: new RegExp(`^${name}\\s*\\d*$`) }).first().click();

for (const scheme of ['light', 'dark'] as const) {
  test(`screens (${scheme})`, async ({ page }) => {
    test.setTimeout(120_000);
    await page.setViewportSize({ width: 1280, height: 800 });
    await page.emulateMedia({ colorScheme: scheme, reducedMotion: 'reduce' });
    await desktop(page);
    await page.goto(origin);
    const shot = async (name: string) => {
      await page.waitForTimeout(500);
      await page.screenshot({ path: `${OUT}/${scheme}-${name}.png` });
    };

    await shot('01-home');
    await nav(page, 'Journals'); await shot('02-journals');
    await page.getByText('Welcome', { exact: true }).first().click(); await shot('03-page');
    await page.getByText('CoWiki keeps your notes').click(); await shot('04-page-editing');
    await page.keyboard.press('Escape');
    await nav(page, 'Tasks'); await shot('05-tasks');
    await nav(page, 'Flashcards'); await shot('06-flashcards');
    await page.getByRole('button', { name: /^Rust/ }).click(); await shot('07-deck');
    await page.getByRole('button', { name: 'Study now' }).click(); await shot('08-study');
    await page.keyboard.press('Space'); await shot('09-study-answer');
    await nav(page, 'Reviews'); await shot('10-reviews');
    await page.getByRole('button', { name: /Current Draft/ }).click(); await shot('11-review-detail');
    await nav(page, 'History'); await shot('12-history');
    await nav(page, 'Links'); await shot('13-links');
    await nav(page, 'Home');
    await page.keyboard.press(process.platform === 'darwin' ? 'Meta+k' : 'Control+k');
    await page.keyboard.type('String'); await shot('14-search');
    await page.keyboard.press('Escape');
    await nav(page, 'Settings'); await shot('15-settings-space');
    await page.getByRole('dialog').getByRole('button', { name: 'General', exact: true }).last().click(); await shot('16-settings-app');
    await page.keyboard.press('Escape');
    await page.getByRole('button', { name: /Draft/ }).first().click(); await shot('17-status-menu');
    await page.keyboard.press('Escape');
    await page.getByRole('button', { name: 'Agent', exact: true }).click(); await shot('18-agent');
  });
}
