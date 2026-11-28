#!/usr/bin/env python3
"""Build medium-batch stratified slices (`ramdocs_med300`, `multihop_med400`) + fresh blind extensions (`ext100`, `ext200`).

Strictly partitions:
- RAMDocs (500 total): dev100 (100) + val100 (100) + ext100 (100 fresh blind) = med300 (300 = 60%)
- MultiHop-RAG (2,556 total): dev100 (100) + val100 (100) + ext200 (200 fresh blind) = med400 (400)
"""

from __future__ import annotations

import json
import sys
from collections import Counter
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
    slices_dir = BENCH_ROOT / "slices"

    # 1. RAMDocs ext100 + med300
    ram_dev = set(load_slice_indices(slices_dir / "ramdocs_dev100.json"))
    ram_val = set(load_slice_indices(slices_dir / "ramdocs_val100.json"))
    assert ram_dev.isdisjoint(ram_val)
    ram_all = load_ramdocs(BENCH_ROOT / "data/ramdocs/RAMDocs_test.jsonl")
    ram_rem = [r for r in ram_all if r["_idx"] not in (ram_dev | ram_val)]
    ram_ext = build_ramdocs_stratified_slice(ram_rem, n=100, seed=2026)
    ram_ext["disjoint_from"] = ["slices/ramdocs_dev100.json", "slices/ramdocs_val100.json"]
    assert set(ram_ext["indices"]).isdisjoint(ram_dev | ram_val)
    (slices_dir / "ramdocs_ext100.json").write_text(json.dumps(ram_ext, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")

    med300_indices = sorted(ram_dev | ram_val | set(ram_ext["indices"]))
    assert len(med300_indices) == 300
    ram_med300 = {
        "benchmark": "ramdocs",
        "source": "HanNight/RAMDocs (RAMDocs_test.jsonl)",
        "population_size": len(ram_all),
        "sample_size": len(med300_indices),
        "partitions": {
            "dev100": sorted(ram_dev),
            "val100": sorted(ram_val),
            "ext100": sorted(ram_ext["indices"]),
        },
        "indices": med300_indices,
    }
    (slices_dir / "ramdocs_med300.json").write_text(json.dumps(ram_med300, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"[ok] RAMDocs ext100 (100 fresh blind) + med300 (300/{len(ram_all)})")

    # 2. MultiHop-RAG ext200 + med400
    mh_dev = set(load_slice_indices(slices_dir / "multihop_dev100.json"))
    mh_val = set(load_slice_indices(slices_dir / "multihop_val100.json"))
    assert mh_dev.isdisjoint(mh_val)
    mh_all, _ = load_multihop_rag(
        BENCH_ROOT / "data/multihop_rag/MultiHopRAG.json",
        BENCH_ROOT / "data/multihop_rag/corpus.json",
    )
    mh_rem = [q for q in mh_all if q["_idx"] not in (mh_dev | mh_val)]
    mh_ext = build_multihop_stratified_slice(mh_rem, n=200, seed=2026, include_null=True)
    mh_ext["disjoint_from"] = ["slices/multihop_dev100.json", "slices/multihop_val100.json"]
    assert set(mh_ext["indices"]).isdisjoint(mh_dev | mh_val)
    (slices_dir / "multihop_ext200.json").write_text(json.dumps(mh_ext, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")

    med400_indices = sorted(mh_dev | mh_val | set(mh_ext["indices"]))
    assert len(med400_indices) == 400
    type_counts = Counter(mh_all[i]["question_type"] for i in med400_indices)
    mh_med400 = {
        "benchmark": "multihop_rag",
        "source": "yixuantt/MultiHopRAG (MultiHopRAG.json + corpus.json)",
        "population_size": len(mh_all),
        "sample_size": len(med400_indices),
        "non_null_sample_size": sum(v for k, v in type_counts.items() if k != "null_query"),
        "question_type_counts": dict(sorted(type_counts.items())),
        "partitions": {
            "dev100": sorted(mh_dev),
            "val100": sorted(mh_val),
            "ext200": sorted(mh_ext["indices"]),
        },
        "indices": med400_indices,
    }
    (slices_dir / "multihop_med400.json").write_text(json.dumps(mh_med400, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"[ok] MultiHop-RAG ext200 (200 fresh blind, non_null={mh_ext['non_null_sample_size']}) + med400 (400/{len(mh_all)}, non_null={mh_med400['non_null_sample_size']})")


if __name__ == "__main__":
    main()
