#!/usr/bin/env python3
"""Create strictly disjoint held-out validation slices (zero overlap with dev100)."""

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
    load_slice_indices,
)


def main() -> None:
    parser = argparse.ArgumentParser(description="Generate strictly disjoint held-out val slices.")
    parser.add_argument("--n", type=int, default=100)
    parser.add_argument("--seed", type=int, default=1337)
    args = parser.parse_args()

    slices_dir = BENCH_ROOT / "slices"

    ram_dev = set(load_slice_indices(slices_dir / "ramdocs_dev100.json"))
    ram_all = load_ramdocs(BENCH_ROOT / "data/ramdocs/RAMDocs_test.jsonl")
    ram_rem = [r for r in ram_all if r["_idx"] not in ram_dev]
    ram_val = build_ramdocs_stratified_slice(ram_rem, n=args.n, seed=args.seed)
    assert set(ram_val["indices"]).isdisjoint(ram_dev)
    ram_val["disjoint_from"] = "slices/ramdocs_dev100.json"
    out_ram = slices_dir / f"ramdocs_val{args.n}.json"
    out_ram.write_text(json.dumps(ram_val, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"[ok] wrote {out_ram} ({ram_val['sample_size']} disjoint from dev100)")

    mh_dev = set(load_slice_indices(slices_dir / "multihop_dev100.json"))
    mh_all, _ = load_multihop_rag(
        BENCH_ROOT / "data/multihop_rag/MultiHopRAG.json",
        BENCH_ROOT / "data/multihop_rag/corpus.json",
    )
    mh_rem = [q for q in mh_all if q["_idx"] not in mh_dev]
    mh_val = build_multihop_stratified_slice(mh_rem, n=args.n, seed=args.seed, include_null=True)
    assert set(mh_val["indices"]).isdisjoint(mh_dev)
    mh_val["disjoint_from"] = "slices/multihop_dev100.json"
    out_mh = slices_dir / f"multihop_val{args.n}.json"
    out_mh.write_text(json.dumps(mh_val, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"[ok] wrote {out_mh} ({mh_val['sample_size']} disjoint from dev100, non_null={mh_val['non_null_sample_size']})")


if __name__ == "__main__":
    main()
