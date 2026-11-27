"""Dataset loaders and deterministic stratified Dev-slice builders.

Supports:
- RAMDocs (HanNight/RAMDocs, COLM 2025, 500 test instances)
- MultiHop-RAG (yixuantt/MultiHop-RAG, COLM 2024, 2,556 queries + 609 news articles)
"""

from __future__ import annotations

import json
import random
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any, Dict, List, Sequence, Tuple


def load_ramdocs(path: Path) -> List[Dict[str, Any]]:
    rows: List[Dict[str, Any]] = []
    with path.open("r", encoding="utf-8") as fh:
        for idx, line in enumerate(fh):
            line = line.strip()
            if not line:
                continue
            item = json.loads(line)
            item["_idx"] = idx
            rows.append(item)
    return rows


def load_multihop_rag(queries_path: Path, corpus_path: Path) -> Tuple[List[Dict[str, Any]], List[Dict[str, Any]]]:
    with queries_path.open("r", encoding="utf-8") as fh:
        queries = json.load(fh)
    for idx, q in enumerate(queries):
        q["_idx"] = idx
    with corpus_path.open("r", encoding="utf-8") as fh:
        corpus = json.load(fh)
    for idx, doc in enumerate(corpus):
        doc["_doc_id"] = idx
    return queries, corpus


def _allocate_strata_counts(strata: Dict[str, List[int]], target_n: int) -> Dict[str, int]:
    total = sum(len(v) for v in strata.values())
    if target_n >= total:
        return {k: len(v) for k, v in strata.items()}

    exact = {k: (len(v) * target_n) / total for k, v in strata.items()}
    alloc = {k: min(len(strata[k]), int(exact[k])) for k in strata}
    # Ensure every non-empty stratum gets at least 1 when target_n allows
    if target_n >= len(strata):
        for k in strata:
            if alloc[k] == 0 and len(strata[k]) > 0:
                alloc[k] = 1

    # Adjust to match target_n exactly by largest fractional remainder
    while sum(alloc.values()) < target_n:
        candidates = sorted(
            (k for k in strata if alloc[k] < len(strata[k])),
            key=lambda k: (exact[k] - alloc[k], len(strata[k]), k),
            reverse=True,
        )
        if not candidates:
            break
        alloc[candidates[0]] += 1

    while sum(alloc.values()) > target_n:
        candidates = sorted(
            (k for k in strata if alloc[k] > 1),
            key=lambda k: (alloc[k] - exact[k], k),
            reverse=True,
        )
        if not candidates:
            candidates = sorted((k for k in strata if alloc[k] > 0), key=lambda k: alloc[k], reverse=True)
        alloc[candidates[0]] -= 1

    return alloc


def build_ramdocs_stratified_slice(
    rows: Sequence[Dict[str, Any]],
    n: int = 100,
    seed: int = 42,
) -> Dict[str, Any]:
    """Stratify RAMDocs by (gold_count, has_misinfo, has_noise)."""
    rng = random.Random(seed)
    strata: Dict[str, List[int]] = defaultdict(list)
    for row in rows:
        idx = int(row["_idx"])
        g_cnt = len(row.get("gold_answers", []))
        has_m = any(d.get("type") == "misinfo" for d in row.get("documents", []))
        has_n = any(d.get("type") == "noise" for d in row.get("documents", []))
        key = f"gold={g_cnt}|misinfo={int(has_m)}|noise={int(has_n)}"
        strata[key].append(idx)

    alloc = _allocate_strata_counts(strata, n)
    selected: List[int] = []
    stratum_stats: Dict[str, Dict[str, int]] = {}
    for key in sorted(strata.keys()):
        pool = list(strata[key])
        rng.shuffle(pool)
        take = alloc[key]
        chosen = sorted(pool[:take])
        selected.extend(chosen)
        stratum_stats[key] = {"population": len(pool), "sampled": take}

    selected.sort()
    return {
        "benchmark": "ramdocs",
        "source": "HanNight/RAMDocs (RAMDocs_test.jsonl)",
        "seed": seed,
        "population_size": len(rows),
        "sample_size": len(selected),
        "strata": stratum_stats,
        "indices": selected,
    }


def build_multihop_stratified_slice(
    queries: Sequence[Dict[str, Any]],
    n: int = 100,
    seed: int = 42,
    include_null: bool = True,
) -> Dict[str, Any]:
    """Stratify MultiHop-RAG by (question_type, evidence_count)."""
    rng = random.Random(seed)
    strata: Dict[str, List[int]] = defaultdict(list)
    for q in queries:
        qtype = q.get("question_type", "unknown")
        if not include_null and qtype == "null_query":
            continue
        idx = int(q["_idx"])
        ev_cnt = len(q.get("evidence_list", []))
        key = f"type={qtype}|ev={ev_cnt}"
        strata[key].append(idx)

    alloc = _allocate_strata_counts(strata, n)
    selected: List[int] = []
    stratum_stats: Dict[str, Dict[str, int]] = {}
    for key in sorted(strata.keys()):
        pool = list(strata[key])
        rng.shuffle(pool)
        take = alloc[key]
        chosen = sorted(pool[:take])
        selected.extend(chosen)
        stratum_stats[key] = {"population": len(pool), "sampled": take}

    selected.sort()
    by_idx = {int(q["_idx"]): q for q in queries}
    type_counts = Counter(by_idx[i]["question_type"] for i in selected)
    return {
        "benchmark": "multihop_rag",
        "source": "yixuantt/MultiHopRAG (MultiHopRAG.json + corpus.json)",
        "seed": seed,
        "population_size": len(queries),
        "sample_size": len(selected),
        "non_null_sample_size": sum(v for k, v in type_counts.items() if k != "null_query"),
        "question_type_counts": dict(sorted(type_counts.items())),
        "strata": stratum_stats,
        "indices": selected,
    }


def load_slice_indices(slice_path: Path) -> List[int]:
    with slice_path.open("r", encoding="utf-8") as fh:
        payload = json.load(fh)
    return [int(i) for i in payload["indices"]]
