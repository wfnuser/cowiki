# CoWiki governance benchmark

The benchmark measures candidate edits against immutable evidence and earlier revisions. The no-maintenance method accepts the complete candidate verbatim. It has no model calls and makes no repairs. Each run reports the initial detection counts, final state and cost for each case, plus metrics by case type. Undefined ratios serialize as JSON null.

## Reproduce the offline development comparison

Run these commands in Bash on the course machine. A fresh machine needs Rust 1.100.0 and the dependencies in the committed Cargo.lock. The fetch step can use the registry. All subsequent build, test and scoring commands work with Cargo offline and require no provider credentials.

```bash
cd /data/zhongqiyu/AIAA5032_cowiki_m2/cowiki
source /data/zhongqiyu/AIAA5032_cowiki_m2/env.sh
G=benchmarks/governance-bench/Cargo.toml
cargo fetch --locked --manifest-path "$G"  # Only for a cold dependency cache
cargo fmt --all --manifest-path "$G" -- --check
cargo clippy --offline --locked --manifest-path "$G" --all-targets --all-features -- -D warnings
cargo test --offline --locked --manifest-path "$G" --all-features
for mode in none rules; do
  cargo run --offline --release --locked --manifest-path "$G" -- \
    --split dev --verifier "$mode" --baseline --out "/tmp/m2-dev-$mode-reference.json"
  cmp "/tmp/m2-dev-$mode-reference.json" "benchmarks/governance-bench/results/dev-$mode.json"
  cargo run --offline --release --locked --manifest-path "$G" -- \
    --split dev --verifier "$mode" --out "/tmp/m2-dev-$mode-timed.json"
done
for slice in tune validation; do
  cargo run --offline --release --locked --manifest-path "$G" -- \
    --split dev --dev-slice "$slice" --verifier rules --out "/tmp/m2-dev-$slice.json"
done
```

The checked-in [no-maintenance](results/dev-none.json) and [rules](results/dev-rules.json) references contain the 64 packet hashes, cited findings, hashes of proposal outcomes and final text, aggregate metrics and deterministic cost counts. The timed reports retain complete outcomes and machine-dependent timings. The `governance-bench` CI job reproduces both references byte for byte. The all-features tests exercise the provider protocol using a loopback server.

The six-family tuning slice has 48 edits. The two-family internal validation slice has 16. The validation families have the two lowest SHA-256 hashes of UTF-8 `issue169-dev-validation-v1/<family-id>`. This partition was fixed before tuning in this round. Validation aggregates were first inspected after selecting the rules. Earlier development rounds used all eight families.

## Data and labels

Each row in the selected split’s `families.jsonl` assigns one source family to a split and contains the evidence pages for that family. `data/dev/cases.jsonl` supplies the editing request, base and candidate text, and ordered revisions where applicable. Only the scorer reads the corresponding `gold.jsonl`. A finding has a machine code and 1-based inclusive line ranges in the candidate and evidence snapshots. The pilot scorer matches findings one-to-one, even for two codes on one claim. It checks final text against explicitly accepted outcomes and tracks protected text.

### Scorer fixtures

The 24 constructed pilot edits under `tests/fixtures/pilot/` are a **fixture sanity check**. The same author created these cases and the offline rules. Exclude pilot scores from all empirical comparisons. These four families are adapted from the Apache-2.0 CoWiki retrieval fixtures at commit `d1d51f5eda92450eee602f85c632ed8debec7a12`, in `../retrieval-bench/src/corpus.rs`. Source locations in that file are lines 34 (ledger idempotency), 38 (trace retention), 51 (wrapping key rotation) and 82 (telemetry batch interval). Active policies paraphrase the original. The retired values, handoff pages and revision histories are constructed examples. Archived policies in the pilot describe the same subject and scope as their active successors.

For every family the pilot includes an unsupported changed interval, a reintroduced retired interval, an explicit historical quotation, a compatible paraphrase, a neutral addition and a conflicting write accompanied by an independent valid addition. A current assertion of the retired interval has two supported labels. A historical quotation with the current rule alongside it is a valid control. All variants and revisions from one family belong to the same split. Duplicate source ownership and invalid citations fail validation.

The test harness reads those fixtures independently of either evaluation split. The benchmark opens only the selected split’s files.

### Blind evaluation protocol

A separate authoring session received the taxonomy, schema, labeling requirements and pinned upstream sources. It had no access to the CoWiki repository, detector, prompts, fixture cases or earlier session history. The released CPython documentation supplies the held-out families. CMake reference documentation supplies the development families. `data/upstream/sources.json` records the upstream URL, Git tag and SHA-256 for each input file. Upstream license texts and copyright notices accompany the adapted data in the same directory.

Reviewers inspect the separate stratified audit sheet. Developers may inspect dev cases. Verify a held-out delivery from its directory with `sha256sum -c SHA256SUMS`. Freeze the method configuration before the single scored held-out run. Keep original data immutable and version any later human adjudication separately.

The independently authored split has 64 development edits in eight CMake command families (29 valid) and 160 held-out edits in sixteen CPython module families (77 valid, 48.1%). The 30-case reviewer export is kept outside the checkout. The full authoring data card and held-out provenance are for reviewers and post-freeze analysis. The SHA-256 of the held-out `SHA256SUMS` is `d5ff4e3c358355c2bceb4c26ff8812081eaeb1f63b741b8c719b4deda3db9397`.

## Course comparisons

| Verifier | Course arm |
|---|---|
| `none` | Accept the candidate verbatim. |
| `single-pass` | Ask the model once for findings and a repair. |
| `rules` | M2 with deterministic classification and bounded scratch repair. |
| `remote` | M2 with batched judgments and optional cited repairs in the same response. |
| `both` | Full M2. Queue findings from either detector. An explicit opposing rule verdict blocks a model repair. A lexical abstention leaves it eligible for the scratch checks. |

For model arms, enable `--features remote-verifier` and set `COWIKI_VERIFIER_URL` (full chat-completions URL), `COWIKI_VERIFIER_MODEL` (pin the deployment revision), `COWIKI_VERIFIER_CACHE_DIR` and, for authenticated servers, `COWIKI_VERIFIER_API_KEY`. Use `--cache record` to record misses, or `--cache replay` to require disk records. Both successes and terminal provider failures replay without HTTP. Remote and both share the same verification request. A replay counts the original logical call and reports zero physical calls. A transport failure without provider usage conservatively charges the reserved budget and marks the estimate.

### One-call cited repairs

The verifier’s strict reply schema attaches a repair to a judgment by its zero-based candidate and evidence indices. Each attachment quotes a unique preimage substring, gives a replacement and cites the supporting evidence entry. A retired finding needs an active, verified correction. The verifier binds the result to the candidate snapshot digest. Single-pass retains its original prompt, reply format and cache key.

M2 validates each attachment independently on a private scratch copy. A replacement must restore a cited assertion or a base qualifier or value corroborated by the cited source. M2 compares deterministic rule findings and complete, freshly indexed Space audits before and after each eligible edit. Any new issue vetoes the edit. Rejected attachments leave their finding in review and suppress the rule fallback on that claim. Every accepted proposal retains its original preimage and exact byte range. Reports include per-case `repair_checks` and aggregate `repair_validation` counts by origin and outcome. Transport failures and over-budget answers are counted separately from repair rejections.

### Model arms on development data

Export the four configuration variables named above in the shell. The binary reads the environment directly. Use a private cache directory and a pinned model for all three arms. Run the same two cases per CMake family in each pilot. Cargo's offline switch controls dependency resolution. A cache miss in record mode still calls the configured provider.

```bash
for mode in single-pass remote both; do
  cargo run --offline --release --locked --features remote-verifier --manifest-path "$G" -- \
    --split dev --dev-slice pilot --verifier "$mode" --cache record --out "/tmp/m2-pilot-$mode.json"
done
```

Before the full run, inspect each pilot's `cost` object for logical and HTTP calls, cache hits, prompt and completion tokens, estimated cases, failures and elapsed time. For the 64-case dev budget, multiply each arm’s 16-case logical usage by four and report the per-case ceiling separately. Distinguish cached logical usage from incremental provider charges. Use the same inputs, per-case limits and cache configuration in the full comparison.

```bash
for mode in single-pass remote both; do
  cargo run --offline --release --locked --features remote-verifier --manifest-path "$G" -- \
    --split dev --verifier "$mode" --cache record --out "/tmp/m2-dev-$mode.json"
done
```

Use `--cache replay` in the same commands to reproduce recorded judgments without contacting a provider. Keep the 30-case label review independent. The single held-out evaluation requires Qiyu’s go-ahead after configuration freeze.

All methods receive one immutable, label-blind packet per case. Source retrieval reserves 800 active and 400 retired retrieval tokens. The common packing step alternates active and retired evidence, ranks within those sets by word overlap, and admits up to 12 judgment pairs. It reserves the longer system prompt and uses a conservative upper bound of one token per UTF-8 byte plus chat framing. The default per-case limits are one call, 4,096 total tokens and 768 completion tokens. A provider with additional session overhead can use `COWIKI_VERIFIER_TOTAL_TOKENS` and `COWIKI_VERIFIER_OUTPUT_TOKENS` for the shared model-arm limits. A revision that names a verified or retired source can reserve space for it when ordinary packing would drop it. To fit the same cap, wire history may reference identical base frontmatter while retaining the original revision body, ID and source path. The registered evaluation used the earlier packing policy recorded in its freeze. `COWIKI_VERIFIER_TIMEOUT_SECS` defaults to 30. Changing only the total ceiling permits rescoring stored provider outputs under that ceiling with zero HTTP calls. A change to the requested output limit changes the request and its cache key. An oversized mandatory input yields a pre-dispatch failure. Rows include the packet digest, unresolved findings, timing, usage and errors. The program still writes scored outcomes on method failure and exits unsuccessfully. The issue-code table uses one-to-one exact code and two-sided span matching. Repair success uses the listed acceptable full texts. Unlisted but defensible wording requires independent human adjudication. A primary challenge stratum owns each case's cost.

The structural rules compare audit reports from private base and candidate snapshots, canceling pre-existing findings. The shared packet includes the link inventory and original redirect metadata needed to interpret local references. All model arms can return cited structural findings. The production hygiene check takes the OKF `generated` flag as authoritative. Cases using a custom document type named “generated” on its own remain visible in the scoring denominator. For stale dates, the validator cites the candidate's date and the explicit run date. A gold label that requires a separate calendar-page citation can score as a miss under exact matching.

### Local worktree invariants on headless Linux

The test-only manifest at `local-engine-tests/Cargo.toml` compiles the desktop's original LocalEngine, mutation lock, execution gate, Git change implementation and integration tests. Only local Git transport is needed. A temporary real Space and linked worktree exercise staging, preimage conflicts, pre-merge trust changes, retry, discard and merge. Run `cargo test --manifest-path benchmarks/governance-bench/local-engine-tests/Cargo.toml --locked --lib local_engine::` for that suite. The existing desktop CI job additionally builds and tests the Tauri shell.
