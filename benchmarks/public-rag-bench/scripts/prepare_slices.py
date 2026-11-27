#!/usr/bin/env python3
"""Generate deterministic stratified Dev slices (`seed=42`) for rapid iteration."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

BENCH_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(BENCH_ROOT / "src"))

from cowiki_bench.datasets import (
    build_multihop_stratified_slice,
    build_ramdocs_stratified_slice,
    load_multihop_rag,
    load_ramdocs,
)


def main() -> None:
    parser = argparse.ArgumentParser(description="Generate stratified dev slices.")
    parser.add_argument("--n", type=int, default=100, help="Target slice size per benchmark.")
    parser.add_argument("--seed", type=int, default=42, help="Random seed.")
    args = parser.parse_args()

    slices_dir = BENCH_ROOT / "slices"
    slices_dir.mkdir(parents=True, exist_ok=True)

    ram_path = BENCH_ROOT / "data/ramdocs/RAMDocs_test.jsonl"
    if ram_path.exists():
        ram_rows = load_ramdocs(ram_path)
        ram_slice = build_ramdocs_stratified_slice(ram_rows, n=args.n, seed=args.seed)
        out_path = slices_dir / f"ramdocs_dev{args.n}.json"
        out_path.write_text(json.dumps(ram_slice, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
        print(f"[ok] wrote {out_path} ({ram_slice['sample_size']}/{ram_slice['population_size']})")

    mh_q_path = BENCH_ROOT / "data/multihop_rag/MultiHopRAG.json"
    mh_c_path = BENCH_ROOT / "data/multihop_rag/corpus.json"
    if mh_q_path.exists() and mh_c_path.exists():
        mh_queries, _ = load_multihop_rag(mh_q_path, mh_c_path)
        mh_slice = build_multihop_stratified_slice(mh_queries, n=args.n, seed=args.seed, include_null=True)
        out_path = slices_dir / f"multihop_dev{args.n}.json"
        out_path.write_text(json.dumps(mh_slice, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
        print(
            f"[ok] wrote {out_path} ({mh_slice['sample_size']}/{mh_slice['population_size']}, "
            f"non_null={mh_slice['non_null_sample_size']})"
        )


if __name__ == "__main__":
    main()
