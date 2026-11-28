#!/usr/bin/env python3
"""Run RAMDocs conflict-resolution ablations (Standard RAG Baseline vs CoWiki Compiled Arbiter)."""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import os
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, List

BENCH_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(BENCH_ROOT / "src"))

from cowiki_bench.datasets import load_ramdocs, load_slice_indices
from cowiki_bench.llm_client import OpenAICompatibleClient, PersistentApiCache, load_secret_from_env_or_file
from cowiki_bench.metrics import aggregate_ramdocs_scores, score_ramdocs_instance
from cowiki_bench.ramdocs_engine import (
    cluster_near_duplicate_docs,
    solve_ramdocs_cowiki_arbiter,
    solve_ramdocs_cowiki_arbiter_v2,
    solve_ramdocs_cowiki_arbiter_v3,
    solve_ramdocs_cowiki_arbiter_v4,
    solve_ramdocs_cowiki_arbiter_v5,
    solve_ramdocs_standard_rag,
)


def main() -> None:
    parser = argparse.ArgumentParser(description="Run RAMDocs conflict resolution benchmark.")
    parser.add_argument(
        "--slice",
        type=Path,
        default=BENCH_ROOT / "slices/ramdocs_dev100.json",
        help="Path to stratified slice JSON (ignored if --full is set).",
    )
    parser.add_argument("--full", action="store_true", help="Evaluate all 500 RAMDocs rows.")
    parser.add_argument(
        "--methods",
        default="baseline_standard_rag,cowiki_compiled_arbiter",
        help="Comma-separated methods: baseline_standard_rag, cowiki_compiled_arbiter",
    )
    parser.add_argument("--model", default=os.environ.get("COWIKI_BENCH_CHAT_MODEL", "qwen-plus"))
    parser.add_argument("--base-url", default=os.environ.get("COWIKI_BENCH_BASE_URL", "https://dashscope.aliyuncs.com/compatible-mode/v1"))
    parser.add_argument("--env-file", type=Path, default=None, help="Optional .env file for API key lookup.")
    parser.add_argument("--workers", type=int, default=4, help="Parallel worker threads.")
    parser.add_argument("--dry-run", action="store_true", help="Run CPU near-duplicate diagnostic stats only (0 API calls).")
    parser.add_argument("--out", type=Path, default=None, help="Output summary JSON path.")
    args = parser.parse_args()

    ram_path = BENCH_ROOT / "data/ramdocs/RAMDocs_test.jsonl"
    if not ram_path.exists():
        raise SystemExit("RAMDocs dataset not found. Run: python3 -B scripts/fetch_datasets.py --benchmarks ramdocs")

    rows = load_ramdocs(ram_path)
    if args.full:
        eval_rows = rows
        slice_label = "full_500"
    else:
        indices = load_slice_indices(args.slice)
        eval_rows = [rows[i] for i in indices]
        slice_label = args.slice.stem

    if args.dry_run:
        dup_rows = 0
        misinfo_dup_rows = 0
        for row in eval_rows:
            docs = [d["text"] for d in row["documents"]]
            infos = cluster_near_duplicate_docs(docs)
            if any(info.cluster_size > 1 for info in infos):
                dup_rows += 1
                if any(info.cluster_size > 1 and row["documents"][info.doc_id]["type"] == "misinfo" for info in infos):
                    misinfo_dup_rows += 1
        print(
            f"[dry-run] slice={slice_label} n={len(eval_rows)} | "
            f"rows_with_near_dup_docs={dup_rows} ({dup_rows/len(eval_rows):.1%}) | "
            f"rows_with_dup_misinfo={misinfo_dup_rows} ({misinfo_dup_rows/len(eval_rows):.1%})"
        )
        return

    api_key = load_secret_from_env_or_file(
        ["COWIKI_BENCH_API_KEY", "DASHSCOPE_API_KEY", "OPENAI_API_KEY", "LLM_API_KEY"],
        env_file=args.env_file,
    )
    if not api_key:
        raise SystemExit(
            "No API key found. Set COWIKI_BENCH_API_KEY / DASHSCOPE_API_KEY / OPENAI_API_KEY or pass --env-file."
        )

    cache = PersistentApiCache(BENCH_ROOT / ".cache/ramdocs_api_cache.sqlite")
    client = OpenAICompatibleClient(api_key=api_key, base_url=args.base_url, cache=cache)
    methods = [m.strip() for m in args.methods.split(",") if m.strip()]

    solvers = {
        "baseline_standard_rag": solve_ramdocs_standard_rag,
        "cowiki_compiled_arbiter": solve_ramdocs_cowiki_arbiter,
        "cowiki_compiled_arbiter_v2": solve_ramdocs_cowiki_arbiter_v2,
        "cowiki_compiled_arbiter_v3": solve_ramdocs_cowiki_arbiter_v3,
        "cowiki_compiled_arbiter_v4": solve_ramdocs_cowiki_arbiter_v4,
        "cowiki_compiled_arbiter_v5": solve_ramdocs_cowiki_arbiter_v5,
    }

    summary_methods: Dict[str, Any] = {}
    for method in methods:
        if method not in solvers:
            raise SystemExit(f"Unknown RAMDocs method: {method}")
        solver = solvers[method]
        t0 = time.perf_counter()

        def _run_one(row: Dict[str, Any]) -> Dict[str, Any]:
            docs = [d["text"] for d in row["documents"]]
            err_msg = None
            try:
                out = solver(row["question"], docs, client, args.model)
            except Exception as exc:
                err_msg = str(exc)[:300]
                out = {
                    "answers": [],
                    "prompt_tokens": 0,
                    "completion_tokens": 0,
                    "llm_calls": 1,
                }
            sc = score_ramdocs_instance(
                predicted_answers=out["answers"],
                gold_answers=row.get("gold_answers", []),
                wrong_answers=row.get("wrong_answers", []),
            )
            return {
                "idx": row["_idx"],
                "question": row["question"],
                "gold_answers": row.get("gold_answers", []),
                "wrong_answers": row.get("wrong_answers", []),
                "predicted_answers": out["answers"],
                "prompt_tokens": out["prompt_tokens"],
                "completion_tokens": out["completion_tokens"],
                "llm_calls": out["llm_calls"],
                "error": err_msg,
                "score": sc,
            }

        per_row: List[Dict[str, Any]] = []
        with concurrent.futures.ThreadPoolExecutor(max_workers=max(1, args.workers)) as pool:
            futs = [pool.submit(_run_one, row) for row in eval_rows]
            for fut in concurrent.futures.as_completed(futs):
                per_row.append(fut.result())
        per_row.sort(key=lambda x: x["idx"])
        wall_sec = time.perf_counter() - t0

        agg = aggregate_ramdocs_scores([r["score"] for r in per_row])
        agg["avg_prompt_tokens"] = round(sum(r["prompt_tokens"] for r in per_row) / len(per_row), 1)
        agg["avg_completion_tokens"] = round(sum(r["completion_tokens"] for r in per_row) / len(per_row), 1)
        agg["avg_llm_calls"] = round(sum(r["llm_calls"] for r in per_row) / len(per_row), 2)
        agg["api_errors"] = sum(1 for r in per_row if r.get("error"))
        agg["wall_time_sec"] = round(wall_sec, 2)
        summary_methods[method] = agg

        print(
            f"\n[{method}] model={args.model} n={agg['n']} | "
            f"EM_strict={agg['EM_strict']:.4f} | EM_paper={agg['EM_paper']:.4f} | "
            f"F1={agg['F1']:.4f} (P={agg['Precision']:.4f} R={agg['Recall']:.4f}) | "
            f"MisinfoLeak={agg['MisinfoLeakRate']:.4f} | FullGold={agg['FullGoldRecallRate']:.4f} | "
            f"avg_prompt_tok={agg['avg_prompt_tokens']:.0f} | calls={agg['avg_llm_calls']}"
        )

        # Save detailed per-row dump in gitignored runs/ directory for error analysis
        runs_dir = BENCH_ROOT / "results/runs"
        runs_dir.mkdir(parents=True, exist_ok=True)
        run_dump = runs_dir / f"ramdocs_{slice_label}_{method}_{args.model.replace('/', '_')}.jsonl"
        with run_dump.open("w", encoding="utf-8") as fh:
            for r in per_row:
                fh.write(json.dumps(r, ensure_ascii=False) + "\n")

    summary = {
        "benchmark": "RAMDocs",
        "generated_at_utc": datetime.now(timezone.utc).isoformat(),
        "slice": slice_label,
        "model": args.model,
        "reference_targets": {
            "paper_llama33_70b_standard_rag_em": 0.3260,
            "paper_llama33_70b_madam_rag_em": 0.3440,
            "lumimax_claimed_score": 0.4529,
        },
        "api_stats": client.stats,
        "methods": summary_methods,
    }
    out_path = args.out or (BENCH_ROOT / "results" / f"ramdocs_{slice_label}_{args.model.replace('/', '_')}_summary.json")
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(summary, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"\n[saved] {out_path}")
    cache.close()


if __name__ == "__main__":
    main()
