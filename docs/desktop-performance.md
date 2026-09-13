# Desktop performance measurements

This benchmark measures the current local engine rather than the retired server
compiler. It changes no production locking, indexing or Git behavior. Startup
already uses lazy index reconciliation (PR #129).

## Run

Use the same Node/Rust toolchain and Tauri platform dependencies as normal
contribution checks. The Rust suite contains a small offline workflow smoke test;
large measurements are opt-in and should run on an otherwise idle machine.

```sh
cd web
npm ci
npm run build
npm run bench:ui
cargo test --release --locked --manifest-path src-tauri/Cargo.toml local_engine::benchmarks::desktop_benchmark -- --ignored --exact --nocapture
```

To save machine-readable reports, set `COWIKI_BENCH_OUTPUT` for Rust and
`COWIKI_BENCH_UI_OUTPUT` for the UI command. Parent directories must already exist.
Use temporary paths; do not commit private Spaces or machine-local runtime files.

Defaults are 100, 1,000 and 10,000 generated documents, 20 search samples and five
mutation samples. For a quick check, `COWIKI_BENCH_SIZES=100` limits the corpus.
`COWIKI_BENCH_SEARCH_SAMPLES`, `COWIKI_BENCH_MUTATION_SAMPLES` and
`COWIKI_BENCH_UI_SAMPLES` adjust repetitions within documented code bounds.
Do not compare debug and release measurements.

New reports use schema version 2. Both engine and UI JSON embed `source.commit`,
`source.dirty`, and `toolchain` entries for Rust, Cargo and Node. They record the
effective corpus sizes and sample counts in `settings`; the UI also records its
warmup/rendering settings. These values are captured before measurement. Missing
Git/toolchain information is `null`, never substituted with a guessed revision.
Run through the documented Cargo/npm commands in the checkout being measured,
and rebuild `dist` before comparing UI asset sizes. A dirty checkout needs its
patch preserved alongside the report; its commit alone does not identify it.
The 2026-09-06 schema-v1 samples are retained as historical evidence and have not
been relabeled with a newer checkout or toolchain.

## What is measured

Each corpus consists of deterministic Markdown pages with frontmatter, eight
paragraphs and a cross-page link, arranged in groups of 100. Fixture creation,
initial import/indexing and the initial Git commit happen before timing. Each
corpus then runs in a fresh child process against its own temporary Space.

- Process-cold startup: the first `LocalEngine::open` in that child, using an
  existing healthy index. This excludes process spawn and Tauri window startup.
- Warm startup: subsequent engine opens in the same process.
- First-use and warm search through the public local search API.
- Auto Save with the same mutation lock and stale-content check as desktop IPC.
- Rename, delete and actual file import.
- Working diffs, Agent Review lists, History and checkpoint creation.
- Background Change creation, merge and discard, using deterministic file edits
  in a real managed Git worktree as a fake Agent.
- Process peak RSS, SQLite database/WAL/SHM bytes, Git bytes, and leftover managed
  worktrees after successful cleanup.

Mutation samples perform a realistic sequential workflow: imports and history
accumulate. Assertions verify that saves do not create commits, merged content
appears in the Draft, discarded content does not, and worktrees are cleaned.
Missing results or broken operations fail the command instead of producing a
misleading timing report. No model, PTY or Cloud call is made.

OS filesystem caches are **not** evicted. “Process cold” does not mean disk cold.
RSS is the worker's lifetime high-water mark, not per-operation allocation.
Percentiles use nearest rank, and reports retain raw samples. With five mutation
samples, p95 is the slowest sample; use more repetitions before a performance claim.

The UI benchmark measures real navigation helpers on flat 100/1,000/10,000-page
lists, PageReader server rendering for a representative page, and raw/gzip sizes
of production JS/CSS assets. It is a diagnostic baseline: browser layout/paint,
interactive list scrolling, startup window latency and IPC require a separate
browser/desktop profiler recording. SSR numbers do not predict frame rate.

## Comparing results

Record the source commit, Rust/Node versions, OS/architecture, CPU and memory
limits, build profile, fixture version and sample counts. Run before and after on
the same machine and compare distributions, not a single favorable sample. Keep
SQLite derived, preserve Git/Markdown authority, and retain stale-write/conflict
checks when an optimization is justified by measurements.

See the [recorded baseline](desktop-performance-baseline.md) for measured results and priorities.
