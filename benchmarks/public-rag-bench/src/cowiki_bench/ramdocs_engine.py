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

import difflib
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


def is_corrupted_binary_doc(text: str) -> bool:
    """Detect raw binary / corrupted PDF stream noise documents purely on CPU."""
    if not text or len(text.strip()) < 15:
        return True
    sample = text[:800]
    bad = sum(1 for ch in sample if ch == "\ufffd" or (ord(ch) < 32 and ch not in "\n\r\t"))
    if bad / max(1, len(sample)) >= 0.05:
        return True
    if "endstream" in sample and "endobj" in sample:
        return True
    alpha = sum(1 for ch in sample if ch.isalnum())
    if alpha / max(1, len(sample)) < 0.35:
        return True
    return False


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


# ============================================================================
# Solver 3: CoWiki Compiled Arbiter v2 (Binary-Clean + Grounded Recall-Preserving)
# ============================================================================

def solve_ramdocs_cowiki_arbiter_v2(
    question: str,
    documents: Sequence[str],
    client: OpenAICompatibleClient,
    model: str,
) -> Dict[str, Any]:
    """v2 upgrades over v1:
    1. CPU corrupted-binary/PDF-stream filter before LLM calls (prevents JSON/prompt poisoning).
    2. Recall-preserving excerpt-aware Stage 1 extraction (accepts nicknames, romanizations,
       acronyms, and title-omitted Wikipedia body excerpts like 'the city' vs 'the county').
    3. Hard Document-Grounding Invariant (NEVER replaces document answers with parametric numbers).
    4. Exact short verbatim phrase preservation (avoids synonym drift like 'ethnic group' vs 'indigenous people').
    5. Default-KEEP across distinct disambiguated entities; STRICT intra-entity conflict arbitration
       using CPU independent-source clusters + factual/coherence verification.
    """
    clean_indices = [i for i, d in enumerate(documents) if not is_corrupted_binary_doc(d)]
    if not clean_indices:
        clean_indices = list(range(len(documents)))
    clean_docs = [documents[i] for i in clean_indices]

    pre_infos = cluster_near_duplicate_docs(clean_docs)
    doc_lines: List[str] = []
    for info, text in zip(pre_infos, clean_docs):
        tags = [f"cluster=C{info.cluster_id}", f"indep_w={info.independent_weight}"]
        if info.is_encyclopedic_lead:
            tags.append("lead_bio")
        doc_lines.append(f"[Doc {info.doc_id + 1} | {', '.join(tags)}]\n{text}")
    doc_block = "\n\n".join(doc_lines)

    stage1_prompt = f"""You are compiling structured claims from retrieved document excerpts for an ambiguous multi-entity QA task.

Question: {question}

CRITICAL EXTRACTION RULES:
1. **Excerpts often omit the full title or use variants**: A passage from an encyclopedia article may refer to the subject as "the city", "the county", "the district", "he", "she", a surname, a nickname (e.g., "Mike" for "Michael"), a spelling variant/romanization, or an expansion of an acronym. DO NOT discard a passage just because the exact query string is not repeated verbatim!
2. **When is a document `noise` (`answers_question: false`)?** ONLY when it genuinely does NOT contain an answer to the question (e.g., navigation menus, unrelated biography/filmography/list of other people, or background without the requested fact).
3. **Distinguish DIFFERENT disambiguated entities**: Give each distinct real-world entity a clear distinguishing `canonical_entity` (e.g., "Springfield (city)" vs "Springfield (county)", "Michael Jordan (basketball)" vs "Michael Jordan (computer scientist)", "Work X (1987 film)" vs "Work X (1992 album)"). Give two passages about the SAME entity the SAME `canonical_entity`.
4. **Verbatim concise answer**: Extract `candidate_answer` as the SHORTEST exact phrase directly from the document text (typically 1-5 words). Do NOT paraphrase or rewrite into longer descriptions!
5. **One primary answer per document-entity**: If a passage mentions a historical census table or multiple dates, pick the single primary headline figure/answer for that entity.
6. **Do NOT filter out suspected misinformation yet** in Stage 1 — extract ALL document-supported claims faithfully, and put any doubt in `suspicion_note`.

Return strict JSON:
{{
  "claims": [
    {{
      "doc_id": 1,
      "answers_question": true,
      "canonical_entity": "Distinct disambiguated entity",
      "candidate_answer": "concise exact answer from text",
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
            {"role": "system", "content": "You are a high-recall knowledge compiler. Extract grounded claims faithfully as valid JSON."},
            {"role": "user", "content": stage1_prompt},
        ],
        temperature=0.0,
        max_tokens=1600,
        json_mode=True,
    )
    total_pt = r1.prompt_tokens
    total_ct = r1.completion_tokens
    llm_calls = 1

    compiled = _extract_json_object(r1.content)
    claims = compiled.get("claims", [])
    if not isinstance(claims, list):
        claims = []

    info_by_id = {info.doc_id + 1: info for info in pre_infos}
    entity_groups: Dict[str, List[Dict[str, Any]]] = defaultdict(list)
    all_extracted_answers: List[str] = []

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
            "evidence_quote": str(cl.get("evidence_quote") or "")[:220],
            "suspicion_note": str(cl.get("suspicion_note") or ""),
            "source_cluster": f"C{info.cluster_id}" if info else "C?",
            "independent_weight": info.independent_weight if info else 1.0,
            "is_lead_bio": info.is_encyclopedic_lead if info else False,
        }
        entity_groups[ent_key].append(enriched)
        all_extracted_answers.append(ans)

    if not entity_groups:
        # Grounded safety fallback to standard RAG if Stage 1 returned empty JSON
        fb = solve_ramdocs_standard_rag(question, clean_docs, client, model)
        return {
            "answers": fb["answers"],
            "prompt_tokens": total_pt + fb["prompt_tokens"],
            "completion_tokens": total_ct + fb["completion_tokens"],
            "llm_calls": llm_calls + fb["llm_calls"],
            "fallback_used": True,
        }

    # Build compact cluster table for Stage 2 Arbitration
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

        candidates_out = [
            {
                "answer": slot["answer"],
                "raw_doc_count": len(slot["doc_ids"]),
                "independent_source_clusters": len(slot["distinct_source_clusters"]),
                "has_encyclopedic_lead": slot["has_lead_bio"],
                "quotes": slot["quotes"][:2],
                "suspicions": slot["suspicions"][:2],
            }
            for slot in by_ans.values()
        ]
        cluster_summary.append(
            {
                "entity": g_claims[0]["canonical_entity"],
                "has_intra_entity_conflict": len(candidates_out) > 1,
                "candidate_answers": candidates_out,
            }
        )

    stage2_prompt = f"""You are a Grounded Multi-Entity Disambiguation & Misinformation Arbiter.

Question: {question}

Compiled Entity-Level Candidates (near-duplicate passages have already been collapsed into `independent_source_clusters`):
{json.dumps(cluster_summary, indent=2, ensure_ascii=False)}

STRICT ARBITRATION RULES:
1. **STRICT DOCUMENT GROUNDING (ZERO HALLUCINATION)**: Every answer in `final_answers` MUST be copied directly (concisely) from one of the `answer` strings in the candidates above! NEVER invent, update, or substitute a number/name from outside memory.
2. **AMBIGUITY = DEFAULT KEEP (UNION) ACROSS DISTINCT ENTITIES**: Queries are frequently ambiguous and genuinely refer to 2 or 3 DIFFERENT real-world entities (e.g., a city vs a county/district; two athletes in different sports; two works with the same title in different years/media; two expansions of an acronym).
   - **A valid secondary entity frequently has ONLY 1 supporting document!** NEVER drop an entity merely because `raw_doc_count == 1` or because another entity has more documents!
   - Keep ONE winning answer for EVERY distinct genuine entity supported by the passages.
3. **CONFLICT WITHIN THE SAME ENTITY = PICK ONLY THE TRUE FACT**: If two candidate answers refer to the SAME specific entity (even if Stage 1 gave them slightly different entity labels), they CANNOT both be right—one is misinformation (or a secondary/outdated figure)! Pick ONLY the true one by checking:
   - Well-established factual truth (historical dates, biographies, sports, creative works, political parties — reject obvious synthetic swaps, implausibly famous celebrity/entity substitutions, or implausible placeholder/round numbers that contradict specific authentic data);
   - Higher `independent_source_clusters` and `has_encyclopedic_lead: true` (ignoring duplicate-inflated `raw_doc_count`);
   - Internal chronological and contextual coherence with the rest of the quote.
4. **WHEN TO REJECT A SINGLE-CANDIDATE ENTITY**: ONLY reject a single-candidate entity if (a) it is clearly a fabricated misinformation claim contradicting known reality or an implausible synthetic swap, OR (b) it is clearly about an unrelated person/entity that does NOT match the question (e.g., a different surname or family member). When in doubt about an obscure real place/person/organization whose passage reads like authentic encyclopedic prose, **KEEP IT**!
5. **EXACT CONCISE PHRASING**: Copy the concise candidate answer string cleanly (do NOT paraphrase into longer sentences or synonyms). Never output an empty list if valid grounded candidates exist.

Return strict JSON:
{{
  "reasoning": "concise entity-by-entity decision",
  "final_answers": ["ans1", "ans2"]
}}"""

    r2 = client.chat(
        model=model,
        messages=[
            {"role": "system", "content": "You are a grounded epistemic arbiter. Output valid JSON only."},
            {"role": "user", "content": stage2_prompt},
        ],
        temperature=0.0,
        max_tokens=800,
        json_mode=True,
    )
    total_pt += r2.prompt_tokens
    total_ct += r2.completion_tokens
    llm_calls += 1

    final_answers = _extract_answer_list_from_text(r2.content)
    # Never-empty grounded safeguard: if arbiter accidentally emptied everything, keep highest-support candidate(s)
    if not final_answers and cluster_summary:
        for cl in cluster_summary:
            best = max(cl["candidate_answers"], key=lambda x: (x["independent_source_clusters"], x["has_encyclopedic_lead"]))
            final_answers.append(best["answer"])

    return {
        "answers": final_answers,
        "prompt_tokens": total_pt,
        "completion_tokens": total_ct,
        "llm_calls": llm_calls,
        "compiled_clusters": cluster_summary,
        "arbiter_raw": r2.content,
    }


# ============================================================================
# Solver 4: CoWiki Diff-Provenance + Identity-Collision Conflict Arbiter (v3)
# ============================================================================

YEAR_RE = re.compile(r"\b(1[5-9]\d{2}|20[0-2]\d)\b")
CAP_WORD_RE = re.compile(r"\b[A-Z][a-z]{2,}\b")


def compute_provenance_audit_notes(
    clean_docs: Sequence[str],
    pre_infos: Sequence[DocPreprocessInfo],
) -> List[str]:
    """CoWiki Diff-Provenance & Identity-Collision Pre-Audit (100% CPU, zero leakage).

    1. For every near-duplicate source cluster (size >= 2), computes exact word-level
       `difflib` hunks exposing localized factual edits or injected/appended sentences.
    2. Across non-duplicate pairs, detects biographical/temporal fingerprint collisions
       (>= 2 identical specific years + >= 2 shared capitalized name tokens) so tampered
       infoboxes/rewrites of the SAME person are flagged before entity splitting.
    """
    notes: List[str] = []
    by_cluster: Dict[int, List[int]] = defaultdict(list)
    for info in pre_infos:
        by_cluster[info.cluster_id].append(info.doc_id)

    # 1. Pairwise word-level diff inside near-duplicate clusters
    for cid, members in sorted(by_cluster.items()):
        if len(members) < 2:
            continue
        base_id = members[0]
        base_words = clean_docs[base_id].split()
        for other_id in members[1:]:
            other_words = clean_docs[other_id].split()
            sm = difflib.SequenceMatcher(None, base_words, other_words, autojunk=False)
            ratio = sm.ratio()
            hunks: List[str] = []
            for tag, i1, i2, j1, j2 in sm.get_opcodes():
                if tag == "equal":
                    continue
                ctx_l = " ".join(base_words[max(0, i1 - 3) : i1])
                ctx_r = " ".join(base_words[i2 : min(len(base_words), i2 + 3)])
                a_seg = " ".join(base_words[i1:i2])
                b_seg = " ".join(other_words[j1:j2])
                if len(a_seg) + len(b_seg) < 2:
                    continue
                if tag == "replace":
                    hunks.append(f"REPLACE (...{ctx_l}) [Doc {base_id+1}: {a_seg[:140]!r} vs Doc {other_id+1}: {b_seg[:140]!r}] ({ctx_r}...)")
                elif tag == "insert":
                    hunks.append(f"INSERTED ONLY IN Doc {other_id+1} (...{ctx_l}): {b_seg[:220]!r}")
                elif tag == "delete":
                    hunks.append(f"INSERTED ONLY IN Doc {base_id+1} (...{ctx_l}): {a_seg[:220]!r}")
                if len(hunks) >= 5:
                    break
            if hunks:
                notes.append(
                    f"[NEAR-DUPLICATE FORK AUDIT: Doc {base_id+1} & Doc {other_id+1} share {ratio:.0%} text "
                    f"(SAME underlying passage & entity — NEVER treat as two separate entities!). Exact diffs: "
                    + " ; ".join(hunks)
                    + "]"
                )
            else:
                notes.append(
                    f"[EXACT DUPLICATE: Doc {base_id+1} & Doc {other_id+1} are verbatim copies of the same passage (count as 1 source).]"
                )

    # 2. Biographical / temporal identity-collision alerts across distinct clusters
    doc_years = [set(YEAR_RE.findall(d)) for d in clean_docs]
    common_caps = {"The", "And", "For", "With", "From", "This", "That", "Wikipedia", "Jump", "Edit", "External", "References", "Links", "Main", "Page", "Contents", "History", "Retrieved", "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December", "United", "States"}
    doc_caps = [{w for w in CAP_WORD_RE.findall(d) if w not in common_caps} for d in clean_docs]

    n = len(clean_docs)
    for i in range(n):
        for j in range(i + 1, n):
            if pre_infos[i].cluster_id == pre_infos[j].cluster_id:
                continue
            shared_years = sorted(doc_years[i] & doc_years[j])
            shared_caps = sorted(doc_caps[i] & doc_caps[j])
            if len(shared_years) >= 2 and len(shared_caps) >= 3:
                notes.append(
                    f"[IDENTITY-COLLISION ALERT: Doc {i+1} & Doc {j+1} share exact years {shared_years[:5]} and "
                    f"proper nouns {shared_caps[:6]}. Check whether they describe the EXACT SAME person/entity "
                    f"with a tampered fact (e.g., altered sport/rank/date/location) rather than two distinct entities!]"
                )
                if len(notes) >= 12:
                    return notes
    return notes


def solve_ramdocs_cowiki_arbiter_v3(
    question: str,
    documents: Sequence[str],
    client: OpenAICompatibleClient,
    model: str,
) -> Dict[str, Any]:
    """v3: CoWiki Diff-Provenance + Identity-Collision Conflict Arbiter.

    Combines v2's high-recall excerpt-aware grounded compilation with CoWiki M2's
    explicit pairwise near-duplicate diff audit and biographical identity-collision
    detection so tampered forks and mutated infoboxes cannot disguise themselves as
    distinct disambiguated entities.
    """
    clean_indices = [i for i, d in enumerate(documents) if not is_corrupted_binary_doc(d)]
    if not clean_indices:
        clean_indices = list(range(len(documents)))
    clean_docs = [documents[i] for i in clean_indices]

    pre_infos = cluster_near_duplicate_docs(clean_docs)
    audit_notes = compute_provenance_audit_notes(clean_docs, pre_infos)

    doc_lines: List[str] = []
    for info, text in zip(pre_infos, clean_docs):
        tags = [f"cluster=C{info.cluster_id}", f"indep_w={info.independent_weight}"]
        if info.is_encyclopedic_lead:
            tags.append("lead_bio")
        doc_lines.append(f"[Doc {info.doc_id + 1} | {', '.join(tags)}]\n{text}")
    doc_block = "\n\n".join(doc_lines)
    audit_block = "\n".join(audit_notes) if audit_notes else "(No near-duplicate forks or identity collisions detected.)"

    stage1_prompt = f"""You are compiling structured claims from retrieved document excerpts for an ambiguous multi-entity QA task with possible document tampering/misinformation.

Question: {question}

Automated CoWiki Provenance & Diff Audit:
{audit_block}

CRITICAL EXTRACTION RULES:
1. **Excerpts often omit the full title or use variants**: Passages may refer to the subject as "the city", "the county", "the district", "he", "she", a surname, nickname, spelling variant/romanization, or acronym expansion. Do NOT discard valid excerpts just because the exact query string isn't repeated verbatim.
2. **Same-Entity Identity Binding (CRITICAL!)**:
   - If two documents are flagged in a **NEAR-DUPLICATE FORK AUDIT**, they are ALWAYS two versions of the **EXACT SAME entity** — give them the **EXACT SAME `canonical_entity`**!
   - If two documents share the **same person/place identity fingerprint** (e.g., same full name + same birth/death years or lifespan or founding year/institution), they are the **EXACT SAME entity** even if one document alters their sport, military branch, rank, date, or city! Give them the **EXACT SAME `canonical_entity`** (e.g. "Person X (1891-1964)" rather than splitting by tampered branch/sport)!
   - Conversely, genuinely DIFFERENT entities (different birth/death dates, different states/counties, city vs county, different release years of distinct works, different acronym expansions) MUST get DIFFERENT `canonical_entity` names.
3. **Tamper Forensics**: When a NEAR-DUPLICATE FORK AUDIT shows one doc inserted/appended a jarring contradictory sentence into an otherwise verbatim copy of another doc, or swapped a fact that contradicts the shared title/surrounding text (e.g., wrong state vs surrounding county/region, wrong branch vs shared awards/service), explicitly record this in `tamper_flag: true` and `suspicion_note`.
4. **Verbatim concise answer**: Extract `candidate_answer` as the SHORTEST exact phrase directly from the document text (typically 1-5 words). Pick the single primary headline answer per document-entity (do not dump historical census lists).
5. **Noise filtering**: Mark `answers_question: false` ONLY when a document genuinely does not answer the question (menus, unrelated people, background without the requested fact).

Return strict JSON:
{{
  "claims": [
    {{
      "doc_id": 1,
      "answers_question": true,
      "canonical_entity": "Specific entity anchored by distinguishing dates/place (NOT tampered attribute)",
      "candidate_answer": "concise exact answer from text",
      "evidence_quote": "short verbatim quote",
      "tamper_flag": false,
      "suspicion_note": ""
    }}
  ]
}}

Documents:
{doc_block}"""

    r1 = client.chat(
        model=model,
        messages=[
            {"role": "system", "content": "You are a forensic knowledge compiler. Bind tampered forks to the same entity and output valid JSON only."},
            {"role": "user", "content": stage1_prompt},
        ],
        temperature=0.0,
        max_tokens=1600,
        json_mode=True,
    )
    total_pt = r1.prompt_tokens
    total_ct = r1.completion_tokens
    llm_calls = 1

    compiled = _extract_json_object(r1.content)
    claims = compiled.get("claims", [])
    if not isinstance(claims, list):
        claims = []

    info_by_id = {info.doc_id + 1: info for info in pre_infos}
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
        entity_groups[ent_key].append(
            {
                "doc_id": did,
                "canonical_entity": ent_raw,
                "candidate_answer": ans,
                "evidence_quote": str(cl.get("evidence_quote") or "")[:220],
                "tamper_flag": bool(cl.get("tamper_flag", False)),
                "suspicion_note": str(cl.get("suspicion_note") or ""),
                "source_cluster": f"C{info.cluster_id}" if info else "C?",
                "independent_weight": info.independent_weight if info else 1.0,
                "is_lead_bio": info.is_encyclopedic_lead if info else False,
            }
        )

    if not entity_groups:
        fb = solve_ramdocs_standard_rag(question, clean_docs, client, model)
        return {
            "answers": fb["answers"],
            "prompt_tokens": total_pt + fb["prompt_tokens"],
            "completion_tokens": total_ct + fb["completion_tokens"],
            "llm_calls": llm_calls + fb["llm_calls"],
            "fallback_used": True,
        }

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
                    "has_lead_bio": False,
                    "tamper_flagged_count": 0,
                    "quotes": [],
                    "suspicions": [],
                }
            slot = by_ans[an]
            slot["doc_ids"].append(c["doc_id"])
            slot["distinct_source_clusters"].add(c["source_cluster"])
            slot["has_lead_bio"] = slot["has_lead_bio"] or c["is_lead_bio"]
            if c["tamper_flag"]:
                slot["tamper_flagged_count"] += 1
            if c["evidence_quote"]:
                slot["quotes"].append(c["evidence_quote"])
            if c["suspicion_note"]:
                slot["suspicions"].append(c["suspicion_note"])

        candidates_out = [
            {
                "answer": slot["answer"],
                "doc_ids": slot["doc_ids"],
                "independent_source_clusters": len(slot["distinct_source_clusters"]),
                "has_encyclopedic_lead": slot["has_lead_bio"],
                "tamper_flagged": slot["tamper_flagged_count"] > 0,
                "quotes": slot["quotes"][:2],
                "suspicions": slot["suspicions"][:2],
            }
            for slot in by_ans.values()
        ]
        cluster_summary.append(
            {
                "entity": g_claims[0]["canonical_entity"],
                "has_intra_entity_conflict": len(candidates_out) > 1,
                "candidate_answers": candidates_out,
            }
        )

    stage2_prompt = f"""You are a Forensic Multi-Entity Disambiguation & Tamper-Resolution Arbiter.

Question: {question}

Automated Provenance & Diff Audit (ground-truth structural evidence from the raw passages):
{audit_block}

Compiled Entity-Level Candidates:
{json.dumps(cluster_summary, indent=2, ensure_ascii=False)}

STRICT DECISION RULES:
1. **STRICT DOCUMENT GROUNDING**: Every string in `final_answers` MUST come concisely from the candidate `answer` strings above. NEVER invent or substitute an answer from outside memory!
2. **NEAR-DUPLICATE FORK RESOLUTION**: Whenever the Provenance Audit shows two documents are near-duplicate versions of the SAME passage (`NEAR-DUPLICATE FORK AUDIT`), they describe the SAME entity — pick ONLY the authentic original version and DISCARD the tampered fork!
   - A version that inserts/appends an out-of-place contradictory claim into a verbatim copy of another doc is ALWAYS the tampered fork!
   - A version whose swapped location/date/rank/sport contradicts the shared title or surrounding sentences is ALWAYS the tampered fork!
3. **IDENTITY-COLLISION CHECK**: If two entity entries actually describe the SAME person/place/work (same lifespan/birth-death dates, same founding year, same organization) with a conflicting attribute (e.g., two different sports, two different military branches/ranks, two different birth dates for the same person), MERGE them and keep ONLY the ONE factually authentic answer!
4. **UNION ACROSS GENUINELY DISTINCT ENTITIES**: Once tampered forks and same-entity conflicts are eliminated, KEEP the ONE winning concise answer for EVERY genuinely distinct real entity (even if a valid secondary entity only has 1 supporting document!).
   - Reject clearly fabricated/wrong-entity claims or synthetic misinfo swaps, while preserving authentic obscure encyclopedic entities.
5. **CONCISE VERBATIM PHRASING**: Keep answers short (typically 1-5 words directly matching the document phrasing). Never return an empty list if valid authentic candidates exist.

Return strict JSON:
{{
  "reasoning": "concise forensic resolution per entity",
  "final_answers": ["ans1", "ans2"]
}}"""

    r2 = client.chat(
        model=model,
        messages=[
            {"role": "system", "content": "You are a forensic epistemic arbiter. Output valid JSON only."},
            {"role": "user", "content": stage2_prompt},
        ],
        temperature=0.0,
        max_tokens=800,
        json_mode=True,
    )
    total_pt += r2.prompt_tokens
    total_ct += r2.completion_tokens
    llm_calls += 1

    final_answers = _extract_answer_list_from_text(r2.content)
    if not final_answers and cluster_summary:
        for cl in cluster_summary:
            clean_cands = [x for x in cl["candidate_answers"] if not x.get("tamper_flagged")] or cl["candidate_answers"]
            best = max(clean_cands, key=lambda x: (x["independent_source_clusters"], x["has_encyclopedic_lead"]))
            final_answers.append(best["answer"])

    return {
        "answers": final_answers,
        "prompt_tokens": total_pt,
        "completion_tokens": total_ct,
        "llm_calls": llm_calls,
        "audit_notes": audit_notes,
        "compiled_clusters": cluster_summary,
        "arbiter_raw": r2.content,
    }


# ============================================================================
# Solver 5: CoWiki Surgical Fork-Diff Arbiter (v4: High-Recall Stage 1 + Strict Fork Pruning)
# ============================================================================

WIKI_CHROME_RE = re.compile(
    r"(?:Jump to content|Main menu|move to sidebar|Toggle the table of contents|Edit links|Creative Commons Attribution|From Wikipedia\s*,?\s*the free encyclopedia|Privacy [Pp]olicy|Personal tools|Create account|Log in)",
    re.IGNORECASE,
)


def compute_strict_fork_diffs(
    clean_docs: Sequence[str],
    min_ratio: float = 0.55,
) -> List[str]:
    """High-precision near-duplicate fork detector (CPU only).

    Triggers ONLY when two non-boilerplate passages share >= `min_ratio`
    sequence similarity (`difflib.SequenceMatcher`), exposing localized factual
    swaps or injected/appended sentences without false-firing on unrelated entities.
    """
    notes: List[str] = []
    n = len(clean_docs)
    # Strip Wikipedia UI boilerplate before measuring overlap so two unrelated wiki navbars never match
    stripped_words = [WIKI_CHROME_RE.sub(" ", d).split() for d in clean_docs]
    shingles = [_word_shingles(" ".join(w), k=4) for w in stripped_words]

    for i in range(n):
        if len(stripped_words[i]) < 15:
            continue
        for j in range(i + 1, n):
            if len(stripped_words[j]) < 15:
                continue
            # Fast pre-filter before SequenceMatcher
            if jaccard_similarity(shingles[i], shingles[j]) < 0.25:
                continue
            sm = difflib.SequenceMatcher(None, stripped_words[i], stripped_words[j], autojunk=False)
            ratio = sm.ratio()
            if ratio < min_ratio:
                continue
            hunks: List[str] = []
            for tag, i1, i2, j1, j2 in sm.get_opcodes():
                if tag == "equal":
                    continue
                ctx_l = " ".join(stripped_words[i][max(0, i1 - 3) : i1])
                ctx_r = " ".join(stripped_words[i][i2 : min(len(stripped_words[i]), i2 + 3)])
                a_seg = " ".join(stripped_words[i][i1:i2])
                b_seg = " ".join(stripped_words[j][j1:j2])
                if len(a_seg) + len(b_seg) < 2:
                    continue
                if tag == "replace":
                    hunks.append(f"DIFF (...{ctx_l}) [Doc {i+1}: {a_seg[:140]!r} vs Doc {j+1}: {b_seg[:140]!r}] ({ctx_r}...)")
                elif tag == "insert":
                    hunks.append(f"EXTRA SENTENCE ONLY IN Doc {j+1} (...{ctx_l}): {b_seg[:220]!r}")
                elif tag == "delete":
                    hunks.append(f"EXTRA SENTENCE ONLY IN Doc {i+1} (...{ctx_l}): {a_seg[:220]!r}")
                if len(hunks) >= 4:
                    break
            if hunks:
                doc_a_lead = " ".join(stripped_words[i][:28])
                doc_b_lead = " ".join(stripped_words[j][:28])
                notes.append(
                    f"[FORK DIFF: Doc {i+1} & Doc {j+1} are {ratio:.0%}-identical versions of the SAME source passage (same entity!). "
                    f"Doc {i+1} opening: {doc_a_lead!r} | Doc {j+1} opening: {doc_b_lead!r} | "
                    + " ; ".join(hunks)
                    + "]"
                )
    return notes


def solve_ramdocs_cowiki_arbiter_v4(
    question: str,
    documents: Sequence[str],
    client: OpenAICompatibleClient,
    model: str,
) -> Dict[str, Any]:
    """v4: Best-of-both-worlds synthesis of v2 (high recall) and v3 (high precision fork/conflict pruning).

    - Stage 1: Identical to v2's zero-paranoia high-recall excerpt-aware extractor
      (preserves all valid 1-doc secondary disambiguated entities; hits SQLite cache when v2 ran).
    - CPU Gate: Strict `ratio >= 0.55` fork-diff audit (zero Wikipedia-chrome false positives,
      zero noisy identity-collision alerts).
    - Stage 2: Default-KEEP union across genuinely distinct entities, WITH surgical pruning
      of (a) tampered near-duplicate forks exposed by `FORK DIFF`, and (b) exact same-entity
      conflicts (identical person/lifespan or same institution with contradictory attribute).
    """
    clean_indices = [i for i, d in enumerate(documents) if not is_corrupted_binary_doc(d)]
    if not clean_indices:
        clean_indices = list(range(len(documents)))
    clean_docs = [documents[i] for i in clean_indices]

    pre_infos = cluster_near_duplicate_docs(clean_docs)
    fork_diffs = compute_strict_fork_diffs(clean_docs, min_ratio=0.55)

    doc_lines: List[str] = []
    for info, text in zip(pre_infos, clean_docs):
        tags = [f"cluster=C{info.cluster_id}", f"indep_w={info.independent_weight}"]
        if info.is_encyclopedic_lead:
            tags.append("lead_bio")
        doc_lines.append(f"[Doc {info.doc_id + 1} | {', '.join(tags)}]\n{text}")
    doc_block = "\n\n".join(doc_lines)

    # Identical Stage 1 prompt to v2 (high recall, zero tamper-paranoia!)
    stage1_prompt = f"""You are compiling structured claims from retrieved document excerpts for an ambiguous multi-entity QA task.

Question: {question}

CRITICAL EXTRACTION RULES:
1. **Excerpts often omit the full title or use variants**: A passage from an encyclopedia article may refer to the subject as "the city", "the county", "the district", "he", "she", a surname, a nickname (e.g., "Mike" for "Michael"), a spelling variant/romanization, or an expansion of an acronym. DO NOT discard a passage just because the exact query string is not repeated verbatim!
2. **When is a document `noise` (`answers_question: false`)?** ONLY when it genuinely does NOT contain an answer to the question (e.g., navigation menus, unrelated biography/filmography/list of other people, or background without the requested fact).
3. **Distinguish DIFFERENT disambiguated entities**: Give each distinct real-world entity a clear distinguishing `canonical_entity` (e.g., "Springfield (city)" vs "Springfield (county)", "Michael Jordan (basketball)" vs "Michael Jordan (computer scientist)", "Work X (1987 film)" vs "Work X (1992 album)"). Give two passages about the SAME entity the SAME `canonical_entity`.
4. **Verbatim concise answer**: Extract `candidate_answer` as the SHORTEST exact phrase directly from the document text (typically 1-5 words). Do NOT paraphrase or rewrite into longer descriptions!
5. **One primary answer per document-entity**: If a passage mentions a historical census table or multiple dates, pick the single primary headline figure/answer for that entity.
6. **Do NOT filter out suspected misinformation yet** in Stage 1 — extract ALL document-supported claims faithfully, and put any doubt in `suspicion_note`.

Return strict JSON:
{{
  "claims": [
    {{
      "doc_id": 1,
      "answers_question": true,
      "canonical_entity": "Distinct disambiguated entity",
      "candidate_answer": "concise exact answer from text",
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
            {"role": "system", "content": "You are a high-recall knowledge compiler. Extract grounded claims faithfully as valid JSON."},
            {"role": "user", "content": stage1_prompt},
        ],
        temperature=0.0,
        max_tokens=1600,
        json_mode=True,
    )
    total_pt = r1.prompt_tokens
    total_ct = r1.completion_tokens
    llm_calls = 1

    compiled = _extract_json_object(r1.content)
    claims = compiled.get("claims", [])
    if not isinstance(claims, list):
        claims = []

    info_by_id = {info.doc_id + 1: info for info in pre_infos}
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
        entity_groups[ent_key].append(
            {
                "doc_id": did,
                "canonical_entity": ent_raw,
                "candidate_answer": ans,
                "evidence_quote": str(cl.get("evidence_quote") or "")[:220],
                "suspicion_note": str(cl.get("suspicion_note") or ""),
                "source_cluster": f"C{info.cluster_id}" if info else "C?",
                "independent_weight": info.independent_weight if info else 1.0,
                "is_lead_bio": info.is_encyclopedic_lead if info else False,
            }
        )

    if not entity_groups:
        fb = solve_ramdocs_standard_rag(question, clean_docs, client, model)
        return {
            "answers": fb["answers"],
            "prompt_tokens": total_pt + fb["prompt_tokens"],
            "completion_tokens": total_ct + fb["completion_tokens"],
            "llm_calls": llm_calls + fb["llm_calls"],
            "fallback_used": True,
        }

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
                    "has_lead_bio": False,
                    "quotes": [],
                    "suspicions": [],
                }
            slot = by_ans[an]
            slot["doc_ids"].append(c["doc_id"])
            slot["distinct_source_clusters"].add(c["source_cluster"])
            slot["has_lead_bio"] = slot["has_lead_bio"] or c["is_lead_bio"]
            if c["evidence_quote"]:
                slot["quotes"].append(c["evidence_quote"])
            if c["suspicion_note"]:
                slot["suspicions"].append(c["suspicion_note"])

        candidates_out = [
            {
                "answer": slot["answer"],
                "doc_ids": slot["doc_ids"],
                "independent_source_clusters": len(slot["distinct_source_clusters"]),
                "has_encyclopedic_lead": slot["has_lead_bio"],
                "quotes": slot["quotes"][:2],
                "suspicions": slot["suspicions"][:2],
            }
            for slot in by_ans.values()
        ]
        cluster_summary.append(
            {
                "entity": g_claims[0]["canonical_entity"],
                "has_intra_entity_conflict": len(candidates_out) > 1,
                "candidate_answers": candidates_out,
            }
        )

    fork_section = ""
    if fork_diffs:
        fork_section = (
            "\nHIGH-CONFIDENCE NEAR-DUPLICATE FORK DIFFS (two versions of the SAME passage — ONE is authentic, ONE is a tampered fork!):\n"
            + "\n".join(fork_diffs)
            + "\n"
        )

    stage2_prompt = f"""You are a Grounded Multi-Entity Disambiguation & Surgical Misinformation Arbiter.

Question: {question}
{fork_section}
Compiled Entity-Level Candidates:
{json.dumps(cluster_summary, indent=2, ensure_ascii=False)}

DECISION RULES (BALANCE HIGH RECALL ACROSS ENTITIES WITH SURGICAL MISINFO REMOVAL):
1. **STRICT DOCUMENT GROUNDING**: Every string in `final_answers` MUST be copied concisely from one of the `answer` strings above. NEVER invent or substitute numbers/names from outside memory!
2. **DEFAULT KEEP (UNION) ACROSS GENUINELY DISTINCT ENTITIES**:
   - Most questions refer to 2 or 3 DIFFERENT real-world entities sharing an ambiguous name (e.g., a city vs a county/district; two different people with different birth/death dates or biographies; two different works from different years/media; two expansions of an acronym).
   - **A valid secondary entity frequently has ONLY 1 supporting document!** NEVER drop an unrelated valid entity just because another entity has more documents or because a different pair of docs had a fork diff!
3. **SURGICAL RULE A — FORK DIFF PRUNING**: If `HIGH-CONFIDENCE NEAR-DUPLICATE FORK DIFFS` lists `Doc A` vs `Doc B`, they are two versions of the **EXACT SAME entity** (NOT two entities!). Compare the diff against the shared context and factual truth:
   - An extra sentence inserted/appended into a verbatim copy of another doc with a bizarre/contradictory claim is ALWAYS the tampered fork -> DROP its claim, KEEP the clean original!
   - A swapped location/date/number that contradicts the shared title or surrounding geography/facts is ALWAYS the tampered fork -> DROP it, KEEP the consistent original!
4. **SURGICAL RULE B — EXACT SAME-ENTITY CONFLICT**: If two candidate answers refer to the **EXACT SAME person/institution/event** (for example: identical full name + identical birth/death dates or lifespan, or the same sports cup/organization/building) with conflicting facts (e.g., two different sports, two different military branches/ranks, two different founding/birth years, or an authentic specific census number vs a suspicious round/fake number):
   - They CANNOT both be true -> KEEP ONLY the ONE factually authentic answer (verify against well-established historical/biographical facts, `independent_source_clusters`, and internal coherence) and DROP the misinformation!
5. **CONCISE VERBATIM PHRASING**: Keep answers concise (1-5 words copied from the document candidate). Never output an empty list if valid grounded candidates exist.

Return strict JSON:
{{
  "reasoning": "brief entity-by-entity check (keep all distinct authentic entities; surgically prune only tampered forks and same-entity conflicts)",
  "final_answers": ["ans1", "ans2"]
}}"""

    r2 = client.chat(
        model=model,
        messages=[
            {"role": "system", "content": "You are a surgical epistemic arbiter. Output valid JSON only."},
            {"role": "user", "content": stage2_prompt},
        ],
        temperature=0.0,
        max_tokens=800,
        json_mode=True,
    )
    total_pt += r2.prompt_tokens
    total_ct += r2.completion_tokens
    llm_calls += 1

    final_answers = _extract_answer_list_from_text(r2.content)
    if not final_answers and cluster_summary:
        for cl in cluster_summary:
            best = max(cl["candidate_answers"], key=lambda x: (x["independent_source_clusters"], x["has_encyclopedic_lead"]))
            final_answers.append(best["answer"])

    return {
        "answers": final_answers,
        "prompt_tokens": total_pt,
        "completion_tokens": total_ct,
        "llm_calls": llm_calls,
        "fork_diffs": fork_diffs,
        "compiled_clusters": cluster_summary,
        "arbiter_raw": r2.content,
    }


# ============================================================================
# Solver 6: CoWiki Astute Fork-Diff Arbiter (v5)
# ============================================================================

def solve_ramdocs_cowiki_arbiter_v5(
    question: str,
    documents: Sequence[str],
    client: OpenAICompatibleClient,
    model: str,
) -> Dict[str, Any]:
    """v5: Combines v4's high-recall compilation & strict fork-diff with three literature-backed upgrades:

    1. **Answer-Type Canonicalization & Semantic Deduplication** (matches exact question type:
       sport name vs athlete role, full acronym expansion vs circular echo, single primary
       census figure per distinct municipality, merging synonymous strings for one entity).
    2. **Astute-RAG Adaptive Internal-External Elicitation** (Wang et al., ICLR 2025):
       When fork diffs or multi-candidate ambiguity/conflicts exist, elicits calibrated
       high-confidence internal encyclopedic knowledge (explicitly allowing 'UNCERTAIN' so
       obscure real entities are never suppressed) and consolidates internal + external evidence.
    3. **Clean-Original Recovery on Fork Diffs**: Exposes both fork openings & exact word diffs
       so an injected sentence in Doc B can never hide the authentic original answer in Doc A.
    """
    clean_indices = [i for i, d in enumerate(documents) if not is_corrupted_binary_doc(d)]
    if not clean_indices:
        clean_indices = list(range(len(documents)))
    clean_docs = [documents[i] for i in clean_indices]

    pre_infos = cluster_near_duplicate_docs(clean_docs)
    fork_diffs = compute_strict_fork_diffs(clean_docs, min_ratio=0.55)

    doc_lines: List[str] = []
    for info, text in zip(pre_infos, clean_docs):
        tags = [f"cluster=C{info.cluster_id}", f"indep_w={info.independent_weight}"]
        if info.is_encyclopedic_lead:
            tags.append("lead_bio")
        doc_lines.append(f"[Doc {info.doc_id + 1} | {', '.join(tags)}]\n{text}")
    doc_block = "\n\n".join(doc_lines)

    stage1_prompt = f"""You are compiling structured factual claims from retrieved document excerpts for an ambiguous multi-entity QA task.

Question: {question}

EXTRACTION RULES:
1. **Excerpts may omit the full title or use variants**: A passage may refer to the subject as "the city", "the county", "the district", "he", "she", a surname, nickname, romanization/spelling variant, or full expansion of an acronym. Do NOT discard a passage merely because the exact query string is not repeated verbatim.
2. **Noise (`answers_question: false`)**: Mark false ONLY when the passage genuinely lacks the requested fact (navigation bars, unrelated biographies/lists, or off-topic background).
3. **Distinct Entity Labeling**: Give each genuinely distinct real-world entity a clear distinguishing `canonical_entity` (e.g., "Springfield (city)" vs "Springfield (county)", "Person A (born 1924)" vs "Person A (born 1948)", "Work X (1987 film)" vs "Work X (1992 album)"). Give passages about the SAME entity the SAME `canonical_entity`.
4. **Match the Exact Requested Answer Type (CRITICAL!)**:
   - If asked *"What sport..."*, extract the **sport name** (e.g. "Ice hockey", "American football", "Middle-distance running"), NOT the player's role/occupation ("forward", "skier", "weightlifter").
   - If asked *"What does [ACRONYM] refer to?"*, extract the **full expanded name/referent**, NEVER echo the acronym itself.
   - If asked *"What is X known for / primary function?"*, extract the **concise core category** stated in the opening definition (e.g. "A film", "A convention", "A language"), NOT a tangential detail.
   - Keep `candidate_answer` concise (typically 1-5 words directly grounded in the text).
5. **Single Primary Figure per Document-Entity**: If a passage contains a historical census table or multiple historical years for one place, extract ONLY the single primary headline figure for that place (not every historical census year).
6. **Faithful High-Recall Extraction**: Extract all document-supported claims first; put any suspicion in `suspicion_note`.

Return strict JSON:
{{
  "claims": [
    {{
      "doc_id": 1,
      "answers_question": true,
      "canonical_entity": "Distinct disambiguated entity",
      "candidate_answer": "concise exact answer matching question type",
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
            {"role": "system", "content": "You are a high-recall, type-aligned knowledge compiler. Output valid JSON only."},
            {"role": "user", "content": stage1_prompt},
        ],
        temperature=0.0,
        max_tokens=1600,
        json_mode=True,
    )
    total_pt = r1.prompt_tokens
    total_ct = r1.completion_tokens
    llm_calls = 1

    compiled = _extract_json_object(r1.content)
    claims = compiled.get("claims", [])
    if not isinstance(claims, list):
        claims = []

    info_by_id = {info.doc_id + 1: info for info in pre_infos}
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
        entity_groups[ent_key].append(
            {
                "doc_id": did,
                "canonical_entity": ent_raw,
                "candidate_answer": ans,
                "evidence_quote": str(cl.get("evidence_quote") or "")[:220],
                "suspicion_note": str(cl.get("suspicion_note") or ""),
                "source_cluster": f"C{info.cluster_id}" if info else "C?",
                "independent_weight": info.independent_weight if info else 1.0,
                "is_lead_bio": info.is_encyclopedic_lead if info else False,
            }
        )

    if not entity_groups:
        fb = solve_ramdocs_standard_rag(question, clean_docs, client, model)
        return {
            "answers": fb["answers"],
            "prompt_tokens": total_pt + fb["prompt_tokens"],
            "completion_tokens": total_ct + fb["completion_tokens"],
            "llm_calls": llm_calls + fb["llm_calls"],
            "fallback_used": True,
        }

    cluster_summary: List[Dict[str, Any]] = []
    unique_answers: Set[str] = set()
    has_any_conflict = bool(fork_diffs)
    for ent_key, g_claims in entity_groups.items():
        by_ans: Dict[str, Dict[str, Any]] = {}
        for c in g_claims:
            an = normalize_answer(c["candidate_answer"])
            unique_answers.add(an)
            if an not in by_ans:
                by_ans[an] = {
                    "answer": c["candidate_answer"],
                    "doc_ids": [],
                    "distinct_source_clusters": set(),
                    "has_lead_bio": False,
                    "quotes": [],
                    "suspicions": [],
                }
            slot = by_ans[an]
            slot["doc_ids"].append(c["doc_id"])
            slot["distinct_source_clusters"].add(c["source_cluster"])
            slot["has_lead_bio"] = slot["has_lead_bio"] or c["is_lead_bio"]
            if c["evidence_quote"]:
                slot["quotes"].append(c["evidence_quote"])
            if c["suspicion_note"]:
                slot["suspicions"].append(c["suspicion_note"])

        if len(by_ans) > 1:
            has_any_conflict = True
        candidates_out = [
            {
                "answer": slot["answer"],
                "doc_ids": slot["doc_ids"],
                "independent_source_clusters": len(slot["distinct_source_clusters"]),
                "has_encyclopedic_lead": slot["has_lead_bio"],
                "quotes": slot["quotes"][:2],
                "suspicions": slot["suspicions"][:2],
            }
            for slot in by_ans.values()
        ]
        cluster_summary.append(
            {
                "entity": g_claims[0]["canonical_entity"],
                "has_intra_entity_conflict": len(candidates_out) > 1,
                "candidate_answers": candidates_out,
            }
        )

    # Adaptive Astute-RAG Internal Knowledge Elicitation (ONLY when fork_diffs or >=2 unique answers!)
    internal_anchor_block = ""
    if has_any_conflict or len(unique_answers) >= 2:
        cand_list_str = ", ".join(
            f"{cl['entity']}: {[c['answer'] for c in cl['candidate_answers']]}"
            for cl in cluster_summary
        )
        elicit_prompt = (
            "Briefly state ONLY verified, high-confidence encyclopedic facts you reliably know from memory "
            "about the entities and candidate claims below. "
            "CRITICAL: For obscure towns, minor people, or anything you are NOT 100% sure of, explicitly write 'UNCERTAIN' "
            "— NEVER guess! Only flag a claim as 'CONFIRMED_FALSE' if you are certain it contradicts established reality "
            "(e.g., wrong sport/rank/location/creator for a known entity, or an implausible fabricated claim).\n\n"
            f"Question: {question}\n"
            f"Candidate claims to sanity-check: {cand_list_str}\n\n"
            "Return strict JSON: {\"knowledge_checks\": [{\"entity\": \"...\", \"verdict\": \"CONFIRMED_TRUE|CONFIRMED_FALSE|UNCERTAIN\", \"note\": \"...\"}]}"
        )
        r_int = client.chat(
            model=model,
            messages=[
                {"role": "system", "content": "You are a calibrated encyclopedic fact-checker. Say UNCERTAIN whenever not 100% sure. Output valid JSON only."},
                {"role": "user", "content": elicit_prompt},
            ],
            temperature=0.0,
            max_tokens=450,
            json_mode=True,
        )
        total_pt += r_int.prompt_tokens
        total_ct += r_int.completion_tokens
        llm_calls += 1
        internal_anchor_block = f"\nCALIBRATED INTERNAL KNOWLEDGE CHECK (trust 'CONFIRMED_FALSE' only; whenever 'UNCERTAIN', defer to the grounded documents!):\n{r_int.content}\n"

    fork_section = ""
    if fork_diffs:
        fork_section = (
            "\nHIGH-CONFIDENCE NEAR-DUPLICATE FORK DIFFS (two versions of the SAME passage — ONE is authentic original, ONE is a tampered fork!):\n"
            + "\n".join(fork_diffs)
            + "\n"
        )

    stage2_prompt = f"""You are a Grounded Multi-Entity Disambiguation & Astute Conflict Arbiter.

Question: {question}
{fork_section}{internal_anchor_block}
Compiled External Document Candidates:
{json.dumps(cluster_summary, indent=2, ensure_ascii=False)}

CONSOLIDATION & ARBITRATION RULES:
1. **STRICT EXTERNAL DOCUMENT GROUNDING**: Every string in `final_answers` MUST be directly grounded in the external documents above (or the clean original passage opening shown in `FORK DIFF`). NEVER substitute numbers/names from memory that do not appear in the documents!
2. **AMBIGUITY = DEFAULT KEEP (UNION) ACROSS GENUINELY DISTINCT ENTITIES**:
   - Most queries genuinely refer to 2 or 3 DIFFERENT real-world entities sharing an ambiguous name (e.g., city vs county/district; two people with different birth/death dates or biographies; two works in different years/media; two acronym expansions).
   - **A valid secondary entity frequently has ONLY 1 supporting document!** Whenever the internal knowledge check says `UNCERTAIN` for an authentic-looking encyclopedic passage, **KEEP IT**!
3. **SURGICAL FORK-DIFF PRUNING**: When `FORK DIFF` pairs `Doc A` and `Doc B`, they are ALWAYS two versions of the **SAME entity**:
   - If one version inserts/appends a jarring contradictory claim into a verbatim copy of the other, that inserted claim is ALWAYS tampered misinformation -> **DROP it** and **KEEP the authentic original doc's concise answer**!
   - If one version swaps a location/date/number/role contradicting the shared title or surrounding context, **DROP the tampered swap** and **KEEP the internally coherent original**!
4. **SAME-ENTITY CONFLICT & INTERNAL-EXTERNAL CONSOLIDATION**:
   - If two candidate answers refer to the **EXACT SAME person/place/institution** (same lifespan/birth-death dates, same founding year, same organization, or two different census years of the SAME municipality), keep **ONLY ONE** true primary answer!
   - Drop any claim marked `CONFIRMED_FALSE` by high-confidence internal knowledge or clearly contradicted by `independent_source_clusters` / context coherence.
5. **SEMANTIC DEDUPLICATION & EXACT QUESTION-TYPE FORMAT**:
   - NEVER output two synonymous strings for the same entity (e.g., do NOT output both "American football" and "Football" for one player — keep the single best concise phrase!).
   - Match the exact requested answer type (sport name for *"What sport..."*, full expansion for *"What does [ACRONYM] refer to?"*, concise 1-5 word phrase).
   - Never return an empty list if valid authentic candidates exist.

Return strict JSON:
{{
  "reasoning": "concise consolidation per entity",
  "final_answers": ["ans1", "ans2"]
}}"""

    r2 = client.chat(
        model=model,
        messages=[
            {"role": "system", "content": "You are an astute grounded conflict arbiter. Output valid JSON only."},
            {"role": "user", "content": stage2_prompt},
        ],
        temperature=0.0,
        max_tokens=800,
        json_mode=True,
    )
    total_pt += r2.prompt_tokens
    total_ct += r2.completion_tokens
    llm_calls += 1

    final_answers = _extract_answer_list_from_text(r2.content)
    if not final_answers and cluster_summary:
        for cl in cluster_summary:
            best = max(cl["candidate_answers"], key=lambda x: (x["independent_source_clusters"], x["has_encyclopedic_lead"]))
            final_answers.append(best["answer"])

    return {
        "answers": final_answers,
        "prompt_tokens": total_pt,
        "completion_tokens": total_ct,
        "llm_calls": llm_calls,
        "fork_diffs": fork_diffs,
        "compiled_clusters": cluster_summary,
        "arbiter_raw": r2.content,
    }
