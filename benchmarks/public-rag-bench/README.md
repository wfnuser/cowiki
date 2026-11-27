# CoWiki Public RAG Benchmark Harness (`benchmarks/public-rag-bench`)

Reproducible, zero-dependency Python harness benchmarking CoWiki retrieval and conflict-arbitration algorithms against published public RAG benchmarks (**RAMDocs** and **MultiHop-RAG**), with strict fairness invariants and disjoint `dev100` / held-out `val100` slices.

## Fairness Invariants

- **MultiHop-RAG**: Strictly `<= 256` estimated body tokens per chunk (matching official `yixuantt/MultiHop-RAG` `SentenceSplitter(chunk_size=256)`). Returned chunk text contains ONLY the clean body chunk (never inflated with full articles or extra metadata). Evaluates official `retrieval_evaluate.py` metrics (`Hits@10`, `Hits@4`, `MAP@10`, `MRR@10`) PLUS stricter multi-hop completeness (`AllGold@10`, `EvidenceRecall@10`) and end-to-end QA + `null_query` abstention.
- **RAMDocs**: Identical backbone LLM across baselines and CoWiki Compiled Arbiter; zero dataset examples in prompts; strictly disjoint `val100` held-out slice (`seed=1337`) never inspected during development.

## Verified Pilot Results (200 Stratified Queries per Benchmark: `dev100` + Blind `val100`)

### 1. RAMDocs (`qwen-plus`, 100 dev + 100 blind held-out val)

| Method | `dev100` EM_strict | Blind `val100` EM_strict | Combined EM_strict | Combined EM_paper | Combined F1 | MisinfoLeak | Calls/Q |
| --- | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| *Paper Standard RAG (Llama-3.3-70B)* | — | — | *32.60%* | — | — | — | 1.0 |
| *Paper MADAM-RAG (Llama-3.3-70B)* | — | — | *34.40%* | — | — | — | ~18 |
| *LumiMax Claimed Reference* | — | — | *45.29%* | — | — | — | — |
| `baseline_standard_rag` (`qwen-plus`) | 35.00% | 40.00% | 37.50% | 42.50% | 0.7366 | 19.00% | 1.0 |
| **`cowiki_compiled_arbiter_v4`** (`qwen-plus`) | **52.00%** | **61.00%** | **56.50%** | **61.00%** | **0.8038** | **10.00%** | **2.0** |

### 2. MultiHop-RAG Retrieval (`<=256` tok fair chunks, 176 non-null queries across `dev100` + blind `val100`)

| Method | `dev100` Hits@10 | Blind `val100` Hits@10 | Combined Hits@10 | Combined Hits@4 | Combined MAP@10 | Combined MRR@10 | Combined AllGold@10 | Combined EvRec@10 |
| --- | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| *Paper Best (Dense + bge-reranker-large)* | — | — | *74.67%* | — | — | *~0.54* | — | — |
| *LumiMax Claimed Reference* | — | — | *92.95%* | — | — | — | — | — |
| `bm25_raw` (offline CPU) | 80.68% | 82.95% | 81.82% | 68.75% | 0.2857 | 0.5732 | 22.73% | 51.89% |
| `bm25_okf_meta` (offline CPU) | 88.64% | 90.91% | 89.77% | 79.55% | 0.3185 | 0.6410 | 27.84% | 57.62% |
| `bm25_okf_ppr_slots` (offline CPU) | 88.64% | 89.77% | 89.20% | 73.30% | 0.2946 | 0.5812 | 34.09% | 60.84% |
| `hybrid_dense_ppr` (`text-embedding-v4`) | 89.77% | 93.18% | 91.48% | 78.41% | 0.3181 | 0.6085 | 36.36% | 63.59% |
| `hybrid_dense_ppr_rerank` (+ `gte-rerank-v2`) | 90.91% | **96.59%** | 93.75% | 88.64% | 0.3833 | 0.7240 | 44.89% | 69.75% |
| `hybrid_multihop_decomposed_rerank` | **95.45%** | 94.32% | **94.89%** | 89.20% | 0.3926 | 0.7148 | 43.75% | 70.60% |
| **`hybrid_dual_rerank_fusion`** | **93.18%** | **96.59%** | **94.89%** | **90.34%** | **0.4065** | **0.7442** | **45.45%** | **71.50%** |

End-to-End MultiHop-RAG QA on `dev100` (`qwen-plus` Reader, n=100 incl. 12 `null_query`): `bm25_raw` **68.00%** → `hybrid_multihop_decomposed_rerank` **77.00% (+9.0pp)** with lower context tokens (`2,334 → 2,275`), `temporal_query` accuracy doubled (`31.82% → 63.64%`), and `null_query` abstention at `91.67%`.

## Quickstart

```bash
# 1. Offline unit tests (0 API calls)
python3 -B -m unittest discover -s tests -v

# 2. Fetch datasets into data/ (gitignored)
python3 -B scripts/fetch_datasets.py --benchmarks ramdocs,multihop_rag

# 3. Generate deterministic stratified dev100 + disjoint val100 slices
python3 -B scripts/prepare_slices.py --n 100 --seed 42
python3 -B scripts/prepare_heldout_val.py --n 100 --seed 1337

# 4. Run MultiHop-RAG full 7-tier ablation
export COWIKI_BENCH_API_KEY=...
export COWIKI_BENCH_BASE_URL=https://dashscope.aliyuncs.com/compatible-mode/v1
python3 -B scripts/run_multihop_bench.py \
  --slice slices/multihop_val100.json \
  --methods bm25_raw,bm25_okf_meta,bm25_okf_ppr_slots,hybrid_dense_ppr,hybrid_dense_ppr_rerank,hybrid_multihop_decomposed_rerank,hybrid_dual_rerank_fusion

# 5. Run MultiHop-RAG End-to-End QA
python3 -B scripts/run_multihop_qa_bench.py --slice slices/multihop_dev100.json --reader-model qwen-plus

# 6. Run RAMDocs Standard RAG vs CoWiki Surgical Fork-Diff Arbiter (v4)
python3 -B scripts/run_ramdocs_bench.py \
  --slice slices/ramdocs_val100.json \
  --methods baseline_standard_rag,cowiki_compiled_arbiter_v4 \
  --model qwen-plus
```
