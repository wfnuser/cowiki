# CoWiki Retrieval & Governance Benchmark (`cowiki-retrieval-bench`)

Standalone, zero-GUI benchmark suite for CoWiki's production Rust knowledge engine ([`crates/cowiki-knowledge`](../../crates/cowiki-knowledge)).

It executes directly against the same `cowiki_knowledge::{okf, embeddings, knowledge_index}` library compiled into CoWiki Desktop and CoWiki's embedded MCP server — never a mock or duplicate implementation — and serves as an automated CI regression gate.

## What It Measures

1. **Multi-Hop, Anchor, & Paraphrase Retrieval Ablations** (`N = 24`, strict `400` and `800` token budgets)
   - Evaluates 5 real engine configurations over a 40+ page OKF v0.3 engineering corpus with 2-hop Markdown/wikilink chains, `#section-anchor` deep links, a high-degree hub page (testing hub-degree dampening), morphological paraphrases, and retired/deprecated/redirected competitors:
     - `baseline_fts5`: lexical BM25 only (`max_hops=0`, no dense vectors, no OKF governance)
     - `hybrid_full`: BM25 + dense vector RRF + 2-hop anchor-aware link graph + OKF v0.3 governance
     - `ablation_no_graph`: BM25 + dense vector + governance, link expansion disabled
     - `ablation_no_vector`: BM25 + 2-hop link graph + governance, dense vectors disabled
     - `ablation_no_bm25`: dense vector + 2-hop link graph + governance, lexical BM25 disabled
2. **OKF v0.3 Governance Correctness**
   - Retired leakage rate (`0%` invariant across both lexical and hybrid search)
   - 1-hop, 2-hop, and frontmatter-only stub `redirect_to` resolution (`100%` invariant)
   - Active `human_verified` outranking `deprecated` / expired `stale_after` pages (`100%` invariant)
   - Agent-Edit Trust Reset invariance (`verified: false`, `trust_tier: agent_draft`, `generated: true`, custom YAML preserved)
   - `okf::validate_bundle` precision and recall on seeded defects (`100% / 100%`)
3. **Corpus Scaling & Incremental Update Equivalence** (`100`, `500`, `1,000` Markdown pages; up to `4,000` embedded section units)
   - Cold `rebuild_space`, warm `refresh_space`, on-disk SQLite footprint, single-file `index_paths` add/edit/delete latency, query P50/P95 latency, and strict score/order equivalence against a fresh full rebuild.
4. **Multi-Threaded Read Throughput** (`1`, `4`, `8` concurrent OS threads on a 500-page SQLite WAL Space)
   - Measures sustained read QPS, P50/P95 latency, and zero lock errors under parallel `hybrid_search` load.
5. **Optional External Multi-Hop JSONL Evaluator** (`--dataset <path>`)
   - Evaluates CoWiki's production engine on external multi-hop QA datasets (such as HotpotQA or 2WikiMultiHopQA converted to JSONL with `{id, question, supporting_titles, paragraphs: [{title, text, links?}]}`), optionally with remote OpenAI-compatible embeddings (`--embedder remote`).

## Baseline Results

Recorded from [`results/baseline_results.json`](results/baseline_results.json) (OKF v0.3, Schema v4, deterministic offline `local-hash-proj-v1` 128-dim embedder, Apple Silicon release build):

### 1. Retrieval Ablations (Budget = 800 tokens, N = 24)

| Config | Page Recall | Full Multi-Hop | MRR | 2-Hop Full | Paraphrase Recall | Gov MRR | Avg Tokens | Violations | P50 (ms) | P95 (ms) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `baseline_fts5` | 68.75% | 45.83% | 0.8125 | 0.00% | 33.33% | 0.8750 | 111.2 | 0 | 0.12 | 0.17 |
| `ablation_no_graph` | 80.56% | 54.17% | 0.9688 | 12.50% | 100.00% | 1.0000 | 121.7 | 0 | 0.18 | 0.22 |
| `ablation_no_vector` | 83.33% | 83.33% | 0.7917 | 100.00% | 33.33% | 0.8750 | 352.8 | 0 | 0.58 | 0.66 |
| `ablation_no_bm25` | 100.00% | 100.00% | 0.8889 | 100.00% | 100.00% | 0.8333 | 509.7 | 0 | 0.61 | 0.71 |
| **`hybrid_full`** | **100.00%** | **100.00%** | **0.9167** | **100.00%** | **100.00%** | **1.0000** | **505.8** | **0** | **0.70** | **0.78** |

> At a tighter `400`-token budget, `hybrid_full` preserves **100.00%** page recall, **100.00%** full multi-hop coverage, **0.9167** MRR, and **0** budget violations (`346.5` avg tokens).

### 2. OKF v0.3 Governance Correctness

| Check | Result |
|---|---:|
| Retired leakage rate (6 queries, lexical + hybrid) | **0.00%** |
| Redirect resolution rate (1-hop, 2-hop, frontmatter-only stub) | **100.00%** |
| Fresh `human_verified` > `deprecated` / expired `stale_after` | **100.00%** |
| Agent-Edit Trust Reset pass rate (custom YAML preserved) | **100.00%** |
| `validate_bundle` precision / recall (6/6 seeded defects) | **100.00% / 100.00%** |

### 3. Corpus Scaling & Incremental Equivalence

| Pages | Units | Links | Rebuild (ms) | Warm Refresh (ms) | DB Size (KB) | Inc Add / Edit / Del (ms) | Equiv vs Full | Query P50 / P95 (ms) |
|---:|---:|---:|---:|---:|---:|---:|:---:|---:|
| 100 | 400 | 200 | 21.5 | 0.3 | 1,184.0 | 0.37 / 0.34 / 0.17 | PASS | 1.54 / 1.64 |
| 500 | 2,000 | 1,000 | 119.3 | 1.6 | 5,400.0 | 0.42 / 0.35 / 0.21 | PASS | 4.86 / 5.38 |
| 1,000 | 4,000 | 2,000 | 242.7 | 3.2 | 10,660.0 | 0.56 / 0.35 / 0.19 | PASS | 9.38 / 9.53 |

### 4. Concurrent Read Throughput (SQLite WAL, 500-page Space)

| Threads | Queries | Wall (ms) | Throughput (QPS) | P50 (ms) | P95 (ms) | Errors |
|---:|---:|---:|---:|---:|---:|---:|
| 1 | 40 | 190.7 | 209.8 | 4.74 | 5.27 | 0 |
| 4 | 160 | 452.4 | 353.6 | 10.92 | 13.64 | 0 |
| 8 | 320 | 1,854.9 | 172.5 | 45.87 | 53.78 | 0 |

## How to Run

### Default Offline Benchmark (Zero API Keys Required)

```bash
cargo run --manifest-path benchmarks/retrieval-bench/Cargo.toml --release --locked \
  -- --out benchmarks/retrieval-bench/results/baseline_results.json
```

### Remote Dense Embeddings (OpenAI-Compatible API)

```bash
export COWIKI_EMBEDDING_URL="https://api.openai.com/v1/embeddings"
export COWIKI_EMBEDDING_API_KEY="your-key"
export COWIKI_EMBEDDING_MODEL="text-embedding-3-small"

cargo run --manifest-path benchmarks/retrieval-bench/Cargo.toml --release --locked \
  -- --embedder remote --scale-pages 100 --queries-per-thread 10 \
  --out /tmp/cowiki-remote-bench.json
```

### External Multi-Hop Dataset (JSONL)

Each line in the JSONL file should contain:

```json
{"id":"q1","question":"...","supporting_titles":["Page A","Page B"],"paragraphs":[{"title":"Page A","text":"...","links":["Page B"]},{"title":"Page B","text":"..."}]}
```

Run:

```bash
cargo run --manifest-path benchmarks/retrieval-bench/Cargo.toml --release --locked \
  -- --dataset /path/to/multihop.jsonl --out /tmp/cowiki-ext-bench.json
```

### Unit Tests

```bash
cargo test --manifest-path benchmarks/retrieval-bench/Cargo.toml --locked
```
