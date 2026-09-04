import assert from 'node:assert/strict';
import test from 'node:test';

import { reviewPreviewPanes } from '../src/lib/review-preview.ts';

test('rendered Review preview compares complete Markdown before and after content', () => {
  assert.deepEqual(reviewPreviewPanes({
    path: 'wiki/topic.md',
    old_content: '---\ntitle: Old\n---\n\n# Old\n',
    new_content: '---\ntitle: New\n---\n\n# New\n',
  }), [
    { key: 'before', label: 'Before', content: '# Old\n' },
    { key: 'after', label: 'After', content: '# New\n' },
  ]);
});

test('rendered Review preview uses one pane for added and deleted Markdown', () => {
  assert.deepEqual(reviewPreviewPanes({
    path: 'wiki/added.md',
    old_content: null,
    new_content: '# Added\n',
  }), [
    { key: 'after', label: 'Added', content: '# Added\n' },
  ]);
  assert.deepEqual(reviewPreviewPanes({
    path: 'wiki/deleted.md',
    old_content: '# Deleted\n',
    new_content: null,
  }), [
    { key: 'before', label: 'Deleted', content: '# Deleted\n' },
  ]);
});

test('rendered Review preview is unavailable for non-Markdown or binary changes', () => {
  assert.deepEqual(reviewPreviewPanes({
    path: 'diagram.png',
    old_content: null,
    new_content: null,
    is_binary: true,
  }), []);
  assert.deepEqual(reviewPreviewPanes({
    path: 'report.html',
    old_content: '<h1>Before</h1>',
    new_content: '<h1>After</h1>',
  }), []);
});
