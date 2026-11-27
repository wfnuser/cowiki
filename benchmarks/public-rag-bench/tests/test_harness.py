"""Offline unit tests for CoWiki Public RAG Benchmark Harness (zero API calls)."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

BENCH_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(BENCH_ROOT / "src"))

from cowiki_bench.datasets import build_multihop_stratified_slice, build_ramdocs_stratified_slice
from cowiki_bench.metrics import (
    aggregate_ramdocs_scores,
    estimate_tokens,
    evaluate_multihop_retrieval,
    score_ramdocs_instance,
)
from cowiki_bench.multihop_index import MultiHopHybridIndex, chunk_document_body, extract_dates_from_text, l2_normalize
from cowiki_bench.ramdocs_engine import cluster_near_duplicate_docs, compute_strict_fork_diffs, is_corrupted_binary_doc


class TestBenchHarness(unittest.TestCase):
    def test_chunker_strict_token_limit(self) -> None:
        long_text = " ".join(f"Sentence number {i} discusses Sam Bankman-Fried and FTX cryptocurrency trial." for i in range(60))
        chunks = chunk_document_body(long_text, max_tokens=256, overlap_tokens=32)
        self.assertGreaterEqual(len(chunks), 3)
        for ch in chunks:
            self.assertLessEqual(estimate_tokens(ch), 256)

    def test_date_extraction(self) -> None:
        q = "After the TechCrunch report on October 7, 2023, and the subsequent article on Oct 30, 2023 or 2023-11-05"
        self.assertEqual(extract_dates_from_text(q), ["2023-10-07", "2023-10-30", "2023-11-05"])

    def test_multihop_official_metrics(self) -> None:
        records = [
            {
                "question_type": "inference_query",
                "gold_list": [{"fact": "Alpha fact one."}, {"fact": "Beta fact two."}],
                "retrieval_list": [
                    {"text": "Distractor text."},
                    {"text": "Lead in Alpha fact one. Tail."},
                    {"text": "Lead in Beta fact two. Tail."},
                ],
            },
            {
                "question_type": "null_query",
                "gold_list": [],
                "retrieval_list": [{"text": "Anything"}],
            },
        ]
        m = evaluate_multihop_retrieval(records)
        self.assertEqual(m["valid_queries"], 1)
        self.assertEqual(m["Hits@10"], 1.0)
        self.assertEqual(m["AllGold@10"], 1.0)
        self.assertAlmostEqual(m["MRR@10"], 0.5, places=4)
        self.assertAlmostEqual(m["MAP@10"], (0.5 + 1.0 / 3.0) / 2.0, places=4)

    def test_multihop_ppr_and_slot_retrieval(self) -> None:
        corpus = [
            {
                "title": "Weekly Tech Digest",
                "source": "TechCrunch",
                "published_at": "2023-10-07T12:00:00+00:00",
                "author": "Alice",
                "category": "tech",
                "url": "https://tc.example/1",
                "body": "Intro news. Turmoil at Flexport: Dave Clark criticized recent leadership transitions at Flexport.",
            },
            {
                "title": "Another Tech Roundup",
                "source": "The Verge",
                "published_at": "2023-10-30T12:00:00+00:00",
                "author": "Bob",
                "category": "tech",
                "url": "https://verge.example/2",
                "body": "Other story. Ryan Petersen began cutting costs at Flexport by laying off 20 percent of workers.",
            },
        ]
        idx = MultiHopHybridIndex(corpus, max_chunk_tokens=256)
        q = (
            "After the TechCrunch report on October 7, 2023 regarding Dave Clark at Flexport "
            "and The Verge article on October 30, 2023 regarding Ryan Petersen, what changed?"
        )
        hits = idx.retrieve(q, method="bm25_okf_ppr_slots", top_k=5)
        urls = {h["url"] for h in hits}
        self.assertIn("https://tc.example/1", urls)
        self.assertIn("https://verge.example/2", urls)

    def test_ramdocs_metrics_and_near_duplicate_clustering(self) -> None:
        # Exact multi-entity match with zero misinfo
        s1 = score_ramdocs_instance(["1987", "1988"], ["1987", "1988"], ["1995", "1990"])
        self.assertEqual(s1["strict_em"], 1.0)
        self.assertEqual(s1["misinfo_leaked"], 0.0)

        # Leaked misinfo -> strict_em and paper_em both 0
        s2 = score_ramdocs_instance(["1987", "1988", "1995"], ["1987", "1988"], ["1995"])
        self.assertEqual(s2["strict_em"], 0.0)
        self.assertEqual(s2["paper_em"], 0.0)
        self.assertEqual(s2["misinfo_leaked"], 1.0)

        # Near-duplicate detection catches overlapping misinfo snippets
        docs = [
            "Richard Griffin, 2nd Baron Braybrooke (3 July 1750 - 28 February 1825) was an English peer.",
            "The others of the party are John Staples, James Byres, and Sir William Young. Aldworth was born on 12 December 1750 in Westminster.",
            "To the right of the guide is Baron Braybrooke. The others of the party are John Staples, James Byres, and Sir William Young. Aldworth was born on 12 December 1750 in Westminster.",
        ]
        infos = cluster_near_duplicate_docs(docs)
        self.assertEqual(infos[0].cluster_size, 1)
        self.assertEqual(infos[1].cluster_id, infos[2].cluster_id)
        self.assertEqual(infos[1].cluster_size, 2)
        self.assertAlmostEqual(infos[1].independent_weight, 0.5, places=3)

        agg = aggregate_ramdocs_scores([s1, s2])
        self.assertAlmostEqual(agg["EM_strict"], 0.5, places=4)

    def test_stratified_slicers(self) -> None:
        ram_rows = [
            {"_idx": i, "gold_answers": ["a"] * ((i % 3) + 1), "documents": [{"type": "misinfo" if i % 2 == 0 else "correct"}]}
            for i in range(60)
        ]
        sl = build_ramdocs_stratified_slice(ram_rows, n=18, seed=42)
        self.assertEqual(sl["sample_size"], 18)
        self.assertEqual(len(set(sl["indices"])), 18)

        mh_rows = [
            {"_idx": i, "question_type": ["inference_query", "comparison_query", "temporal_query", "null_query"][i % 4], "evidence_list": [{"fact": "x"}] * (0 if i % 4 == 3 else 2)}
            for i in range(80)
        ]
        mh_sl = build_multihop_stratified_slice(mh_rows, n=20, seed=42)
        self.assertEqual(mh_sl["sample_size"], 20)
        self.assertEqual(mh_sl["non_null_sample_size"], 15)

    def test_binary_filter_and_strict_fork_diff(self) -> None:
        self.assertTrue(is_corrupted_binary_doc("abc \ufffd\ufffd\ufffd\ufffd endstream endobj 123 0 obj"))
        self.assertFalse(is_corrupted_binary_doc("Airways International was an airline based in Miami, Florida."))

        docs = [
            "Airways International was an airline based in Miami, Florida. It operated flights within southern Florida and to the Bahamas from 1984 to 1999.",
            "Airways International was an airline based in Anchorage, Alaska. It operated flights within southern Florida and to the Bahamas from 1984 to 1999.",
            "Unrelated document about completely different aviation museums in Europe and Asia with many exhibits.",
        ]
        diffs = compute_strict_fork_diffs(docs, min_ratio=0.55)
        self.assertEqual(len(diffs), 1)
        self.assertIn("Miami, Florida", diffs[0])
        self.assertIn("Anchorage, Alaska", diffs[0])

    def test_l2_normalize_unit_norm(self) -> None:
        v = l2_normalize([3.0, 4.0])
        self.assertAlmostEqual(v[0], 0.6, places=6)
        self.assertAlmostEqual(v[1], 0.8, places=6)


if __name__ == "__main__":
    unittest.main()
