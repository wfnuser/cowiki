import assert from 'node:assert/strict';
import test from 'node:test';

import {
  StaleDocumentError,
  VersionedDocument,
  applyAgentEditWithRetry,
} from '../src/lib/versioned-document.ts';

test('a stale writer cannot replace newer human text', () => {
  const document = new VersionedDocument('base');
  const agentSnapshot = document.snapshot();

  const humanResult = document.replace({
    content: 'base + human',
    expectedRevision: agentSnapshot.revision,
    writer: 'human',
  });

  assert.equal(humanResult.revision, 1);
  assert.throws(
    () => document.replace({
      content: 'base + agent',
      expectedRevision: agentSnapshot.revision,
      writer: 'agent',
    }),
    (error) => {
      assert.ok(error instanceof StaleDocumentError);
      assert.deepEqual(error.latest, humanResult);
      return true;
    },
  );
  assert.deepEqual(document.snapshot(), humanResult);
});

test('an agent re-reads and rewrites its edit after a human changes the document', async () => {
  const document = new VersionedDocument('base');
  let firstWrite = true;
  let proposals = 0;

  const result = await applyAgentEditWithRetry({
    read: async () => document.snapshot(),
    propose: async (snapshot) => {
      proposals += 1;
      return `${snapshot.content} + agent`;
    },
    write: async (edit) => {
      if (firstWrite) {
        firstWrite = false;
        document.replace({
          content: 'base + human',
          expectedRevision: document.snapshot().revision,
          writer: 'human',
        });
      }
      return document.replace(edit);
    },
  });

  assert.equal(proposals, 2);
  assert.equal(result.content, 'base + human + agent');
  assert.equal(result.writer, 'agent');
  assert.deepEqual(document.snapshot(), result);
});
