import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { createServer } from 'vite';

import { joinLeadingTitle, sourceOffsetForClick, splitLeadingTitle } from '../src/lib/click-to-edit.ts';
import { isComposingKey, isSubmitEnter } from '../src/lib/ime.ts';
import { DEFAULT_JOURNAL_CONFIG, journalTimeline } from '../src/lib/journal.ts';

const mainLayout = readFileSync(new URL('../src/pages/MainLayout.tsx', import.meta.url), 'utf8');

test('a click in rendered text puts the caret at the same place in the Markdown', () => {
  const source = '# Title\n\nSome **bold** words and [[Linked page|a link]] here.\n';
  // "bold" is its own text node; the caret sat after "bo".
  assert.equal(sourceOffsetForClick(source, { text: 'bold', offset: 2, ratio: 0.3 }), source.indexOf('bold') + 2);
  // The alias of a wikilink is found in the source too.
  assert.equal(sourceOffsetForClick(source, { text: 'a link', offset: 0, ratio: 0.6 }), source.indexOf('a link'));
  // Text the source does not contain gives null: the caller edits at the end.
  assert.equal(sourceOffsetForClick(source, { text: 'Edited by wfnuser', offset: 3, ratio: 0 }), null);
  assert.equal(sourceOffsetForClick(source, { text: '   ', offset: 1, ratio: 0 }), null);
});

test('repeated text resolves to the occurrence nearest the click', () => {
  const source = 'todo\n\n'.repeat(3) + 'end';
  const last = source.lastIndexOf('todo');
  assert.equal(sourceOffsetForClick(source, { text: 'todo', offset: 4, ratio: 0.85 }), last + 4);
  assert.equal(sourceOffsetForClick(source, { text: 'todo', offset: 0, ratio: 0 }), 0);
});

test('a paragraph whose text differs from its source still finds the words before the caret', () => {
  // Rendered "Tom & Jerry ran" from source "Tom &amp; Jerry ran".
  const source = 'Tom &amp; Jerry ran away';
  const offset = sourceOffsetForClick(source, { text: 'Tom & Jerry ran', offset: 15, ratio: 0.5 });
  assert.equal(offset, source.indexOf(' Jerry ran') + ' Jerry ran'.length);
});

test('a journal entry shows under its date without repeating its title', () => {
  const full = '---\ntype: Journal\ndate: 2026-11-23\n---\n\n# Monday, November 23, 2026\n\n## Plan\n- ship\n';
  const { head, content } = splitLeadingTitle(full);
  assert.equal(content, '## Plan\n- ship\n');
  assert.equal(joinLeadingTitle(head, content), full);
  assert.equal(joinLeadingTitle(head, 'Changed\n'), head + 'Changed\n');

  assert.deepEqual(splitLeadingTitle('Just text\n'), { head: '', content: 'Just text\n' });
  const titleOnly = splitLeadingTitle('# Day');
  assert.deepEqual(titleOnly, { head: '# Day', content: '' });
  assert.equal(joinLeadingTitle(titleOnly.head, 'Hello'), '# Day\nHello');
  // Frontmatter without a title keeps the body apart from the closing fence.
  const fenced = splitLeadingTitle('---\na: 1\n---');
  assert.equal(joinLeadingTitle(fenced.head, 'Note'), '---\na: 1\n---\nNote');
});

test('Journals list today first, then earlier days; future days are left out', () => {
  const config = { ...DEFAULT_JOURNAL_CONFIG, folder: 'Daily' };
  const today = new Date(2026, 10, 23, 9, 30);
  const entries = journalTimeline(config, [
    'Daily/2026-11-20.md', 'Daily/2026-11-22.md', 'Daily/2026-11-30.md', 'Notes/2026-11-21.md', 'Daily/Plan.md',
  ], today);
  assert.deepEqual(entries.map((entry) => [entry.key, entry.missing]), [
    ['2026-11-23', true], ['2026-11-22', false], ['2026-11-20', false],
  ]);
  assert.equal(entries[0].path, 'Daily/2026-11-23.md');
  const written = journalTimeline(config, ['Daily/2026-11-23.md'], today);
  assert.deepEqual(written.map((entry) => entry.missing), [false]);
});

test('Enter that confirms an input-method candidate never sends', () => {
  assert.equal(isSubmitEnter({ key: 'Enter', keyCode: 13 }), true);
  assert.equal(isSubmitEnter({ key: 'Enter', shiftKey: true, keyCode: 13 }), false);
  // Chromium: still composing.
  assert.equal(isSubmitEnter({ key: 'Enter', isComposing: true, keyCode: 13 }), false);
  // WebKit (the macOS app): composition already ended, but the key is the IME's.
  assert.equal(isSubmitEnter({ key: 'Enter', isComposing: false, keyCode: 229 }), false);
  assert.equal(isComposingKey({ nativeEvent: { isComposing: false, keyCode: 229 } as KeyboardEvent }), true);
  assert.equal(isComposingKey({ nativeEvent: { isComposing: false, keyCode: 13 } as KeyboardEvent }), false);
  for (const file of ['agent/AgentComposer.tsx', 'SearchModal.tsx', 'InviteDialog.tsx', 'SettingsDialog.tsx', 'review/DiffView.tsx']) {
    const source = readFileSync(new URL(`../src/components/${file}`, import.meta.url), 'utf8');
    assert.match(source, /isSubmitEnter|isComposingKey/, `${file} must ignore input-method Enter`);
  }
});

test('a Space opens on Home, not on its first file, and pages edit where clicked', () => {
  assert.match(mainLayout, /setActiveView\(\{ kind: 'home', workspaceSlug: ws\.slug \}\)/);
  assert.match(mainLayout, /setActiveView\(\{ kind: 'home', workspaceSlug: personal\.slug \}\)/);
  assert.doesNotMatch(mainLayout, /resolveWorkspaceSwitchTarget|firstConcept/);
  // User decision 2026-11-24: every Space opens on Home; Journals is the stream.
  assert.match(mainLayout, /const showOverview = !!activeWorkspace && onStartPage;/);
  assert.match(mainLayout, /<JournalDay[\s\S]*entry=\{todayEntry\}/);
  assert.match(mainLayout, /onEditAt=\{canEditInline \?/);
  assert.match(mainLayout, /initialCursor=\{editStart && editStart\.path === activeView\.path/);
});

test('Home shows only what needs you, today and recent pages', async () => {
  const vite = await createServer({ root: fileURLToPath(new URL('../', import.meta.url)), appType: 'custom', logLevel: 'silent', server: { middlewareMode: true } });
  try {
    const { SpaceHome } = await vite.ssrLoadModule('/src/components/views/SpaceHome.tsx');
    const now = Date.UTC(2026, 10, 23, 12);
    const html = renderToStaticMarkup(React.createElement(SpaceHome, {
      now,
      needs: [
        { id: 'tasks', count: 3, label: 'tasks due', onOpen: () => {} },
        { id: 'reviews', count: 1, label: 'change to review', onOpen: () => {} },
      ],
      today: React.createElement('section', { 'aria-label': 'Today' }, 'Nov 23rd, 2026'),
      recent: [{ path: 'notes/Plan.md', title: 'Plan', modifiedAt: now / 1000 - 600 }],
      onOpenPage: () => {},
    }));
    assert.match(html, /aria-label="Needs you"[\s\S]*3<\/span> tasks due[\s\S]*1<\/span> change to review/);
    assert.match(html, /aria-label="Today"[\s\S]*aria-label="Recently edited"[\s\S]*Plan[\s\S]*notes[\s\S]*10m/);

    // Nothing to do: no line at all. A Cloud Space in the browser has no edit times.
    const empty = renderToStaticMarkup(React.createElement(SpaceHome, { now, needs: [], recent: null, onOpenPage: () => {} }));
    assert.doesNotMatch(empty, /Needs you|Recently edited/);
  } finally {
    await vite.close();
  }
});
