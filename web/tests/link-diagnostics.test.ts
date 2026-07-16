import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import {
  brokenLinkSummary,
  groupBrokenLinks,
  linkDiagnosticsMode,
} from '../src/lib/link-diagnostics.ts';

const localApi = readFileSync(new URL('../src/local-api.ts', import.meta.url), 'utf8');
const tauriLib = readFileSync(new URL('../src-tauri/src/lib.rs', import.meta.url), 'utf8');
const spacePanel = readFileSync(new URL('../src/components/layout/SpacePanel.tsx', import.meta.url), 'utf8');
const mainLayout = readFileSync(new URL('../src/pages/MainLayout.tsx', import.meta.url), 'utf8');
const linksView = readFileSync(new URL('../src/components/views/LinksView.tsx', import.meta.url), 'utf8');

test('broken-link summaries and groups stay concise', () => {
  assert.equal(brokenLinkSummary(0), 'No broken links');
  assert.equal(brokenLinkSummary(1), '1 broken link');
  assert.equal(brokenLinkSummary(4), '4 broken links');
  assert.deepEqual(groupBrokenLinks([
    { source_path: 'alpha.md', source_title: 'Alpha', target: 'missing-one' },
    { source_path: 'alpha.md', source_title: 'Alpha', target: 'missing-two' },
    { source_path: 'beta.md', source_title: 'Beta', target: 'missing-three' },
  ]), [
    { sourcePath: 'alpha.md', sourceTitle: 'Alpha', targets: ['missing-one', 'missing-two'] },
    { sourcePath: 'beta.md', sourceTitle: 'Beta', targets: ['missing-three'] },
  ]);
});

test('diagnostic states never report a clean result alongside an error', () => {
  assert.equal(linkDiagnosticsMode({ loading: true, error: '', count: 0 }), 'loading');
  assert.equal(linkDiagnosticsMode({ loading: false, error: 'offline', count: 0 }), 'error');
  assert.equal(linkDiagnosticsMode({ loading: false, error: '', count: 0 }), 'clean');
  assert.equal(linkDiagnosticsMode({ loading: false, error: '', count: 2 }), 'broken');
});

test('desktop diagnostics use one registered read-only Tauri command', () => {
  assert.match(localApi, /invoke\('local_list_broken_links', \{ spaceSlug \}\)/);
  assert.match(tauriLib, /fn local_list_broken_links\(/);
  assert.match(tauriLib, /local_list_broken_links,/);
  assert.doesNotMatch(tauriLib, /fn local_(?:repair|fix)_broken_links/);
});

test('the local-only Links view refreshes on demand and window focus without polling', () => {
  assert.match(spacePanel, /showLinkDiagnostics/);
  assert.match(spacePanel, /label: 'Links'/);
  assert.match(mainLayout, /kind: 'links'/);
  assert.match(mainLayout, /<LinksView/);
  assert.match(mainLayout, /showLinkDiagnostics=\{desktop/);
  assert.doesNotMatch(linksView, /setInterval/);
  assert.match(linksView, /addEventListener\('focus'/);
  assert.match(linksView, />\s*Refresh\s*</);
  assert.doesNotMatch(linksView, /A live|live, read-only/i);
  assert.match(linksView, /listBrokenLinks/);
  assert.doesNotMatch(linksView, /writePage|onSave|Auto Save/);
});

test('Links avoids a route that cannot be restored after reload', () => {
  assert.doesNotMatch(mainLayout, /navigate\(`\/\$\{owner\}\/\$\{activeWorkspace\.slug\}\/links`\)/);
  assert.match(mainLayout, /case 'links':[\s\S]*?navigate\('\/', \{ replace: true \}\)/);
});
