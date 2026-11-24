# CoWiki governance benchmark

The benchmark measures candidate edits against immutable evidence and earlier revisions. The no-maintenance method accepts the complete candidate verbatim. It has no model calls and makes no repairs. Each run reports the initial detection counts, final state and cost for each case, plus metrics by case type. Undefined ratios serialize as JSON null.

From the CoWiki repository on the course machine:

```bash
source /data/zhongqiyu/AIAA5032_cowiki_m2/env.sh
cargo run --release --locked --manifest-path benchmarks/governance-bench/Cargo.toml -- --split dev --verifier none --out /tmp/m2-none.json
```

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
| `remote` | M2 with a batched model classification and local proposal validation. |
| `both` | Full M2. Queue findings from either detector and withhold patches on disputed claims. |

For model arms, enable `--features remote-verifier` and set `COWIKI_VERIFIER_URL` (full chat-completions URL), `COWIKI_VERIFIER_MODEL` (pin the deployment revision), `COWIKI_VERIFIER_CACHE_DIR` and, for authenticated servers, `COWIKI_VERIFIER_API_KEY`. Use `--cache record` to record misses, or `--cache replay` to require disk records. Both successes and terminal provider failures replay without HTTP. Remote and both share the same verification request. A replay counts the original logical call and reports zero physical calls. A transport failure without provider usage conservatively charges the reserved budget and marks the estimate.

All methods receive one immutable, label-blind packet per case. Source retrieval reserves 800 active and 400 retired retrieval tokens. The common packing step alternates active and retired evidence, ranks within those sets by word overlap, and admits up to 12 judgment pairs. It reserves the longer system prompt and uses a conservative upper bound of one token per UTF-8 byte plus chat framing. The per-case ceiling is one call, 4,096 total tokens, 768 completion tokens. An oversized mandatory input yields a pre-dispatch failure. Rows include the packet digest, unresolved findings, timing, usage and errors. The program still writes scored outcomes on method failure and exits unsuccessfully. The issue-code table uses one-to-one exact code and two-sided span matching. Repair success uses the listed acceptable full texts. Unlisted but defensible wording requires independent human adjudication. A primary challenge stratum owns each case's cost.
