import assert from 'node:assert/strict';
import test from 'node:test';

import { BUILT_IN_TEMPLATES, newPageBody, templatePagePaths } from '../src/lib/page-templates.ts';

const now = new Date(2026, 9, 31, 9, 5);

test('a blank page keeps the existing properties block', () => {
  assert.equal(newPageBody('Week "plan"', null, now), '---\ntype: Note\ntitle: "Week \\"plan\\""\ndescription: ""\n---\n\n');
});

test('built-in templates fill title and date and add properties', () => {
  const meeting = BUILT_IN_TEMPLATES.find((template) => template.id === 'meeting');
  const body = newPageBody('Kickoff: CoWiki', meeting ?? null, now);
  assert.match(body, /^---\ntype: Meeting\ntitle: "Kickoff: CoWiki"\n/);
  assert.match(body, /# Kickoff: CoWiki\n\n\*\*Date:\*\* 2026-10-31 09:05/);
  const learning = BUILT_IN_TEMPLATES.find((template) => template.id === 'learning-plan');
  assert.match(newPageBody('Rust', learning ?? null, now), /\| 2026-10-31 \| {2}\| read \|/);
});

test('Space templates keep their own frontmatter', () => {
  const body = newPageBody('Idea', { body: '---\ntags: [idea]\n---\n# {{title}}\n' }, now);
  assert.equal(body, '---\ntags: [idea]\n---\n# Idea\n');
  assert.match(newPageBody('Idea', { body: '# {{title}}\n' }, now), /^---\ntype: Note\ntitle: "Idea"/);
});

test('template pages come from Templates and configured template folders', () => {
  assert.deepEqual(
    templatePagePaths(['Templates/Daily.md', 'templates/index.md', 'Meta/Tpl/Week.md', 'Notes/Templates.md', 'Templates/logo.png'], ['Meta/Tpl/Week.md']),
    ['Meta/Tpl/Week.md', 'Templates/Daily.md'],
  );
});
