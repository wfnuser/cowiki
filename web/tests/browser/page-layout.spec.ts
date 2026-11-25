import { test, expect, type Page } from '@playwright/test';
import { createServer, type ViteDevServer } from 'vite';
import { fileURLToPath } from 'node:url';

// Click to edit must not move the page (DESIGN_SYSTEM.md, "Editing"): the
// reader and the editor lay out every kind of block at the same place.
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

const BODY = `# Parity Title

Alpha paragraph with **bold** and a [[Linked page|link]] in it.
Bravo second line of the same paragraph.

## Charlie heading

Delta para after heading.
## Echo tight heading
Foxtrot para right under a heading.

- Golf bullet
- Hotel bullet that is long enough to wrap around the column width so we can check hanging indentation lines in both views really wrapwordhere
  - India nested
    Juliet continuation
- [ ] Kilo task
- [x] Lima done

1. Mike one
2. November two

> Oscar quote
>
> Papa quote

> [!tip] Whiskey callout
> Xray inside

\`\`\`js
Quebec code
\`\`\`

Romeo after code.

---

Sierra after rule.

### Tango h3
Uniform \`inline code\` here.

| Victor col | Yankee col |
| --- | ---: |
| Foxcell one | \`codecell\` |
| Golfcell two | **boldcell** |

Zulu after table.
`;
const MARKERS = ['Parity', 'Alpha', 'Bravo', 'Charlie', 'Delta', 'Echo', 'Foxtrot', 'Golf', 'Hotel', 'wrapwordhere', 'India', 'Juliet', 'Kilo', 'Lima', 'Mike', 'November', 'Oscar', 'Papa', 'Whiskey', 'Xray', 'Quebec', 'Romeo', 'Sierra', 'Tango', 'Uniform', 'Victor', 'Foxcell', 'Zulu'];

async function setup(page: Page, body: string, more: Array<[string, string]> = []) {
  const space = { name: 'Personal', slug: 'personal', id: 'personal', visibility: 'private', role: 'owner', localPath: '/fixture/personal' };
  const files = new Map([['Doc', body], ...more]);
  const meta = (slug: string) => ({ slug, path: slug + '.md', title: slug, summary: '', branch: 'main', kind: 'page', children: [] });
  await page.route('**/*', (route) => route.request().url().startsWith(origin) ? route.continue() : route.abort());
  await page.exposeFunction('fixtureInvoke', async (cmd: string, args: { pageSlug: string; content: string }) => {
    switch (cmd) {
      case 'local_list_spaces': return [space];
      case 'local_list_pages': return Array.from(files.keys(), meta);
      case 'local_get_page': return { ...meta(args.pageSlug), body: files.get(args.pageSlug), edited_by: 'wfnuser', edited_at: 1700000000 };
      case 'local_write_page': files.set(args.pageSlug, args.content); return;
      case 'local_read_space_config': return { features: { modules: { journal: true }, journal: { format: 'YYYY-MM-DD', folder: 'Daily', template: null } } };
      case 'local_list_page_comments': return { comments: [], snapshots: [] };
      case 'local_working_diff': case 'local_list_agent_changes': case 'local_list_sources': case 'local_list_backlinks': case 'local_recent_pages': return [];
      case 'cloud_get_status': return { state: 'unlinked', conflicts: [], message: '', pullRequest: null, committed: false, cloudSpaceId: null, cloudBaseUrl: null };
      case 'local_set_auto_checkpoint': case 'local_auto_checkpoint': return null;
      default: throw new Error('Unsupported fixture IPC: ' + cmd);
    }
  });
  await page.addInitScript(() => {
    Object.assign(window, { __TAURI_INTERNALS__: { invoke: (cmd: string, args: unknown) => Reflect.get(window, 'fixtureInvoke')(cmd, args) } });
    localStorage.setItem('cowiki.personal.preferences', JSON.stringify({ autoCheckpoint: false }));
  });
}

/** Position of each marker's first character relative to the page column. */
async function positions(page: Page, rootSelector: string, markers = MARKERS) {
  return page.evaluate(({ rootSelector, markers }) => {
    const root = document.querySelector(rootSelector)!;
    const column = root.querySelector('.page-column')!.getBoundingClientRect();
    const out: Record<string, [number, number]> = {};
    for (const marker of markers) {
      const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
      for (let node = walker.nextNode(); node; node = walker.nextNode()) {
        const index = node.textContent!.indexOf(marker);
        if (index < 0) continue;
        const range = document.createRange();
        range.setStart(node, index);
        range.setEnd(node, index + 1);
        const rect = range.getClientRects()[0];
        out[marker] = [Math.round((rect.left - column.left) * 10) / 10, Math.round((rect.top - column.top) * 10) / 10];
        break;
      }
    }
    return out;
  }, { rootSelector, markers });
}

test('clicking to edit leaves every line where it was', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await setup(page, BODY);
  await page.goto(origin + '/personal/Doc');
  await page.locator('article').getByText('Delta para').waitFor();
  const read = await positions(page, 'main');
  const readCalloutColor = await page.locator('article .callout-title').evaluate((element) => getComputedStyle(element).color);
  const before = await page.getByText('Delta para').boundingBox();
  await page.getByText('Delta para').click({ position: { x: 10, y: 8 } });
  await expect(page.locator('.cm-content')).toBeVisible();
  const edit = await positions(page, 'main');
  const editCalloutColor = await page.locator('.cm-lp-callout-title').evaluate((element) => {
    const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT);
    const text = walker.nextNode();
    return getComputedStyle(text?.parentElement ?? element).color;
  });
  expect(editCalloutColor).toBe(readCalloutColor);
  for (const marker of MARKERS) {
    expect(edit[marker], marker).toBeTruthy();
    expect(Math.abs(edit[marker][0] - read[marker][0]), `${marker} moved sideways`).toBeLessThan(0.6);
    expect(Math.abs(edit[marker][1] - read[marker][1]), `${marker} moved down`).toBeLessThan(1);
  }
  // The clicked line stays at the same place on screen.
  const after = await page.locator('.cm-line', { hasText: 'Delta para' }).boundingBox();
  expect(Math.abs(after!.y - before!.y)).toBeLessThan(1);

  // And back: Esc returns to the reader at the editor's scroll position.
  await page.locator('main [style*="overflow: auto"]').first().evaluate((element) => { element.scrollTop = 120; });
  const editorTop = await page.locator('.cm-line', { hasText: 'Romeo' }).boundingBox();
  await page.keyboard.press('Escape');
  await expect(page.locator('.cm-content')).toHaveCount(0);
  const readerTop = await page.locator('article').getByText('Romeo').boundingBox();
  expect(Math.abs(readerTop!.y - editorTop!.y)).toBeLessThan(1);
});

test('list and heading markup never shifts the text', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await setup(page, BODY);
  await page.goto(origin + '/personal/Doc');
  await page.getByText('Charlie heading').click();
  // `## ` sits in the margin while the caret is on the heading.
  const mark = await page.locator('.cm-lp-hmark').boundingBox();
  const line = await page.locator('.cm-line', { hasText: 'Charlie' }).boundingBox();
  expect(mark!.x + mark!.width).toBeLessThanOrEqual(line!.x + 0.5);
  // Code fences show only while the caret is in the block.
  await expect(page.locator('.cm-lp-fence-shown')).toHaveCount(0);
  await page.locator('.cm-line', { hasText: 'Quebec' }).click();
  await expect(page.locator('.cm-lp-fence-shown')).toHaveCount(2);
  // Tables stay rendered while the caret is outside, reveal raw pipes when clicked,
  // and render again once the caret leaves.
  await expect(page.locator('.cm-lp-table-widget table')).toHaveCount(1);
  await page.locator('.cm-lp-table-widget td', { hasText: 'Foxcell one' }).click();
  await expect(page.locator('.cm-lp-table-widget')).toHaveCount(0);
  await expect(page.locator('.cm-lp-table', { hasText: '| Foxcell one |' })).toBeVisible();
  await page.locator('.cm-line', { hasText: 'Zulu after table.' }).click();
  await expect(page.locator('.cm-lp-table-widget table')).toHaveCount(1);
  // Enter continues a list; Home then Backspace removes the new bullet.
  await page.locator('.cm-line', { hasText: 'Golf bullet' }).click();
  await page.keyboard.press('End');
  await page.keyboard.press('Enter');
  await page.keyboard.type('Zulu');
  await page.keyboard.press('Home');
  await page.keyboard.press('Backspace');
  const text = await page.locator('.cm-content').innerText();
  expect(text.slice(text.indexOf('Golf'), text.indexOf('Hotel'))).toBe('Golf bullet\nZulu\n');
  await page.locator('.cm-lp-checkbox').first().click();
  await expect(page.locator('.cm-lp-checkbox').first()).toBeChecked();
});

test('a journal day is written in place without moving', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.clock.setFixedTime(new Date('2026-11-24T15:00:00'));
  const day = '---\ntype: Journal\n---\n\n# Monday, November 23, 2026\n\n## Plan\n- Alpha item\n- Bravo item\n\n## Notes\nCharlie note\nDelta line\n';
  await setup(page, BODY, [['Daily/2026-11-23', day], ['Daily/2026-11-20', '# 2026-11-20\n\nEcho earlier day\n']]);
  await page.goto(origin);
  await page.getByRole('button', { name: 'Journals', exact: true }).click();
  const yesterday = page.getByRole('region', { name: 'Monday, November 23, 2026' });
  await expect(yesterday.getByRole('heading', { name: 'Nov 23rd, 2026' })).toBeVisible();
  await expect(page.getByRole('region', { name: 'Tuesday, November 24, 2026' }).getByRole('button', { name: 'Write in this day' })).toBeVisible();
  await page.screenshot({ path: test.info().outputPath('journals.png') });
  const markers = ['Plan', 'Alpha', 'Bravo', 'Notes', 'Charlie', 'Delta', 'Echo'];
  const read = await positions(page, 'main', markers);
  await yesterday.getByText('Charlie note').click();
  await expect(yesterday.locator('.cm-content')).toBeVisible();
  const edit = await positions(page, 'main', markers);
  for (const marker of markers) {
    expect(Math.abs(edit[marker][0] - read[marker][0]), `${marker} moved sideways`).toBeLessThan(0.6);
    expect(Math.abs(edit[marker][1] - read[marker][1]), `${marker} moved down`).toBeLessThan(1);
  }
  await page.screenshot({ path: test.info().outputPath('journals-edit.png') });
});
