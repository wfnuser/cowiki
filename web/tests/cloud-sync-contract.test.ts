import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import test from 'node:test';

const tauri = readFileSync(resolve(import.meta.dirname, '../src-tauri/src/lib.rs'), 'utf8');
const client = readFileSync(resolve(import.meta.dirname, '../src/local-api.ts'), 'utf8');

test('desktop exposes Cloud link, sync, submit, and conflict recovery commands', () => {
  for (const command of [
    'cloud_link_space',
    'cloud_get_status',
    'cloud_sync_if_clean',
    'cloud_submit',
    'cloud_rebase_continue',
    'cloud_rebase_abort',
  ]) {
    assert.match(tauri, new RegExp(`\\b${command}\\b`));
  }
  for (const invoke of [
    "invoke<CloudSyncResult>('cloud_link_space'",
    "invoke<CloudSyncResult>('cloud_get_status'",
    "invoke<CloudSyncResult>('cloud_sync_if_clean'",
    "invoke<CloudSyncResult>('cloud_submit'",
    "invoke<CloudSyncResult>('cloud_rebase_continue'",
    "invoke<CloudSyncResult>('cloud_rebase_abort'",
  ]) {
    assert.ok(client.includes(invoke), `missing ${invoke}`);
  }
});

test('Cloud credentials are not persisted in Git remotes or local link metadata', () => {
  const sync = readFileSync(resolve(import.meta.dirname, '../src-tauri/src/cloud_sync.rs'), 'utf8');
  assert.match(sync, /GIT_CONFIG_VALUE_0/);
  assert.doesNotMatch(sync, /https:\/\/[^\s"']+@/);
  assert.doesNotMatch(sync, /api_key\s*:\s*[^&]/i);
});
