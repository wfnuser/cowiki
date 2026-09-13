# Desktop baseline — 2026-09-06

Measurements of `dev` engine behavior at `694c0a03d7a1ce4cd79f79c2247254efcb053ad7`
with the fixture-v1 harness in this PR. No production optimization is included.
[Run instructions and definitions](desktop-performance.md),
[all engine samples](benchmarks/2026-09-06-desktop.json), and
[all UI samples/assets](benchmarks/2026-09-06-ui.json) accompany this report.

## Environment and limits

Linux x86_64, kernel `6.6.87.2-microsoft-standard-WSL2`, Intel Core i7-12700H;
a hardened Docker container limited to four CPU cores and 8 GiB RAM. Rust/Cargo
1.98.0, Node 24.20.0, release Rust profile, production Vite build. Disposable
Spaces use the disk-backed verifier environment, not tmpfs. The host is shared,
so these are diagnostic samples, not a controlled hardware performance guarantee.

Each corpus uses one fresh worker process; its index already exists. OS caches
are not flushed. Search has 20 warm samples, mutations/warm opens have five, and
UI helpers have 30. Values below are **p50 / p95 in milliseconds**. One-sample
rows show the same observation twice; five-sample p95 is the maximum. Fake Agent
edits exercise real worktrees without model, network or terminal latency.

## Local engine

| Operation | 100 documents | 1,000 documents | 10,000 documents |
|---|---:|---:|---:|
| Process-cold engine open (one sample) | 27.81 / 27.81 | 36.46 / 36.46 | 105.48 / 105.48 |
| Warm engine open | 1.08 / 1.23 | 9.14 / 9.55 | 89.01 / 98.75 |
| First-use search (one sample) | 1.95 / 1.95 | 13.59 / 13.59 | 133.49 / 133.49 |
| Warm search | 0.28 / 0.43 | 4.05 / 4.51 | 38.64 / 41.50 |
| Auto Save | 28.58 / 32.91 | 145.53 / 209.78 | 1700.66 / 2150.92 |
| Rename | 25.26 / 35.17 | 149.67 / 235.82 | 1676.63 / 2153.96 |
| Delete | 25.34 / 28.68 | 153.02 / 190.16 | 1665.55 / 2255.82 |
| Import text file | 15.15 / 18.55 | 28.76 / 33.26 | 155.02 / 199.30 |
| Working Review/diff | 17.56 / 47.29 | 125.92 / 147.77 | 933.47 / 1190.62 |
| Agent Review list/diff | 6.55 / 8.79 | 16.30 / 21.66 | 77.67 / 127.61 |
| Create Background Change | 11.20 / 23.97 | 80.93 / 90.43 | 693.19 / 968.03 |
| Merge Background Change | 6.02 / 7.84 | 33.97 / 36.94 | 285.30 / 450.86 |
| Discard Background Change | 5.75 / 9.04 | 21.30 / 27.97 | 162.79 / 209.05 |
| Create checkpoint | 2.63 / 4.04 | 6.82 / 7.07 | 42.29 / 50.77 |
| Open History | 3.73 / 6.11 | 33.25 / 35.59 | 298.14 / 367.27 |

| Resource after workflow | 100 documents | 1,000 documents | 10,000 documents |
|---|---:|---:|---:|
| Worker peak RSS (MiB) | 15.78 | 17.97 | 22.36 |
| SQLite + WAL + SHM (MiB) | 0.52 | 4.71 | 43.00 |
| Git repository (MiB) | 0.13 | 0.51 | 4.33 |
| Remaining managed worktrees | 0 | 0 | 0 |

RSS covers the engine worker, not the Tauri WebView or desktop application.
Imports and Git history accumulate over the five workflow iterations. Search and
startup samples precede those mutations.

## Frontend diagnostics

| Operation | 100 pages | 1,000 pages | 10,000 pages |
|---|---:|---:|---:|
| Normalize/filter/sort navigation | 1.29 / 2.41 | 10.34 / 16.85 | 87.74 / 125.98 |
| Find final page | 0.03 / 0.09 | 0.26 / 0.44 | 2.44 / 5.25 |
| Collect Submit paths | 0.01 / 0.05 | 0.13 / 0.18 | 1.35 / 2.55 |

PageReader SSR of the 20-paragraph fixture: 3.27 / 6.35 ms.
The largest JS asset is `index-B-R_Pn38.js`: 1,833,281 bytes raw,
551,244 bytes gzip. Node peak RSS (1071.09 MiB) includes the
Vite SSR loader and asset processing; it is not a renderer-memory estimate.
These timings exclude browser layout/paint, scrolling, IPC and window startup.

## Priorities supported by this baseline

1. Profile the Auto Save/rename/delete call paths first. Their 10,000-document
   medians are around 1.7 seconds; working Review/diff is around 0.93 seconds.
   Attribute time to repository preparation, filesystem scans and Git operations
   before choosing caching or changing lock scope. This single-Space sequential
   workload does not establish cross-Space lock contention.
2. Profile navigation normalization/sorting and its call frequency in MainLayout.
   At 10,000 pages the helper alone costs about 88 ms median. Measure an actual
   browser trace before deciding how much memoization or virtualization is needed.
3. Investigate route/panel chunking using the approximately 1.8 MB entry asset and
   a browser startup trace. File size is evidence of payload cost, not measured
   loading time or frame rate.
4. Keep startup/search work proportional to the evidence: lazy index startup is
   already implemented by #129. The current healthy-index engine open is about
   105 ms process-cold and warm search about 39 ms at 10,000 documents. Measure
   missing-index repair separately before another indexing change.
5. All managed worktrees were cleaned in this successful workflow. Failure-path
   cleanup, concurrent editing and long-lived memory still need dedicated runs;
   zero leftovers here does not prove those cases.

Repeat with more samples on the intended macOS desktop before setting budgets or
claiming an improvement. Preserve Markdown/Git authority, stale-write checks and
review isolation in any follow-up optimization.
