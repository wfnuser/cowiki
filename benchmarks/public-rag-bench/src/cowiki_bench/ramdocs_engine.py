"""RAMDocs Conflict-Resolution Solvers (Standard RAG Baseline, MADAM-1R, CoWiki Compiled Arbiter).

Core insight of CoWiki Compiled Arbiter:
1. Pre-clusters near-duplicate/overlapping passages on CPU (4-gram Jaccard) so
   duplicated misinformation snippets cannot hijack majority vote.
2. Compiles every document into structured OKF-style entity-anchored claims:
   `(doc_id, answers_question, canonical_entity, identifying_traits, candidate_answer,
   entity_name_matches_query, doc_quality)`.
3. Separates:
   - **Inter-Entity Ambiguity (DISTINCT entities sharing an ambiguous name)** -> UNION!
   - **Intra-Entity Factual Conflict (SAME entity with conflicting claims)** -> ARBITRATE
     via independent-source consensus + biographical/chronological internal consistency +
     targeted parametric epistemic verification (Astute RAG principle).
"""

from __future__ import annotations

import json
import re
from collections import defaultdict
from dataclasses import dataclass
from typing import Any, Dict, List, Optional, Sequence, Set, Tuple

from .llm_client import OpenAICompatibleClient
from .metrics import normalize_answer


WORD_RE = re.compile(r"[a-z0-9]+")


def _word_shingles(text: str, k: int = 4) -> Set[Tuple[str, ...]]:
    words = WORD_RE.findall(text.lower())
    if len(words) < k:
        return {tuple(words)} if words else set()
    return {tuple(words[i : i + k]) for i in range(len(words) - k + 1)}


def jaccard_similarity(a: Set[Any], b: Set[Any]) -> float:
    if not a or not b:
        return 0.0
    inter = len(a & b)
    if inter == 0:
        return 0.0
    return inter / len(a | b)


@dataclass
class DocPreprocessInfo:
    doc_id: int
    cluster_id: int
    cluster_size: int
    independent_weight: float
    is_encyclopedic_lead: bool
    is_fragment_or_table: bool


def cluster_near_duplicate_docs(
    documents: Sequence[str],
    jaccard_threshold: float = 0.22,
) -> List[DocPreprocessInfo]:
    """Detect overlapping/duplicate passages purely on CPU using 4-gram Jaccard connected components."""
    n = len(documents)
    shingles = [_word_shingles(d, k=4) for d in documents]
    parent = list(range(n))

    def find(x: int) -> int:
        while parent[x] != x:
            parent[x] = parent[parent[x]]
            x = parent[x]
        return x

    def union(a: int, b: int) -> None:
        ra, rb = find(a), find(b)
        if ra != rb:
            parent[rb] = ra

    for i in range(n):
        for j in range(i + 1, n):
            if jaccard_similarity(shingles[i], shingles[j]) >= jaccard_threshold:
                union(i, j)

    groups: Dict[int, List[int]] = defaultdict(list)
    for i in range(n):
        groups[find(i)].append(i)

    # Assign deterministic cluster IDs 0..K-1
    root_to_cid: Dict[int, int] = {}
    for cid, root in enumerate(sorted(groups.keys())):
        root_to_cid[root] = cid

    infos: List[DocPreprocessInfo] = []
    for i, text in enumerate(documents):
        root = find(i)
        cid = root_to_cid[root]
        csize = len(groups[root])
        stripped = text.strip()
        # Heuristic quality tags (never leak ground truth; purely structural surface cues)
        is_lead = bool(
            re.match(r"^[A-Z][A-Za-z0-9,\-\s\(\)]{2,60}\s+(?:is|was|are|were)\s+", stripped)
            or re.search(r"\(\s*\d{1,2}\s+[A-Z][a-z]+\s+\d{4}\s*[–-]", stripped[:200])
        )
        is_frag = bool(
            stripped.startswith((",", ".", ")", "]", ";", ":"))
            or stripped.count("—") >= 4
            or stripped.count("[ edit ]") >= 2
        )
        infos.append(
            DocPreprocessInfo(
                doc_id=i,
                cluster_id=cid,
                cluster_size=csize,
                independent_weight=round(1.0 / csize, 3),
                is_encyclopedic_lead=is_lead,
                is_fragment_or_table=is_frag,
            )
        )
    return infos


def _extract_json_object(raw: str) -> Dict[str, Any]:
    raw = (raw or "").strip()
    if raw.startswith("```"):
        raw = re.sub(r"^```(?:json)?\s*", "", raw)
        raw = re.sub(r"\s*```$", "", raw)
    try:
        obj = json.loads(raw)
        if isinstance(obj, dict):
            return obj
    except Exception:
        pass
    m = re.search(r"\{.*\}", raw, flags=re.DOTALL)
    if m:
        try:
            obj = json.loads(m.group(0))
            if isinstance(obj, dict):
                return obj
        except Exception:
            pass
    return {}


def _extract_answer_list_from_text(raw: str) -> List[str]:
    obj = _extract_json_object(raw)
    for key in ("answers", "all_correct_answers", "correct_answers", "final_answers"):
        val = obj.get(key)
        if isinstance(val, list):
            return [str(x).strip() for x in val if str(x).strip()]
    m = re.search(r"All Correct Answers\s*:\s*(\[.*?\])", raw, flags=re.IGNORECASE | re.DOTALL)
    if m:
        try:
            arr = json.loads(m.group(1))
            if isinstance(arr, list):
                return [str(x).strip() for x in arr if str(x).strip()]
        except Exception:
            inner = m.group(1).strip("[]")
            return [x.strip().strip('"').strip("'") for x in inner.split(",") if x.strip()]
    return []


# ============================================================================
# Solver 1: Standard RAG Baseline (mirrors arXiv:2504.13079 Standard RAG)
# ============================================================================

def solve_ramdocs_standard_rag(
    question: str,
    documents: Sequence[str],
    client: OpenAICompatibleClient,
    model: str,
) -> Dict[str, Any]:
    doc_block = "\n\n".join(f"[Document {i+1}]\n{d}" for i, d in enumerate(documents))
    prompt = (
        "Answer the question based on the provided documents. "
        "Note that the question may be ambiguous (referring to multiple distinct entities with the same name), "
        "and some documents may contain misinformation or irrelevant noise.\n"
        "Return a JSON object with a single key `answers` containing a list of all concise, correct answers "
        "(without duplicates or wrong/misinformation answers). If no document supports a valid answer, return `{\"answers\": []}`.\n\n"
        f"Question: {question}\n\nDocuments:\n{doc_block}"
    )
    res = client.chat(
        model=model,
        messages=[
            {"role": "system", "content": "You are a precise factual QA system. Output valid JSON only."},
            {"role": "user", "content": prompt},
        ],
        temperature=0.0,
        max_tokens=512,
        json_mode=True,
    )
    answers = _extract_answer_list_from_text(res.content)
    return {
        "answers": answers,
        "prompt_tokens": res.prompt_tokens,
        "completion_tokens": res.completion_tokens,
        "llm_calls": 1,
        "raw_output": res.content,
    }


# ============================================================================
# Solver 2: CoWiki OKF Entity-Clustered Conflict Arbiter (2-Stage Compiled)
# ============================================================================

def solve_ramdocs_cowiki_arbiter(
    question: str,
    documents: Sequence[str],
    client: OpenAICompatibleClient,
    model: str,
) -> Dict[str, Any]:
    """Structured Entity Compilation + Near-Duplicate Weighting + Targeted Conflict Arbitration.

    Stage 1 (1 structured call): Compile every document into an OKF claim record with
      explicit disambiguated entity identity and exact entity-name verification.
    Stage 2 (1 targeted arbitration call): Given the compiled entity clusters AND
      CPU-computed independent-source weights (down-weighting near-duplicate passages!),
      separately handle:
        - Distinct valid disambiguated entities -> UNION all their true answers;
        - Conflicting claims about the SAME entity -> ARBITRATE using independent source
          count, internal timeline/biographical consistency, and factual verification;
        - Near-miss entity names (e.g. 'Hutchison' vs 'Hutchinson') or unrelated people -> REJECT.
    """
    pre_infos = cluster_near_duplicate_docs(documents)
    doc_lines: List[str] = []
    for info, text in zip(pre_infos, documents):
        tags = [f"source_cluster=C{info.cluster_id}", f"indep_weight={info.independent_weight}"]
        if info.is_encyclopedic_lead:
            tags.append("style=lead_bio")
        if info.is_fragment_or_table:
            tags.append("style=fragment_or_table")
        doc_lines.append(f"[Doc {info.doc_id + 1} | {', '.join(tags)}]\n{text}")
    doc_block = "\n\n".join(doc_lines)

    stage1_prompt = f"""Analyze each document independently for the question below and compile structured evidence claims.

Question: {question}

Rules for EVERY document:
1. Determine if the document ACTUALLY answers the question for an entity whose name genuinely matches the target in the question.
   - Watch carefully for lookalike/different spellings or different people/relatives (e.g., "Hutchison" != "Hutchinson", a parent/child of the target, or a completely different work). If it is about a different entity whose name does NOT match the question's ambiguous name, mark `answers_question: false`.
   - If the document is irrelevant noise or lacks the requested fact, mark `answers_question: false`.
2. When `answers_question: true`, identify the SPECIFIC disambiguated entity (e.g. "Chayanne (1987 album)" vs "Chayanne (1988 album)", or "Richard Griffin, 2nd Baron Braybrooke" vs "Richard Neville Aldworth Neville") so that two distinct real entities sharing an ambiguous name get DIFFERENT `canonical_entity` names, while two documents about the SAME entity get the EXACT SAME `canonical_entity` name.
3. Extract the concise `candidate_answer` exactly as supported by that document (keep concise units/dates matching the question, e.g. "3,559 people", "1987", "3 July 1750").
4. Note any internal inconsistency or signs of tampered/fabricated facts inside the passage in `suspicion_note` (empty string if none).

Return strict JSON:
{{
  "claims": [
    {{
      "doc_id": 1,
      "answers_question": true,
      "canonical_entity": "Specific disambiguated entity name",
      "candidate_answer": "concise answer string",
      "evidence_quote": "short verbatim quote",
      "suspicion_note": ""
    }}
  ]
}}

Documents:
{doc_block}"""

    r1 = client.chat(
        model=model,
        messages=[
            {"role": "system", "content": "You are an expert knowledge compiler for conflicting multi-source documents. Output valid JSON only."},
            {"role": "user", "content": stage1_prompt},
        ],
        temperature=0.0,
        max_tokens=1500,
        json_mode=True,
    )
    total_pt = r1.prompt_tokens
    total_ct = r1.completion_tokens
    llm_calls = 1

    compiled = _extract_json_object(r1.content)
    claims = compiled.get("claims", [])
    if not isinstance(claims, list):
        claims = []

    # Attach CPU near-duplicate metadata and group by canonical entity
    info_by_id = {info.doc_id + 1: info for info in pre_infos}
    active_claims: List[Dict[str, Any]] = []
    entity_groups: Dict[str, List[Dict[str, Any]]] = defaultdict(list)

    for cl in claims:
        if not isinstance(cl, dict) or not cl.get("answers_question"):
            continue
        ans = str(cl.get("candidate_answer") or "").strip()
        if not ans or normalize_answer(ans) in ("unknown", "none", "na", ""):
            continue
        did = int(cl.get("doc_id", 0))
        info = info_by_id.get(did)
        ent_raw = str(cl.get("canonical_entity") or "default").strip()
        ent_key = normalize_answer(ent_raw) or "default"
        enriched = {
            "doc_id": did,
            "canonical_entity": ent_raw,
            "candidate_answer": ans,
            "evidence_quote": str(cl.get("evidence_quote") or "")[:200],
            "suspicion_note": str(cl.get("suspicion_note") or ""),
            "source_cluster": f"C{info.cluster_id}" if info else "C?",
            "independent_weight": info.independent_weight if info else 1.0,
            "is_lead_bio": info.is_encyclopedic_lead if info else False,
        }
        active_claims.append(enriched)
        entity_groups[ent_key].append(enriched)

    if not active_claims:
        return {
            "answers": [],
            "prompt_tokens": total_pt,
            "completion_tokens": total_ct,
            "llm_calls": llm_calls,
            "compiled_claims": [],
        }

    # Build compact cluster summary for Stage 2 Arbitration
    cluster_summary: List[Dict[str, Any]] = []
    for ent_key, g_claims in entity_groups.items():
        by_ans: Dict[str, Dict[str, Any]] = {}
        for c in g_claims:
            an = normalize_answer(c["candidate_answer"])
            if an not in by_ans:
                by_ans[an] = {
                    "answer": c["candidate_answer"],
                    "doc_ids": [],
                    "distinct_source_clusters": set(),
                    "independent_support_score": 0.0,
                    "has_lead_bio": False,
                    "quotes": [],
                    "suspicions": [],
                }
            slot = by_ans[an]
            slot["doc_ids"].append(c["doc_id"])
            slot["distinct_source_clusters"].add(c["source_cluster"])
            slot["independent_support_score"] = round(
                slot["independent_support_score"] + c["independent_weight"], 3
            )
            slot["has_lead_bio"] = slot["has_lead_bio"] or c["is_lead_bio"]
            if c["evidence_quote"]:
                slot["quotes"].append(c["evidence_quote"])
            if c["suspicion_note"]:
                slot["suspicions"].append(c["suspicion_note"])

        candidates_out = []
        for slot in by_ans.values():
            candidates_out.append(
                {
                    "answer": slot["answer"],
                    "raw_doc_count": len(slot["doc_ids"]),
                    "independent_source_clusters": len(slot["distinct_source_clusters"]),
                    "has_encyclopedic_lead": slot["has_lead_bio"],
                    "quotes": slot["quotes"][:2],
                    "suspicions": slot["suspicions"][:2],
                }
            )
        cluster_summary.append(
            {
                "entity": g_claims[0]["canonical_entity"],
                "has_intra_entity_conflict": len(candidates_out) > 1,
                "candidate_answers": candidates_out,
            }
        )

    # Always run Stage 2 Arbitration (compact JSON input, low token cost!) so we catch:
    # (a) single-doc misinformation injected for a real entity,
    # (b) intra-entity conflicts where near-duplicate misinfo tried to outvote truth,
    # (c) mistakenly split identical entities or wrong-entity leaks.
    stage2_prompt = f"""You are a rigorous Fact-Checking & Disambiguation Arbiter for conflicting RAG documents.

Question: {question}

Below are compiled entity-level candidate answers extracted from the retrieved documents, annotated with `independent_source_clusters` (near-duplicate passages from the same text were already merged so duplicated misinformation cannot fake a majority!).

Compiled Entity Clusters:
{json.dumps(cluster_summary, indent=2, ensure_ascii=False)}

Arbitration Rules (FOLLOW STRICTLY):
1. **AMBIGUITY = UNION across genuinely DISTINCT entities**: If the question's name refers to 2 or 3 distinct real entities (e.g., two albums from different years, two different people with the same name, two towns in different states), you MUST include the SINGLE true answer for EACH distinct valid entity! Do NOT drop a valid entity just because another entity is more famous.
2. **CONFLICT WITHIN THE SAME ENTITY = ARBITRATE & PICK ONLY THE TRUE FACT**: If two candidate answers refer to the SAME entity (even if Stage 1 accidentally split the same entity into two entries!), only ONE fact can be true—the other is misinformation! To identify the misinformation:
   - Check **factual truth** against your reliable encyclopedic knowledge (dates, historical facts, biographies, geography, sports, films, albums).
   - Prefer higher `independent_source_clusters` and `has_encyclopedic_lead: true`, NOT raw duplicated snippets.
   - Check internal chronological/logical consistency (e.g., a person cannot win a championship before being born).
   - NEVER output both the true answer and the misinformation answer for the same entity!
3. **SINGLE-SOURCE MISINFORMATION CHECK**: Even if an entity only has one candidate answer, verify whether that claim is factually contradicted by well-established encyclopedic knowledge or looks like a fabricated/corrupted fact (or wrong entity spelling like Hutchison vs Hutchinson). Drop confirmed misinformation or wrong-entity claims, while keeping legitimate obscure facts supported by authentic encyclopedic text.
4. **CONCISE FORMAT**: Match the concise answer phrasing directly supported by the winning document (e.g. "1987", "3 July 1750", "3,559 people", "American football") without adding extra commentary inside the answer strings.

Return strict JSON:
{{
  "reasoning": "concise entity-by-entity arbitration",
  "final_answers": ["ans1", "ans2"]
}}"""

    r2 = client.chat(
        model=model,
        messages=[
            {"role": "system", "content": "You are a calibrated epistemic arbiter. Output valid JSON only."},
            {"role": "user", "content": stage2_prompt},
        ],
        temperature=0.0,
        max_tokens=700,
        json_mode=True,
    )
    total_pt += r2.prompt_tokens
    total_ct += r2.completion_tokens
    llm_calls += 1

    final_answers = _extract_answer_list_from_text(r2.content)
    return {
        "answers": final_answers,
        "prompt_tokens": total_pt,
        "completion_tokens": total_ct,
        "llm_calls": llm_calls,
        "compiled_clusters": cluster_summary,
        "arbiter_raw": r2.content,
    }
