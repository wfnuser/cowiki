import assert from 'node:assert/strict';
import test from 'node:test';

import { pageLineage, sourceFilename, sourceOriginalUrl } from '../src/lib/page-lineage.ts';

test('page lineage reads portable Source references from OKF frontmatter', () => {
  const lineage = pageLineage(`---
type: Note
title: Durable knowledge
sources:
  - .cowiki/sources/interview.md
  - "sources/research/paper.md"
  - ../outside.md
---

# Durable knowledge
`);

  assert.deepEqual(lineage.sources, [
    '.cowiki/sources/interview.md',
    '.cowiki/sources/research/paper.md',
  ]);
  assert.equal(sourceFilename(lineage.sources[1]), 'research/paper.md');
});

test('page lineage accepts a single Source and combines Git provenance', () => {
  const lineage = pageLineage(
    `---
type: Note
sources: source.md
---
`,
    {
      commit: {
        oid: '0123456789abcdef',
        summary: 'Compile the interview',
        author: 'Qinghao',
        committedAt: 1_725_000_000,
      },
      agents: [{ name: 'Codex', changeId: 'change-1', task: 'Organize interview' }],
      review: { id: 'review-id', number: 12, title: 'Compile interview' },
    },
  );

  assert.deepEqual(lineage.sources, ['.cowiki/sources/source.md']);
  assert.equal(lineage.commit?.oid, '0123456789abcdef');
  assert.equal(lineage.agents[0]?.name, 'Codex');
  assert.equal(lineage.review?.number, 12);
});

test('page lineage ignores unsafe paths and malformed frontmatter', () => {
  assert.deepEqual(pageLineage('---\nsources: [/wiki/page.md, https://example.com]\n---\n').sources, []);
  assert.deepEqual(pageLineage('# Plain Markdown').sources, []);
});

test('Source provenance exposes only safe original web URLs', () => {
  assert.equal(
    sourceOriginalUrl('---\nsource_url: "https://example.com/article"\n---\n'),
    'https://example.com/article',
  );
  assert.equal(sourceOriginalUrl('---\nsource_url: javascript:alert(1)\n---\n'), null);
});
