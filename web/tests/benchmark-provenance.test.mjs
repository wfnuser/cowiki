import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { benchmarkProvenance } from '../scripts/benchmark-provenance.mjs';

test('reports identify clean and modified source without inventing a commit outside Git', () => {
  const root = mkdtempSync(join(tmpdir(), 'cowiki-benchmark-provenance-'));
  const git = (...args) => execFileSync('git', ['-c', 'core.hooksPath=/dev/null', '-c', 'commit.gpgsign=false', '-c', 'user.name=Benchmark', '-c', 'user.email=benchmark@example.test', ...args], { cwd: root, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
  try {
    const unknown = benchmarkProvenance(root);
    assert.equal(unknown.source.commit, null);
    assert.equal(unknown.source.dirty, null);
    git('init', '-q');
    git('commit', '--allow-empty', '-qm', 'fixture');
    const clean = benchmarkProvenance(root);
    assert.equal(clean.source.commit, git('rev-parse', 'HEAD'));
    assert.equal(clean.source.dirty, false);
    assert.equal(clean.toolchain.node, process.version);
    writeFileSync(join(root, 'changed.md'), '# Changed');
    const changed = benchmarkProvenance(root);
    assert.equal(changed.source.commit, clean.source.commit);
    assert.equal(changed.source.dirty, true);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
