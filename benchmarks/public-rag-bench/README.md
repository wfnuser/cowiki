# CoWiki Public RAG Benchmark Harness (`benchmarks/public-rag-bench`)

Reproducible, zero-dependency Python harness benchmarking CoWiki retrieval and conflict-arbitration algorithms against public RAG benchmarks (**RAMDocs** and **MultiHop-RAG**), with strict fairness invariants and three-way disjoint stratified slices (`dev100`, blind `val100`, fresh blind `ext100`/`ext200` → combined `med300`/`med400`).

For the full methodology, architectural comparison against **LumiMax** and published baselines, negative ablations, and caveats, see **[`COMPARISON.md`](COMPARISON.md)**.

## Fairness Invariants

- **MultiHop-RAG**: Strictly `<= 256` estimated body tokens per chunk (matching official `yixuantt/MultiHop-RAG` `SentenceSplitter(chunk_size=256)`). Returned chunk text contains ONLY the clean body chunk (never inflated with full articles or extra metadata). Evaluates official `retrieval_evaluate.py` metrics (`Hits@10`, `Hits@4`, `MAP@10`, `MRR@10`) PLUS stricter multi-hop completeness (`AllGold@10`, `EvidenceRecall@10`) and end-to-end QA + `null_query` abstention.
- **RAMDocs**: Identical backbone LLM across controlled baselines and CoWiki Compiled Arbiter; zero dataset examples in prompts; strictly disjoint `val100` (`seed=1337`) and `ext100` (`seed=20261128`) blind slices never inspected during development; `api_errors = 0` across all headline medium-batch runs.

## Verified Medium-Batch Results

### 1. RAMDocs (`med300` = 300 / 500 = 60% of Dataset, `qwen-plus`, `api_errors = 0`)

| Method | `dev100` `EM_strict` | Blind `val100` `EM_strict` | Blind `ext100` `EM_strict` | **`med300` `EM_strict`** | `med300` `EM_paper` | `med300` F1 | MisinfoLeak | Calls/Q |
| --- | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| *Paper Standard RAG (Llama-3.3-70B, full 500)* | — | — | — | *32.60%* | — | — | — | *1.0* |
| *Paper MADAM-RAG (Llama-3.3-70B, full 500)* | — | — | — | *34.40%* | — | — | — | *~18* |
| *LumiMax Published Reference (full 500)* | — | — | — | *45.29%* | — | — | — | — |
| `baseline_standard_rag` (`qwen-plus`) | 35.00% | 40.00% | 47.00% | 40.67% | 45.00% | 0.7601 | 18.00% | 1.0 |
| **`cowiki_compiled_arbiter_v4`** (`qwen-plus`) | **51.00%** | **63.00%** | **65.00%** | **59.67%** | **63.00%** | **0.8254** | **9.00%** | **2.0** |

### 2. MultiHop-RAG Retrieval (`<=256` Body Tokens/Chunk, `med400` = `352` Non-Null + `48` Null)

| Method | `dev100` (88) `Hits@10` | Blind `val100` (88) `Hits@10` | Blind `ext200` (176) `Hits@10` | **`med400` (352) `Hits@10`** | `med400` `Hits@4` | `med400` `MAP@10` | `med400` `MRR@10` | `med400` `AllGold@10` | `med400` `EvRec@10` |
| --- | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| *Paper Best (Dense + bge-reranker-large, full)* | — | — | — | *74.67%* | — | — | *~0.54* | — | — |
| *LumiMax Published Reference (full)* | — | — | — | *92.95%* | — | — | — | — | — |
| `bm25_raw` (offline CPU) | 80.68% | 82.95% | 87.50% | 84.66% | 72.16% | 0.2930 | 0.5877 | 23.58% | 53.60% |
| `bm25_okf_meta` (offline CPU) | 88.64% | 90.91% | 89.77% | 89.77% | 80.68% | 0.3261 | 0.6537 | 27.27% | 57.60% |
| `bm25_okf_lexical_bridge` (offline CPU) | — | — | 89.77% | 88.07% | 79.26% | 0.3287 | 0.6500 | 27.56% | 56.94% |
| `bm25_okf_ppr_slots` (offline CPU) | 88.64% | 89.77% | 92.05% | 90.62% | 76.42% | 0.3020 | 0.5878 | 33.24% | 61.43% |
| `hybrid_dense_ppr` (`text-embedding-v4`) | 89.77% | 93.18% | 93.18% | 92.33% | 80.40% | 0.3230 | 0.6244 | 34.66% | 63.71% |
| **`hybrid_dense_ppr_rerank` (+ `gte-rerank-v2`)** | **90.91%** | **96.59%** | **94.32%** | **94.03%** | **88.07%** | **0.3878** | **0.7299** | **44.89%** | **70.27%** |
| **`hybrid_dual_rerank_fusion`** (`dev100+val100`, 176) | **93.18%** | **96.59%** | — | *94.89%* *(176)* | *90.34%* | *0.4065* | *0.7442* | *45.45%* | *71.50%* |

End-to-End MultiHop-RAG QA on `dev100` (`qwen-plus` Reader, `n=100` incl. 12 `null_query`): `bm25_raw` **68.00%** → `hybrid_multihop_decomposed_rerank` **77.00% (+9.00 pp)** with lower context tokens (`2,334 → 2,275`), `temporal_query` accuracy doubled (`31.82% → 63.64%`), and `null_query` abstention at `91.67%`.

## Quickstart

```bash
# 1. Offline unit tests (0 API calls)
python3 -B -m unittest discover -s tests -v

# 2. Fetch datasets into data/ (gitignored)
python3 -B scripts/fetch_datasets.py --benchmarks ramdocs,multihop_rag

# 3. Generate deterministic stratified slices (checked-in under slices/)
python3 -B scripts/prepare_slices.py --n 100 --seed 42
python3 -B scripts/prepare_heldout_val.py --n 100 --seed 1337
python3 -B scripts/prepare_medium_slices.py

# 4. Run MultiHop-RAG med400 6-tier ablation
export COWIKI_BENCH_API_KEY=...
export COWIKI_BENCH_BASE_URL=https://dashscope.aliyuncs.com/compatible-mode/v1
python3 -B scripts/run_multihop_bench.py \
  --slice slices/multihop_med400.json \
  --methods bm25_raw,bm25_okf_meta,bm25_okf_lexical_bridge,bm25_okf_ppr_slots,hybrid_dense_ppr,hybrid_dense_ppr_rerank

# 5. Run MultiHop-RAG End-to-End QA
python3 -B scripts/run_multihop_qa_bench.py --slice slices/multihop_dev100.json --reader-model qwen-plus

# 6. Run RAMDocs med300 (Standard RAG vs CoWiki Surgical Fork-Diff Arbiter v4)
python3 -B scripts/run_ramdocs_bench.py \
  --slice slices/ramdocs_med300.json \
  --methods baseline_standard_rag,cowiki_compiled_arbiter_v4 \
  --model qwen-plus
```
