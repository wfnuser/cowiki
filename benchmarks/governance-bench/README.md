# CoWiki governance benchmark

The development pilot measures candidate edits against immutable evidence and earlier revisions. The no-maintenance method accepts the complete candidate verbatim. It has no model calls and makes no repairs. Each run reports the initial detection counts, final state and cost for each case, plus metrics by case type. Undefined ratios serialize as JSON null.

From the CoWiki repository on the course machine:

```bash
source /data/zhongqiyu/AIAA5032_cowiki_m2/env.sh
cargo run --release --locked --manifest-path benchmarks/governance-bench/Cargo.toml -- --split dev --verifier none --out /tmp/m2-none.json
```

## Data and labels

Each row in `data/families.jsonl` assigns one source family to a split and contains the evidence pages for that family. `data/dev/cases.jsonl` supplies the editing request, base and candidate text, and ordered revisions where applicable. Only the scorer reads the corresponding `gold.jsonl`. A finding has a machine code and 1-based inclusive line ranges in the candidate and evidence snapshots. The pilot scorer matches findings one-to-one, even for two codes on one claim. It checks final text against explicitly accepted outcomes and tracks protected text.

The 24 constructed dev edits across four families are adapted from the Apache-2.0 CoWiki retrieval fixtures at commit `d1d51f5eda92450eee602f85c632ed8debec7a12`, in `../retrieval-bench/src/corpus.rs`. Source locations in that file are lines 34 (ledger idempotency), 38 (trace retention), 51 (wrapping key rotation) and 82 (telemetry batch interval). Active policies paraphrase the original. The retired values, handoff pages and revision histories are constructed examples. Archived policies in the pilot describe the same subject and scope as their active successors.

For every family the pilot includes an unsupported changed interval, a reintroduced retired interval, an explicit historical quotation, a compatible paraphrase, a neutral addition and a conflicting write accompanied by an independent valid addition. A current assertion of the retired interval has two supported labels. A historical quotation with the current rule alongside it is a valid control. All variants and revisions from one family belong to the same split. Duplicate source ownership and invalid citations fail validation.

These development cases establish the scorer and no-maintenance reference. Held-out data and model-backed results are pending.
