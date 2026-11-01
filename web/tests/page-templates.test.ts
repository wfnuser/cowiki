import assert from 'node:assert/strict';
import test from 'node:test';

import { newPageBody, templatePagePaths } from '../src/lib/page-templates.ts';

const now = new Date(2026, 9, 31, 9, 5);

test('a blank page keeps the existing properties block', () => {
  assert.equal(newPageBody('Week "plan"', null, now), '---\ntype: Note\ntitle: "Week \\"plan\\""\ndescription: ""\n---\n\n');
});

test('a template page fills title, date and time and keeps its type', () => {
  const meeting = { body: '---\ntype: Meeting\ntitle: "Meeting notes"\n---\n\n# {{title}}\n\n**Date:** {{date}} {{time}}\n' };
  const body = newPageBody('Kickoff: CoWiki', meeting, now);
  assert.equal(body, '---\ntype: Meeting\ntitle: "Kickoff: CoWiki"\n---\n\n# Kickoff: CoWiki\n\n**Date:** 2026-10-31 09:05\n');
});

test('Space templates keep their own frontmatter but never their own title', () => {
  const body = newPageBody('Idea', { body: '---\ntags: [idea]\n---\n# {{title}}\n' }, now);
  assert.equal(body, '---\ntags: [idea]\ntitle: "Idea"\n---\n# Idea\n');
  // Imported template pages carry a title of their own after OKF normalization.
  assert.equal(newPageBody('Idea', { body: '---\ntype: Note\ntitle: Daily\n---\n\n# {{title}}\n' }, now), '---\ntype: Note\ntitle: "Idea"\n---\n\n# Idea\n');
  assert.match(newPageBody('Idea', { body: '# {{title}}\n' }, now), /^---\ntype: Note\ntitle: "Idea"/);
});

test('template pages come from the templates folder and the journal template folder', () => {
  const paths = ['Templates/Daily.md', 'Templates/index.md', 'Meta/Tpl/Week.md', 'Notes/Templates.md', 'Templates/logo.png', 'Mine/Plan.md'];
  assert.deepEqual(
    templatePagePaths(paths, { templates: { folder: 'Templates' }, journal: { template: 'Meta/Tpl/Week.md' } }),
    ['Meta/Tpl/Week.md', 'Templates/Daily.md'],
  );
  assert.deepEqual(templatePagePaths(paths, { templates: { folder: 'Mine' }, journal: { template: null } }), ['Mine/Plan.md']);
});
