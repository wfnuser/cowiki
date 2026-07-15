import assert from 'node:assert/strict';
import test from 'node:test';

import { fileIngestResult } from '../src/lib/source-ingest.ts';

test('partial file ingest keeps failed files selected and explains the failures', () => {
  const result = fileIngestResult([
    { sourcePath: '/tmp/good.pdf', source: { filename: 'good.md' }, error: null },
    { sourcePath: '/tmp/broken.docx', source: null, error: 'cannot parse DOCX' },
  ]);

  assert.deepEqual(result.remainingFiles, ['/tmp/broken.docx']);
  assert.equal(result.shouldClose, false);
  assert.match(result.error, /broken\.docx/);
  assert.match(result.error, /cannot parse DOCX/);
});

test('successful file ingest clears the selection and closes the dialog', () => {
  const result = fileIngestResult([
    { sourcePath: '/tmp/good.pdf', source: { filename: 'good.md' }, error: null },
  ]);

  assert.deepEqual(result, { remainingFiles: [], error: '', shouldClose: true });
});
