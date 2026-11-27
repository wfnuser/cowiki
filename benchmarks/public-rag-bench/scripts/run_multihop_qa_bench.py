#!/usr/bin/env python3
"""End-to-End MultiHop-RAG QA Benchmark (comparing bm25_raw vs CoWiki Decomposed Hybrid + Rerank).

Evaluates all 100 questions (including `null_query` abstention!) under identical Reader LLM,
reporting official `qa_evaluate.py` accuracy, strict EM, null-query abstention accuracy,
and exact prompt/context token usage.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import os
import sys
import time
from collections import defaultdict
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, List

BENCH_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(BENCH_ROOT / "src"))

from cowiki_bench.datasets import load_multihop_rag, load_slice_indices
from cowiki_bench.llm_client import OpenAICompatibleClient, PersistentApiCache, load_secret_from_env_or_file
from cowiki_bench.metrics import estimate_tokens, evaluate_multihop_qa_official, normalize_answer
from cowiki_bench.multihop_index import MultiHopHybridIndex


def answer_multihop_question(
    question: str,
    retrieved: List[Dict[str, Any]],
    client: OpenAICompatibleClient,
    reader_model: str,
) -> Dict[str, Any]:
    ctx_blocks = []
    for i, ch in enumerate(retrieved[:10], start=1):
        ctx_blocks.append(
            f"[Evidence {i} | Source: {ch['source']} | Date: {ch['published_at']} | Title: {ch['title']}]\n{ch['text']}"
        )
    context = "\n\n".join(ctx_blocks)
    prompt = (
        "Answer the question strictly using ONLY the provided news evidence chunks.\n"
        "Rules:\n"
        "1. If the evidence does NOT contain the specific articles, people, or facts needed to answer the question, "
        "reply EXACTLY: `Insufficient information.`\n"
        "2. For yes/no comparison or temporal questions, answer concisely with `Yes` or `No`.\n"
        "3. For entity/fact questions, answer concisely with ONLY the entity name or short factual answer.\n"
        "Return strict JSON: {\"answer\": \"...\"}\n\n"
        f"Question: {question}\n\nEvidence:\n{context}"
    )
    res = client.chat(
        model=reader_model,
        messages=[
            {"role": "system", "content": "You are a precise multi-hop news QA reader. Never hallucinate beyond evidence. Output valid JSON only."},
            {"role": "user", "content": prompt},
        ],
        temperature=0.0,
        max_tokens=160,
        json_mode=True,
    )
    ans = res.content.strip()
    try:
        obj = json.loads(ans)
        if isinstance(obj, dict) and "answer" in obj:
            ans = str(obj["answer"]).strip()
    except Exception:
        pass
    return {
        "answer": ans,
        "context_tokens": sum(estimate_tokens(ch["text"]) for ch in retrieved[:10]),
        "prompt_tokens": res.prompt_tokens,
        "completion_tokens": res.completion_tokens,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description="End-to-End MultiHop-RAG QA benchmark.")
    parser.add_argument("--slice", type=Path, default=BENCH_ROOT / "slices/multihop_dev100.json")
    parser.add_argument("--methods", default="bm25_raw,hybrid_multihop_decomposed_rerank")
    parser.add_argument("--reader-model", default="qwen-plus")
    parser.add_argument("--embed-model", default="text-embedding-v4")
    parser.add_argument("--embed-dim", type=int, default=512)
    parser.add_argument("--rerank-model", default="gte-rerank-v2")
    parser.add_argument("--decompose-model", default="qwen-flash")
    parser.add_argument("--base-url", default=os.environ.get("COWIKI_BENCH_BASE_URL", "https://dashscope.aliyuncs.com/compatible-mode/v1"))
    parser.add_argument("--workers", type=int, default=6)
    parser.add_argument("--env-file", type=Path, default=None)
    parser.add_argument("--out", type=Path, default=None)
    args = parser.parse_args()

    api_key = load_secret_from_env_or_file(
        ["COWIKI_BENCH_API_KEY", "DASHSCOPE_API_KEY", "OPENAI_API_KEY", "LLM_API_KEY"],
        env_file=args.env_file,
    )
    if not api_key:
        raise SystemExit("No API key found.")

    queries, corpus = load_multihop_rag(
        BENCH_ROOT / "data/multihop_rag/MultiHopRAG.json",
        BENCH_ROOT / "data/multihop_rag/corpus.json",
    )
    indices = load_slice_indices(args.slice)
    eval_queries = [queries[i] for i in indices]

    cache = PersistentApiCache(BENCH_ROOT / ".cache/multihop_api_cache.sqlite")
    client = OpenAICompatibleClient(api_key=api_key, base_url=args.base_url, cache=cache)

    index = MultiHopHybridIndex(corpus=corpus, max_chunk_tokens=256, overlap_tokens=32)
    methods = [m.strip() for m in args.methods.split(",") if m.strip()]
    if any(m != "bm25_raw" and m != "bm25_okf_meta" and m != "bm25_okf_ppr_slots" for m in methods):
        index.build_dense_index(client=client, embed_model=args.embed_model, dimensions=args.embed_dim)

    summary_methods: Dict[str, Any] = {}
    for method in methods:
        t0 = time.perf_counter()

        def _run_q(q: Dict[str, Any]) -> Dict[str, Any]:
            ret = index.retrieve(
                query=q["query"],
                method=method,
                top_k=10,
                client=client,
                embed_model=args.embed_model,
                embed_dim=args.embed_dim,
                rerank_model=args.rerank_model,
                decompose_model=args.decompose_model,
            )
            err_msg = None
            try:
                qa = answer_multihop_question(q["query"], ret, client, args.reader_model)
            except Exception as exc:
                err_msg = str(exc)[:300]
                qa = {
                    "answer": "",
                    "context_tokens": sum(estimate_tokens(ch["text"]) for ch in ret[:10]),
                    "prompt_tokens": 0,
                    "completion_tokens": 0,
                }
            return {
                "idx": q["_idx"],
                "question_type": q["question_type"],
                "gold_answer": q["answer"],
                "pred_answer": qa["answer"],
                "context_tokens": qa["context_tokens"],
                "prompt_tokens": qa["prompt_tokens"],
                "completion_tokens": qa["completion_tokens"],
                "error": err_msg,
            }

        with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as pool:
            rows = list(pool.map(_run_q, eval_queries))

        overall = evaluate_multihop_qa_official([r["pred_answer"] for r in rows], [r["gold_answer"] for r in rows])
        by_type_rows: Dict[str, List[Dict[str, Any]]] = defaultdict(list)
        for r in rows:
            by_type_rows[r["question_type"]].append(r)
        by_type_metrics = {}
        for qt, grp in sorted(by_type_rows.items()):
            m = evaluate_multihop_qa_official([x["pred_answer"] for x in grp], [x["gold_answer"] for x in grp])
            m["n"] = len(grp)
            by_type_metrics[qt] = m

        # Null-query abstention check
        null_grp = by_type_rows.get("null_query", [])
        null_abst = 0.0
        if null_grp:
            null_abst = round(
                sum(1 for x in null_grp if "insufficient" in normalize_answer(x["pred_answer"])) / len(null_grp),
                4,
            )

        summary_methods[method] = {
            "n": len(rows),
            **overall,
            "null_query_abstention_acc": null_abst,
            "avg_context_tokens": round(sum(r["context_tokens"] for r in rows) / len(rows), 1),
            "avg_prompt_tokens": round(sum(r["prompt_tokens"] for r in rows) / len(rows), 1),
            "api_errors": sum(1 for r in rows if r.get("error")),
            "wall_time_sec": round(time.perf_counter() - t0, 2),
            "by_question_type": by_type_metrics,
        }
        print(
            f"\n[{method}] QA reader={args.reader_model} n={len(rows)} | "
            f"OfficialAcc={overall['official_accuracy']:.4f} | StrictEM={overall['strict_em']:.4f} | "
            f"NullAbstain={null_abst:.4f} | avg_ctx_tok={summary_methods[method]['avg_context_tokens']:.0f}"
        )

    out_path = args.out or (BENCH_ROOT / "results" / f"multihop_qa_{args.slice.stem}_{args.reader_model}.json")
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(
        json.dumps(
            {
                "benchmark": "MultiHop-RAG End-to-End QA",
                "generated_at_utc": datetime.now(timezone.utc).isoformat(),
                "slice": args.slice.stem,
                "reader_model": args.reader_model,
                "methods": summary_methods,
            },
            indent=2,
            ensure_ascii=False,
        )
        + "\n",
        encoding="utf-8",
    )
    print(f"\n[saved] {out_path}")
    cache.close()


if __name__ == "__main__":
    main()
