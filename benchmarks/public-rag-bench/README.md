# CoWiki Public RAG Benchmark Harness (`benchmarks/public-rag-bench`)

Reproducible, zero-dependency Python harness benchmarking CoWiki retrieval & conflict-arbitration algorithms against published public RAG baselines (starting with **RAMDocs** and **MultiHop-RAG**, with strict apples-to-apples fairness guarantees).

## Benchmarks Included (Phase 0)

| Benchmark | Source | Official Metric(s) | Fairness Invariant |
| --- | --- | --- | --- |
| **MultiHop-RAG** | `yixuantt/MultiHopRAG` (2,556 queries, 609 news articles) | `Hits@10`, `Hits@4`, `MAP@10`, `MRR@10` (+ stricter `AllGold@10`, `EvidenceRecall@10`, `avg_top10_tokens`) | Strictly `<= 256` body tokens per chunk (matching official `SentenceSplitter(chunk_size=256)`); clean body-only returned text |
| **RAMDocs** | `HanNight/RAMDocs` (500 conflicting-evidence queries) | `EM_strict`, `EM_paper`, `Precision`, `Recall`, `F1` (+ `MisinfoLeakRate`, `FullGoldRecallRate`, token/call ledger) | Identical backbone LLM across baselines and CoWiki Compiled Arbiter; zero ground-truth leakage |

## Quickstart

```bash
# 1. Run offline unit tests (0 API calls)
python3 -B -m unittest discover -s tests -v

# 2. Fetch public datasets into data/ (gitignored)
python3 -B scripts/fetch_datasets.py --benchmarks ramdocs,multihop_rag

# 3. Generate deterministic stratified 100-question dev slices (seed=42)
python3 -B scripts/prepare_slices.py --n 100 --seed 42

# 4. Run 100% offline MultiHop-RAG ablations (SQLite FTS5 + OKF Metadata + Entity-Passage PPR + Multi-Slot)
python3 -B scripts/run_multihop_bench.py --slice slices/multihop_dev100.json

# 5. Run online Hybrid Dense + PPR + Rerank on MultiHop-RAG (caches all API calls in .cache/)
export COWIKI_BENCH_API_KEY=...
export COWIKI_BENCH_BASE_URL=https://dashscope.aliyuncs.com/compatible-mode/v1
python3 -B scripts/run_multihop_bench.py \
  --slice slices/multihop_dev100.json \
  --methods bm25_raw,bm25_okf_meta,bm25_okf_ppr_slots,hybrid_dense_ppr,hybrid_dense_ppr_rerank

# 6. Run RAMDocs Standard RAG vs CoWiki Compiled Arbiter
python3 -B scripts/run_ramdocs_bench.py \
  --slice slices/ramdocs_dev100.json \
  --model qwen-plus
```
