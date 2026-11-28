#!/usr/bin/env python3
"""Run MultiHop-RAG retrieval ablations (fair <=256-token chunks, official retrieval_evaluate metrics)."""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import os
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, List, Optional

BENCH_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(BENCH_ROOT / "src"))

from cowiki_bench.datasets import load_multihop_rag, load_slice_indices
from cowiki_bench.llm_client import OpenAICompatibleClient, PersistentApiCache, load_secret_from_env_or_file
from cowiki_bench.metrics import evaluate_multihop_retrieval
from cowiki_bench.multihop_index import MultiHopHybridIndex


ONLINE_METHODS = {
    "hybrid_dense_ppr",
    "hybrid_dense_ppr_rerank",
    "hybrid_multihop_decomposed_rerank",
    "hybrid_dual_rerank_fusion",
    "hybrid_iterative_bridge_fusion",
}


def main() -> None:
    parser = argparse.ArgumentParser(description="Run MultiHop-RAG retrieval benchmark.")
    parser.add_argument(
        "--slice",
        type=Path,
        default=BENCH_ROOT / "slices/multihop_dev100.json",
        help="Path to stratified slice JSON (ignored if --full is set).",
    )
    parser.add_argument("--full", action="store_true", help="Evaluate all 2,556 queries (2,255 non-null).")
    parser.add_argument(
        "--methods",
        default="bm25_raw,bm25_okf_meta,bm25_okf_ppr_slots",
        help="Comma-separated methods: bm25_raw, bm25_okf_meta, bm25_okf_ppr_slots, hybrid_dense_ppr, hybrid_dense_ppr_rerank",
    )
    parser.add_argument("--max-chunk-tokens", type=int, default=256)
    parser.add_argument("--overlap-tokens", type=int, default=32)
    parser.add_argument("--base-url", default=os.environ.get("COWIKI_BENCH_BASE_URL", "https://dashscope.aliyuncs.com/compatible-mode/v1"))
    parser.add_argument("--embed-model", default=os.environ.get("COWIKI_BENCH_EMBED_MODEL", "text-embedding-v4"))
    parser.add_argument("--embed-dim", type=int, default=512)
    parser.add_argument("--rerank-model", default=os.environ.get("COWIKI_BENCH_RERANK_MODEL", "gte-rerank-v2"))
    parser.add_argument("--decompose-model", default=os.environ.get("COWIKI_BENCH_DECOMPOSE_MODEL", "qwen-flash"))
    parser.add_argument("--workers", type=int, default=6, help="Parallel worker threads for online methods.")
    parser.add_argument("--env-file", type=Path, default=None, help="Optional .env file for API key lookup.")
    parser.add_argument("--out", type=Path, default=None, help="Output summary JSON path.")
    args = parser.parse_args()

    q_path = BENCH_ROOT / "data/multihop_rag/MultiHopRAG.json"
    c_path = BENCH_ROOT / "data/multihop_rag/corpus.json"
    if not q_path.exists() or not c_path.exists():
        raise SystemExit("MultiHop-RAG dataset not found. Run: python3 -B scripts/fetch_datasets.py --benchmarks multihop_rag")

    queries, corpus = load_multihop_rag(q_path, c_path)
    if args.full:
        eval_queries = queries
        slice_label = "full_2556"
    else:
        indices = load_slice_indices(args.slice)
        eval_queries = [queries[i] for i in indices]
        slice_label = args.slice.stem

    methods = [m.strip() for m in args.methods.split(",") if m.strip()]
    needs_online = any(m in ONLINE_METHODS for m in methods)

    client: Optional[OpenAICompatibleClient] = None
    cache: Optional[PersistentApiCache] = None
    if needs_online:
        api_key = load_secret_from_env_or_file(
            ["COWIKI_BENCH_API_KEY", "DASHSCOPE_API_KEY", "OPENAI_API_KEY", "LLM_API_KEY"],
            env_file=args.env_file,
        )
        if not api_key:
            raise SystemExit(
                "Online method requested but no API key found. "
                "Set COWIKI_BENCH_API_KEY / DASHSCOPE_API_KEY / OPENAI_API_KEY or pass --env-file."
            )
        cache = PersistentApiCache(BENCH_ROOT / ".cache/multihop_api_cache.sqlite")
        client = OpenAICompatibleClient(api_key=api_key, base_url=args.base_url, cache=cache)

    print(f"[index] Building fair <={args.max_chunk_tokens}-token MultiHopHybridIndex over {len(corpus)} articles...")
    index = MultiHopHybridIndex(
        corpus=corpus,
        max_chunk_tokens=args.max_chunk_tokens,
        overlap_tokens=args.overlap_tokens,
    )
    max_obs_tokens = max(c.body_tokens for c in index.chunks)
    print(
        f"[index] Ready: {len(index.chunks)} chunks (max_body_tokens={max_obs_tokens}), "
        f"{len(index.entity_to_chunks)} graph entities, build={index.build_ms:.1f} ms"
    )

    dense_build_ms = None
    if needs_online and client is not None:
        print(f"[dense] Embedding {len(index.chunks)} contextual chunks with {args.embed_model} (dim={args.embed_dim})...")
        dense_build_ms = index.build_dense_index(
            client=client,
            embed_model=args.embed_model,
            dimensions=args.embed_dim,
        )
        print(f"[dense] Done in {dense_build_ms:.1f} ms | cache stats: {client.stats}")

    results_by_method: Dict[str, Any] = {}
    for method in methods:
        t0 = time.perf_counter()

        def _eval_one(q: Dict[str, Any]) -> Dict[str, Any]:
            qt0 = time.perf_counter()
            retrieved = index.retrieve(
                query=q["query"],
                method=method,
                top_k=10,
                client=client,
                embed_model=args.embed_model if method in ONLINE_METHODS else None,
                embed_dim=args.embed_dim if method in ONLINE_METHODS else None,
                rerank_model=args.rerank_model if method in ("hybrid_dense_ppr_rerank", "hybrid_multihop_decomposed_rerank", "hybrid_dual_rerank_fusion", "hybrid_iterative_bridge_fusion") else None,
                decompose_model=args.decompose_model,
            )
            lat_ms = (time.perf_counter() - qt0) * 1000.0
            return {
                "idx": q["_idx"],
                "query": q["query"],
                "question_type": q["question_type"],
                "retrieval_list": retrieved,
                "gold_list": q.get("evidence_list", []),
                "latency_ms": lat_ms,
            }

        records: List[Dict[str, Any]] = []
        if method in ONLINE_METHODS and args.workers > 1:
            with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as pool:
                records = list(pool.map(_eval_one, eval_queries))
        else:
            records = [_eval_one(q) for q in eval_queries]
        latencies_ms = [r["latency_ms"] for r in records]
        wall_sec = time.perf_counter() - t0
        metrics = evaluate_multihop_retrieval(records)
        metrics["avg_query_latency_ms"] = round(sum(latencies_ms) / max(1, len(latencies_ms)), 2)
        metrics["p95_query_latency_ms"] = round(sorted(latencies_ms)[int(0.95 * (len(latencies_ms) - 1))], 2)
        metrics["wall_time_sec"] = round(wall_sec, 2)
        results_by_method[method] = metrics

        print(
            f"\n[{method}] valid={metrics['valid_queries']} | "
            f"Hits@10={metrics['Hits@10']:.4f} | Hits@4={metrics['Hits@4']:.4f} | "
            f"MAP@10={metrics['MAP@10']:.4f} | MRR@10={metrics['MRR@10']:.4f} | "
            f"AllGold@10={metrics['AllGold@10']:.4f} | EvRec@10={metrics['EvidenceRecall@10']:.4f} | "
            f"avg_tok={metrics['avg_top10_tokens']:.0f} | p50/avg={metrics['avg_query_latency_ms']:.1f}ms"
        )

    summary = {
        "benchmark": "MultiHop-RAG",
        "generated_at_utc": datetime.now(timezone.utc).isoformat(),
        "slice": slice_label,
        "corpus_articles": len(corpus),
        "total_chunks": len(index.chunks),
        "max_chunk_tokens_limit": args.max_chunk_tokens,
        "max_chunk_tokens_observed": max_obs_tokens,
        "graph_entities": len(index.entity_to_chunks),
        "index_build_ms": round(index.build_ms, 2),
        "dense_build_ms": round(dense_build_ms, 2) if dense_build_ms is not None else None,
        "reference_targets": {
            "paper_best_dense_rerank_hits10": 0.7467,
            "lumimax_claimed_hits10": 0.9295,
        },
        "api_stats": client.stats if client else None,
        "methods": results_by_method,
    }

    out_path = args.out or (BENCH_ROOT / "results" / f"multihop_{slice_label}_summary.json")
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(summary, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"\n[saved] {out_path}")

    if cache is not None:
        cache.close()


if __name__ == "__main__":
    main()
