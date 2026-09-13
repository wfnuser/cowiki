import assert from 'node:assert/strict';
import { readFile, readdir, writeFile } from 'node:fs/promises';
import { performance } from 'node:perf_hooks';
import { fileURLToPath } from 'node:url';
import { gzipSync } from 'node:zlib';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { createServer } from 'vite';
import { benchmarkProvenance } from './benchmark-provenance.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));
const samples = Number(process.env.COWIKI_BENCH_UI_SAMPLES ?? 30);
assert(Number.isInteger(samples) && samples > 0 && samples <= 1000, 'samples must be 1–1000');
const provenance = benchmarkProvenance(root);
const sizes = [100, 1000, 10000];
const vite = await createServer({ root, appType: 'custom', logLevel: 'silent', server: { middlewareMode: true, watch: null } });

function measure(action) {
  action(); // Warm module and JIT initialization outside the samples.
  const values = [];
  for (let index = 0; index < samples; index++) {
    const start = performance.now();
    action();
    values.push(performance.now() - start);
  }
  values.sort((left, right) => left - right);
  return { unit: 'ms', samples, p50: values[Math.ceil(samples * 0.50) - 1], p95: values[Math.ceil(samples * 0.95) - 1], values };
}

try {
  const { visiblePageTree, findConcept, submitConceptPaths } = await vite.ssrLoadModule('/src/lib/okf-pages.ts');
  const { PageReader } = await vite.ssrLoadModule('/src/components/PageReader.tsx');
  const corpora = sizes.map((documents) => {
    const pages = Array.from({ length: documents }, (_, index) => ({
      slug: `topic-${index}`, path: `topic-${index}.md`, title: `Topic ${index}`,
      summary: 'Local Markdown and Git knowledge', kind: 'page', branch: 'main', children: [],
    }));
    const last = `topic-${documents - 1}`;
    const tree = visiblePageTree(pages);
    assert.equal(tree.length, documents);
    assert(findConcept(tree, last));
    assert.equal(submitConceptPaths(tree).length, documents);
    return {
      documents,
      visible_page_tree: measure(() => visiblePageTree(pages)),
      find_last_concept: measure(() => findConcept(tree, last)),
      collect_submit_paths: measure(() => submitConceptPaths(tree)),
    };
  });
  const body = '# Benchmark page\n\n' + 'Local Markdown keeps knowledge portable. [Related](topic-1.md)\n\n'.repeat(20);
  const pageRender = measure(() => renderToStaticMarkup(React.createElement(PageReader, { body })));
  const assets = [];
  for (const filename of (await readdir(new URL('../dist/assets/', import.meta.url))).sort()) {
    if (!/\.(js|css)$/.test(filename)) continue;
    const content = await readFile(new URL(`../dist/assets/${filename}`, import.meta.url));
    assets.push({ filename, bytes: content.length, gzip_bytes: gzipSync(content).length });
  }
  const report = {
    schema_version: 2, ...provenance, node: process.version, os: process.platform, arch: process.arch,
    settings: { sizes, samples, fixture_version: 1, warmup_iterations: 1, page_reader_paragraphs: 20, latency_percentile: 'nearest-rank', rendering: 'SSR', assets: 'dist/assets' },
    scope: 'Node timings of production navigation helpers and PageReader SSR; excludes browser layout, paint, IPC and PTY.',
    corpora, page_reader_ssr: pageRender, assets,
    max_rss_bytes: process.resourceUsage().maxRSS * 1024,
  };
  const text = JSON.stringify(report, null, 2);
  if (process.env.COWIKI_BENCH_UI_OUTPUT) await writeFile(process.env.COWIKI_BENCH_UI_OUTPUT, text);
  console.log(text);
} finally {
  await vite.close();
}
