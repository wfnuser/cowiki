"""Official-aligned evaluation metrics for MultiHop-RAG and RAMDocs.

MultiHop-RAG retrieval metrics mirror `yixuantt/MultiHop-RAG/retrieval_evaluate.py`
line-for-line, plus stricter multi-hop completeness metrics (`AllGold@10`,
`EvidenceRecall@10`) and token-budget accounting.

RAMDocs metrics follow arXiv:2504.13079 (COLM 2025): Exact Match (all gold answers
present and zero wrong/extraneous answers), Precision, Recall, F1, plus diagnostic
metrics (`MisinfoLeakRate`, `FullGoldRecallRate`, `SubstringEM`).
"""

from __future__ import annotations

import re
import string
import unicodedata
from typing import Any, Dict, Iterable, List, Sequence, Set, Tuple


TOKEN_RE = re.compile(r"[A-Za-z0-9_]+|[\u4e00-\u9fff]|[^\s\w]", re.UNICODE)


def estimate_tokens(text: str) -> int:
    """Deterministic subword-like token estimator (aligned with CoWiki M1)."""
    if not text:
        return 0
    raw = TOKEN_RE.findall(text)
    extra = sum(max(0, (len(tok) - 6) // 5) for tok in raw if tok.isascii() and tok.isalnum())
    return len(raw) + extra


def normalize_answer(s: str) -> str:
    """Exact normalization from HanNight/RAMDocs `run_madam_rag.py`."""
    if not s:
        return ""
    s = unicodedata.normalize("NFKD", s).encode("ascii", "ignore").decode("ascii")
    s = s.lower()
    s = "".join(ch for ch in s if ch not in string.punctuation)
    s = re.sub(r"\b(a|an|the)\b", " ", s)
    return " ".join(s.split())


# ============================================================================
# MultiHop-RAG Official Retrieval & QA Evaluation
# ============================================================================

def _compact(text: str) -> str:
    return text.replace(" ", "").replace("\n", "").replace("\r", "").replace("\t", "")


def evaluate_multihop_retrieval(
    records: Sequence[Dict[str, Any]],
) -> Dict[str, Any]:
    """Evaluate MultiHop-RAG retrieval records.

    Each record must contain:
      - `question_type`: str (`null_query` skipped for official retrieval metrics)
      - `retrieval_list`: list of `{"text": str, ...}` (top-K chunks, each <=256 body tokens)
      - `gold_list`: list of `{"fact": str, ...}`
    """
    hits_at_10_count = 0
    hits_at_4_count = 0
    all_gold_at_10_count = 0
    all_gold_at_4_count = 0
    map_at_10_list: List[float] = []
    mrr_list: List[float] = []
    ev_recall_10_list: List[float] = []
    ev_recall_4_list: List[float] = []
    tokens_10_list: List[int] = []
    by_type: Dict[str, List[Dict[str, Any]]] = {}

    valid_count = 0
    for rec in records:
        qtype = rec.get("question_type", "unknown")
        if qtype == "null_query":
            continue
        gold_raw = [m["fact"] for m in rec.get("gold_list", [])]
        if not gold_raw:
            continue
        valid_count += 1
        retrieved_raw = [m["text"] for m in rec.get("retrieval_list", [])]
        tokens_10_list.append(sum(estimate_tokens(t) for t in retrieved_raw[:10]))

        gold = [_compact(g) for g in gold_raw]
        retrieved = [_compact(r) for r in retrieved_raw]

        hits_at_10_flag = False
        hits_at_4_flag = False
        average_precision_sum = 0.0
        first_relevant_rank = None
        found_gold_10: Set[int] = set()
        found_gold_4: Set[int] = set()

        # Official `retrieval_evaluate.py` checks ranks 1..10 (rank <= 10)
        for rank, retrieved_item in enumerate(retrieved[:10], start=1):
            matched_indices = [idx for idx, g in enumerate(gold) if g in retrieved_item]
            if matched_indices:
                hits_at_10_flag = True
                if first_relevant_rank is None:
                    first_relevant_rank = rank
                if rank <= 4:
                    hits_at_4_flag = True
                new_hits = 0
                for idx in matched_indices:
                    if idx not in found_gold_10:
                        found_gold_10.add(idx)
                        new_hits += 1
                    if rank <= 4:
                        found_gold_4.add(idx)
                average_precision_sum += new_hits / rank

        hits_at_10_count += int(hits_at_10_flag)
        hits_at_4_count += int(hits_at_4_flag)
        all_10 = len(found_gold_10) == len(gold)
        all_4 = len(found_gold_4) == len(gold)
        all_gold_at_10_count += int(all_10)
        all_gold_at_4_count += int(all_4)
        map_q = average_precision_sum / min(len(gold), 10)
        mrr_q = (1.0 / first_relevant_rank) if first_relevant_rank else 0.0
        rec10_q = len(found_gold_10) / len(gold)
        rec4_q = len(found_gold_4) / len(gold)

        map_at_10_list.append(map_q)
        mrr_list.append(mrr_q)
        ev_recall_10_list.append(rec10_q)
        ev_recall_4_list.append(rec4_q)

        by_type.setdefault(qtype, []).append(
            {
                "hits_at_10": int(hits_at_10_flag),
                "hits_at_4": int(hits_at_4_flag),
                "all_gold_at_10": int(all_10),
                "map_at_10": map_q,
                "mrr_at_10": mrr_q,
                "ev_recall_10": rec10_q,
            }
        )

    if valid_count == 0:
        return {"valid_queries": 0}

    breakdown = {}
    for qtype, items in sorted(by_type.items()):
        n_t = len(items)
        breakdown[qtype] = {
            "n": n_t,
            "Hits@10": round(sum(x["hits_at_10"] for x in items) / n_t, 4),
            "Hits@4": round(sum(x["hits_at_4"] for x in items) / n_t, 4),
            "MAP@10": round(sum(x["map_at_10"] for x in items) / n_t, 4),
            "MRR@10": round(sum(x["mrr_at_10"] for x in items) / n_t, 4),
            "AllGold@10": round(sum(x["all_gold_at_10"] for x in items) / n_t, 4),
            "EvidenceRecall@10": round(sum(x["ev_recall_10"] for x in items) / n_t, 4),
        }

    return {
        "valid_queries": valid_count,
        "Hits@10": round(hits_at_10_count / valid_count, 4),
        "Hits@4": round(hits_at_4_count / valid_count, 4),
        "MAP@10": round(sum(map_at_10_list) / valid_count, 4),
        "MRR@10": round(sum(mrr_list) / valid_count, 4),
        "AllGold@10": round(all_gold_at_10_count / valid_count, 4),
        "AllGold@4": round(all_gold_at_4_count / valid_count, 4),
        "EvidenceRecall@10": round(sum(ev_recall_10_list) / valid_count, 4),
        "EvidenceRecall@4": round(sum(ev_recall_4_list) / valid_count, 4),
        "avg_top10_tokens": round(sum(tokens_10_list) / valid_count, 1),
        "by_question_type": breakdown,
    }


def evaluate_multihop_qa_official(predictions: Sequence[str], gold_answers: Sequence[str]) -> Dict[str, float]:
    """Official MultiHop-RAG `qa_evaluate.py` token-intersection accuracy."""
    if not gold_answers:
        return {"accuracy": 0.0, "exact_match": 0.0}
    inter_hits = 0
    em_hits = 0
    for pred, gold in zip(predictions, gold_answers):
        m = re.search(r'The answer to the question is "(.*?)"', pred or "")
        extracted = m.group(1) if m else (pred or "")
        if set(extracted.lower().split()).intersection(gold.lower().split()):
            inter_hits += 1
        if normalize_answer(extracted) == normalize_answer(gold):
            em_hits += 1
    n = len(gold_answers)
    return {
        "official_accuracy": round(inter_hits / n, 4),
        "strict_em": round(em_hits / n, 4),
    }


# ============================================================================
# RAMDocs Official Multi-Answer Conflict Evaluation
# ============================================================================

def _answer_matches(pred_norm: str, target_norm: str) -> bool:
    """Match a predicted normalized answer against a target normalized answer.

    Accepts exact normalized match, or clean token-boundary containment when
    one string includes units/qualifiers (e.g. '3559' vs '3559 people').
    """
    if not pred_norm or not target_norm:
        return False
    if pred_norm == target_norm:
        return True
    # Avoid matching overly short 1-2 char tokens inside unrelated words
    shorter, longer = (pred_norm, target_norm) if len(pred_norm) <= len(target_norm) else (target_norm, pred_norm)
    if len(shorter) < 3:
        return False
    return bool(re.search(rf"(?:^|\s){re.escape(shorter)}(?:$|\s)", longer))


def score_ramdocs_instance(
    predicted_answers: Sequence[str],
    gold_answers: Sequence[str],
    wrong_answers: Sequence[str],
) -> Dict[str, Any]:
    """Score a single RAMDocs instance with set-level EM, Precision, Recall, F1.

    - Deduplicates normalized predictions and gold answers.
    - `strict_em`: 1.0 iff EVERY unique gold answer is matched AND EVERY predicted
      answer matches a gold answer (zero wrong answers and zero extraneous answers).
    - `paper_em_relaxed`: 1.0 iff EVERY gold answer is matched AND NO wrong answer
      is matched (matches paper's text-level check for all gold + no misinfo).
    """
    pred_norms: List[str] = []
    seen_p: Set[str] = set()
    for p in predicted_answers:
        pn = normalize_answer(str(p))
        if pn and pn not in ("unknown", "none", "na", "n a") and pn not in seen_p:
            seen_p.add(pn)
            pred_norms.append(pn)

    gold_norms: List[str] = []
    seen_g: Set[str] = set()
    for g in gold_answers:
        gn = normalize_answer(str(g))
        if gn and gn not in seen_g:
            seen_g.add(gn)
            gold_norms.append(gn)

    wrong_norms: List[str] = []
    seen_w: Set[str] = set()
    for w in wrong_answers:
        wn = normalize_answer(str(w))
        if wn and wn not in seen_w and wn not in seen_g:
            seen_w.add(wn)
            wrong_norms.append(wn)

    if not gold_norms:
        em = 1.0 if not pred_norms else 0.0
        return {
            "strict_em": em,
            "paper_em": em,
            "precision": em,
            "recall": em,
            "f1": em,
            "misinfo_leaked": 0.0,
            "full_gold_recall": em,
            "pred_count": len(pred_norms),
            "gold_count": 0,
        }

    # Greedy 1-to-1 matching between predictions and gold answers
    matched_golds: Set[int] = set()
    matched_preds: Set[int] = set()
    for pi, pn in enumerate(pred_norms):
        for gi, gn in enumerate(gold_norms):
            if gi in matched_golds:
                continue
            if _answer_matches(pn, gn):
                matched_golds.add(gi)
                matched_preds.add(pi)
                break

    # Check whether any unmatched prediction matches a known wrong answer
    misinfo_leaked = False
    for pi, pn in enumerate(pred_norms):
        if pi in matched_preds:
            continue
        if any(_answer_matches(pn, wn) for wn in wrong_norms):
            misinfo_leaked = True
            break

    tp = len(matched_golds)
    precision = (tp / len(pred_norms)) if pred_norms else 0.0
    recall = tp / len(gold_norms)
    f1 = (2 * precision * recall / (precision + recall)) if (precision + recall) > 0 else 0.0
    full_gold = tp == len(gold_norms)
    strict_em = 1.0 if (full_gold and len(matched_preds) == len(pred_norms) and not misinfo_leaked) else 0.0
    paper_em = 1.0 if (full_gold and not misinfo_leaked) else 0.0

    return {
        "strict_em": strict_em,
        "paper_em": paper_em,
        "precision": precision,
        "recall": recall,
        "f1": f1,
        "misinfo_leaked": 1.0 if misinfo_leaked else 0.0,
        "full_gold_recall": 1.0 if full_gold else 0.0,
        "pred_count": len(pred_norms),
        "gold_count": len(gold_norms),
    }


def aggregate_ramdocs_scores(per_item: Sequence[Dict[str, Any]]) -> Dict[str, Any]:
    if not per_item:
        return {"n": 0}
    n = len(per_item)
    return {
        "n": n,
        "EM_strict": round(sum(x["strict_em"] for x in per_item) / n, 4),
        "EM_paper": round(sum(x["paper_em"] for x in per_item) / n, 4),
        "Precision": round(sum(x["precision"] for x in per_item) / n, 4),
        "Recall": round(sum(x["recall"] for x in per_item) / n, 4),
        "F1": round(sum(x["f1"] for x in per_item) / n, 4),
        "MisinfoLeakRate": round(sum(x["misinfo_leaked"] for x in per_item) / n, 4),
        "FullGoldRecallRate": round(sum(x["full_gold_recall"] for x in per_item) / n, 4),
    }
