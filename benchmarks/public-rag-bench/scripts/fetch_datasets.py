#!/usr/bin/env python3
"""Download public benchmark datasets into `data/` (gitignored)."""

from __future__ import annotations

import argparse
import urllib.request
from pathlib import Path

BENCH_ROOT = Path(__file__).resolve().parents[1]
DATA_ROOT = BENCH_ROOT / "data"

SOURCES = {
    "ramdocs": [
        (
            "ramdocs/RAMDocs_test.jsonl",
            "https://raw.githubusercontent.com/HanNight/RAMDocs/main/RAMDocs_test.jsonl",
        ),
    ],
    "multihop_rag": [
        (
            "multihop_rag/MultiHopRAG.json",
            "https://huggingface.co/datasets/yixuantt/MultiHopRAG/resolve/main/MultiHopRAG.json",
        ),
        (
            "multihop_rag/corpus.json",
            "https://huggingface.co/datasets/yixuantt/MultiHopRAG/resolve/main/corpus.json",
        ),
    ],
}


def download_file(url: str, dest: Path, force: bool = False) -> None:
    if dest.exists() and not force:
        print(f"[skip] {dest} already exists ({dest.stat().st_size:,} bytes)")
        return
    dest.parent.mkdir(parents=True, exist_ok=True)
    print(f"[fetch] {url} -> {dest}")
    urllib.request.urlretrieve(url, dest)
    print(f"[ok]    {dest} ({dest.stat().st_size:,} bytes)")


def main() -> None:
    parser = argparse.ArgumentParser(description="Fetch public RAG benchmark datasets.")
    parser.add_argument(
        "--benchmarks",
        default="ramdocs,multihop_rag",
        help="Comma-separated list: ramdocs,multihop_rag",
    )
    parser.add_argument("--force", action="store_true", help="Re-download existing files.")
    args = parser.parse_args()

    for name in [x.strip() for x in args.benchmarks.split(",") if x.strip()]:
        if name not in SOURCES:
            raise SystemExit(f"Unknown benchmark: {name}. Available: {', '.join(SOURCES)}")
        for rel, url in SOURCES[name]:
            download_file(url, DATA_ROOT / rel, force=args.force)


if __name__ == "__main__":
    main()
