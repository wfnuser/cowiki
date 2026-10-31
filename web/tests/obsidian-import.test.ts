import assert from 'node:assert/strict';
import test from 'node:test';

import {
  importFolderName,
  importReportMarkdown,
  importSpaceConfig,
  importWarnings,
  planImport,
  type VaultScan,
} from '../src/lib/obsidian-import.ts';
import { parseSpaceFeatures } from '../src/lib/space-features.ts';

function scan(overrides: Partial<VaultScan> = {}): VaultScan {
  return {
    vaultPath: '/vaults/TheVault',
    vaultName: 'TheVault',
    markdownFiles: 5,
    attachmentFiles: 2,
    otherFiles: 1,
    folders: 3,
    totalBytes: 2048,
    attachmentTypes: { png: 2 },
    wikilinks: 10,
    markdownLinks: 1,
    embeds: 2,
    headingLinks: 1,
    blockLinks: 0,
    blockIds: 0,
    unresolvedLinks: 0,
    unresolvedExamples: [],
    tags: 4,
    taskLines: 3,
    filesWithFrontmatter: 2,
    invalidFrontmatter: [],
    callouts: 0,
    dataviewBlocks: 0,
    tasksQueryBlocks: 0,
    templaterFiles: [],
    excalidrawFiles: 0,
    canvasFiles: 0,
    duplicateNames: [],
    duplicateNameCount: 0,
    reservedNames: [],
    nonUtf8Files: [],
    topLevelSourcesFolder: false,
    symlinksSkipped: 0,
    largeFiles: 0,
    largeFileBytes: 0,
    largeFileExamples: [],
    markdownPaths: ['2026-10-29.md', '2026-10-30.md', '2026-10-31.md', 'Projects/a.md', 'Daily/2026-01-01.md'],
    dailyNotes: null,
    templatesFolder: null,
    attachmentFolder: null,
    linkFormat: null,
    communityPlugins: [],
    ...overrides,
  };
}

test('journal settings come from Obsidian when present', () => {
  const plan = planImport(scan({ dailyNotes: { folder: 'Daily', format: 'YYYY-MM-DD', template: 'Templates/Day' } }));
  assert.equal(plan.journalSource, 'obsidian-settings');
  assert.deepEqual(plan.journal, { folder: 'Daily', format: 'YYYY-MM-DD', template: 'Templates/Day.md' });
  assert.equal(plan.journalEntries, 1);
});

test('without settings, the folder with date-named notes is detected', () => {
  const plan = planImport(scan());
  assert.equal(plan.journalSource, 'detected');
  assert.equal(plan.journal.folder, '');
  assert.equal(plan.journalEntries, 3);
  const fallback = planImport(scan({ markdownPaths: ['a.md'] }));
  assert.equal(fallback.journalSource, 'default');
  assert.equal(fallback.journal.folder, 'journal');
});

test('the Space config enables modules and records the import', () => {
  const plan = planImport(scan());
  const config = importSpaceConfig(plan, scan());
  assert.deepEqual(parseSpaceFeatures(config).journal, plan.journal);
  assert.equal(parseSpaceFeatures(config).review.folder, plan.journal.folder);
  assert.equal((config.import as Record<string, unknown>).vaultName, 'TheVault');
});

test('warnings explain risky content', () => {
  assert.deepEqual(importWarnings(scan()), []);
  const risky = importWarnings(scan({
    invalidFrontmatter: ['a.md'],
    reservedNames: ['x/index.md'],
    topLevelSourcesFolder: true,
    dataviewBlocks: 2,
    unresolvedLinks: 3,
  }));
  assert.equal(risky.filter((warning) => warning.level === 'warning').length, 3);
  assert.ok(risky.some((warning) => warning.text.includes('Dataview')));
});

test('the report summarises counts, journal and problems', () => {
  const vault = scan({
    unresolvedLinks: 25,
    unresolvedExamples: ['a.md: [[Missing]]'],
    duplicateNames: [{ name: 'readme', paths: ['A/README.md', 'B/README.md'] }],
    duplicateNameCount: 1,
    communityPlugins: ['dataview'],
    largeFiles: 3,
    largeFileBytes: 30 * 1024 * 1024,
    largeFileExamples: ['talk.wav'],
  });
  const report = importReportMarkdown(vault, planImport(vault), new Date(2026, 9, 31), '/Users/me/cowiki/TheVault (CoWiki)');
  assert.match(report, /^---\ntype: Note\ntitle: "CoWiki Import Report"/);
  assert.match(report, /\| Markdown notes \| 5 \|/);
  assert.match(report, /imported on 2026-10-31/);
  assert.match(report, /3 existing entries recognised/);
  assert.match(report, /…and 24 more/);
  assert.match(report, /\*\*readme\*\*: `A\/README.md`, `B\/README.md`/);
  assert.match(report, /`dataview`/);
  assert.match(report, /3 files of 5 MB or more \(30\.0 MB/);
  assert.match(report, /`talk.wav`\n {2}- …and 2 more/);
});

test('import folder names are filesystem-safe', () => {
  assert.equal(importFolderName('Work/Notes: 2026'), 'Work-Notes- 2026 (CoWiki)');
  assert.equal(importFolderName('  '), 'Obsidian (CoWiki)');
});
