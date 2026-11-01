import assert from 'node:assert/strict';
import test from 'node:test';

import {
  obsidianToReaderMarkdown,
  renderedLineOffset,
  wikilinkFromHref,
  wikilinkHref,
} from '../src/lib/obsidian-markdown.ts';

test('wikilinks become navigable links that keep their label', () => {
  const out = obsidianToReaderMarkdown('See [[Transformer]] and [[Paper - Attention|the paper]].');
  assert.equal(
    out,
    `See [Transformer](${wikilinkHref('Transformer')}) and [the paper](${wikilinkHref('Paper - Attention|the paper')}).`,
  );
  assert.equal(wikilinkFromHref(wikilinkHref('A/B#C|D')), 'A/B#C|D');
  assert.equal(wikilinkFromHref('https://example.com'), null);
});

test('headings and block links show a readable label', () => {
  assert.match(obsidianToReaderMarkdown('[[Note#Setup]]'), /^\[Note › Setup\]/);
  assert.match(obsidianToReaderMarkdown('[[Note#^abc]]'), /^\[Note\]/);
  assert.match(obsidianToReaderMarkdown('[[#Local]]'), /^\[Local\]/);
});

test('image embeds become images and note embeds become links', () => {
  assert.equal(obsidianToReaderMarkdown('![[Pasted image 1.png]]'), '![](<Pasted image 1.png>)');
  assert.equal(obsidianToReaderMarkdown('![[diagram.png|300]]'), '![](<diagram.png>)');
  assert.equal(obsidianToReaderMarkdown('![[diagram.png|Flow]]'), '![Flow](<diagram.png>)');
  assert.equal(obsidianToReaderMarkdown('![[Meeting notes]]'), `[↪ Meeting notes](${wikilinkHref('Meeting notes')})`);
});

test('code is left alone and line count never changes', () => {
  const body = [
    '```md',
    '[[not a link]]',
    '```',
    'Inline `[[code]]` stays, [[Real]] converts.',
    '%% hidden',
    'still hidden %%',
    'Visible ==highlight== text ^block-1',
    '> [!warning] Careful',
    '> body',
  ].join('\n');
  const out = obsidianToReaderMarkdown(body);
  const lines = out.split('\n');
  assert.equal(lines.length, body.split('\n').length);
  assert.equal(lines[1], '[[not a link]]');
  assert.match(lines[3], /Inline `\[\[code\]\]` stays, \[Real\]\(#cowiki-wiki:Real\) converts\./);
  assert.equal(lines[4], '');
  assert.equal(lines[5], '');
  assert.equal(lines[6], 'Visible **highlight** text');
  assert.equal(lines[7], '> **⚠️ Careful**');
  assert.equal(lines[8], '> body');
});

test('rendered line offset locates the body inside the file', () => {
  const full = '---\ntitle: x\n---\n\n# Title\n- [ ] a\n';
  assert.equal(renderedLineOffset(full, '# Title\n- [ ] a'), 4);
  assert.equal(renderedLineOffset(full, ''), 0);
});

test('task statuses and Logseq markers render as checkboxes', () => {
  const body = ['- [/] Draft', '- [-] Dropped', '  * TODO Call Ana', '- DONE Shipped', '- CANCELLED Old', '- TODOS are words', '1. [/] Numbered'].join('\n');
  assert.equal(obsidianToReaderMarkdown(body), [
    '- [ ] Draft',
    '- [x] ~~Dropped~~',
    '  * [ ] Call Ana',
    '- [x] Shipped',
    '- [x] ~~Old~~',
    '- TODOS are words',
    '1. [ ] Numbered',
  ].join('\n'));
});

test('Logseq card properties are hidden and cloze text is shown', () => {
  const body = ['- Rust has {{cloze zero-cost}} abstractions #card', '  id:: 6540a1b2', '  card-repeats:: 1', '  - answer', 'status:: draft'].join('\n');
  assert.equal(obsidianToReaderMarkdown(body), [
    '- Rust has **zero-cost** abstractions #card',
    '',
    '',
    '  - answer',
    'status:: draft',
  ].join('\n'));
});
