"""Fair <=256-token Hybrid Graph+Slot Retrieval Engine for MultiHop-RAG.

Key design invariants:
- Every indexed chunk has strictly <=256 estimated body tokens (matching official
  `yixuantt/MultiHop-RAG` SentenceSplitter(chunk_size=256, chunk_overlap_ratio=0.1)).
- Returned `text` for evaluation contains ONLY the clean <=256-token body chunk
  (zero chunk inflation).
- Structured OKF metadata (source, published_at ISO + natural date, title, author,
  category) is indexed alongside the body in SQLite FTS5 and contextual embedding text.
- Passage-Entity Bipartite Graph + HippoRAG 2-style Personalized PageRank (PPR)
  runs in pure CPU Python stdlib in milliseconds per query.
"""

from __future__ import annotations

import math
import re
import sqlite3
import time
from collections import Counter, defaultdict
from dataclasses import dataclass
from datetime import datetime
from typing import Any, Dict, List, Optional, Sequence, Set, Tuple

from .llm_client import OpenAICompatibleClient
from .metrics import estimate_tokens


SENTENCE_SPLIT_RE = re.compile(r"(?<=[.!?])\s+(?=[A-Z0-9\"'\(\[])|\n+")
WORD_RE = re.compile(r"[A-Za-z0-9]+(?:['-][A-Za-z0-9]+)?")
ENTITY_RE = re.compile(
    r"\b(?:[A-Z][a-z0-9]+(?:\s+(?:of|the|for|and|in|on|at|to|&|de|van|von)?\s*[A-Z][a-z0-9]+){0,4}|[A-Z]{2,6}(?:-[A-Z0-9]{1,5})?)\b"
)
MONTH_NAMES = (
    "January", "February", "March", "April", "May", "June",
    "July", "August", "September", "October", "November", "December",
)
MONTH_ABBR = (
    "Jan", "Feb", "Mar", "Apr", "May", "Jun",
    "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
)
MONTH_MAP = {
    **{m.lower(): i + 1 for i, m in enumerate(MONTH_NAMES)},
    **{m.lower(): i + 1 for i, m in enumerate(MONTH_ABBR)},
    "sept": 9,
}

DATE_PATTERN_1 = re.compile(
    r"\b(January|February|March|April|May|June|July|August|September|October|November|December|"
    r"Jan|Feb|Mar|Apr|Jun|Jul|Aug|Sep|Sept|Oct|Nov|Dec)\.?\s+(\d{1,2})(?:st|nd|rd|th)?,?\s+(20\d{2})\b",
    re.IGNORECASE,
)
DATE_PATTERN_2 = re.compile(
    r"\b(\d{1,2})(?:st|nd|rd|th)?\s+(January|February|March|April|May|June|July|August|September|October|November|December|"
    r"Jan|Feb|Mar|Apr|Jun|Jul|Aug|Sep|Sept|Oct|Nov|Dec)\.?,?\s+(20\d{2})\b",
    re.IGNORECASE,
)
DATE_PATTERN_ISO = re.compile(r"\b(20\d{2})-(\d{2})-(\d{2})\b")
TIME_PATTERN_HMS = re.compile(r"\b(\d{2}:\d{2}:\d{2})\b")

STOPWORDS = frozenset(
    {
        "the", "and", "for", "with", "that", "this", "from", "are", "was", "were", "have", "has",
        "had", "not", "but", "you", "your", "they", "their", "them", "his", "her", "she", "him",
        "its", "our", "out", "about", "into", "over", "after", "before", "between", "through",
        "during", "under", "again", "further", "then", "once", "here", "there", "when", "where",
        "why", "how", "all", "any", "both", "each", "few", "more", "most", "other", "some", "such",
        "only", "own", "same", "than", "too", "very", "can", "will", "just", "should", "now",
        "who", "what", "which", "whom", "whose", "does", "did", "been", "being", "also", "while",
        "article", "articles", "report", "reported", "reports", "reporting", "published", "story",
        "news", "regarding", "concerning", "between", "following", "subsequent", "prior", "based",
        "whether", "suggest", "suggests", "indicate", "indicates", "mention", "mentions", "mentioned",
        "associated", "individual", "company", "companies", "organization", "entity", "event", "events",
        "consistent", "change", "nature", "similar", "different", "compare", "compared", "comparison",
        "according", "first", "second", "third", "last", "new", "year", "years", "month", "week", "day",
        "monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday",
        "january", "february", "march", "april", "may", "june", "july", "august", "september",
        "october", "november", "december",
    }
)

GENERIC_ENTITY_STOPS = frozenset(
    {
        "the", "this", "that", "these", "those", "there", "here", "when", "where", "while", "after",
        "before", "during", "since", "because", "however", "although", "though", "still", "even",
        "also", "just", "like", "more", "most", "some", "many", "much", "such", "other", "another",
        "first", "second", "third", "last", "next", "new", "one", "two", "three", "four", "five",
        "monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday",
        "january", "february", "march", "april", "may", "june", "july", "august", "september",
        "october", "november", "december", "but", "and", "for", "with", "from", "into", "over",
        "according", "meanwhile", "furthermore", "additionally", "ultimately", "specifically",
        "image", "credit", "photo", "read", "more", "table", "contents", "editor", "note",
        "who", "what", "which", "does", "did", "was", "were", "are", "has", "have", "considering",
    }
)


@dataclass
class ChunkRecord:
    chunk_id: int
    doc_id: int
    chunk_idx: int
    title: str
    source: str
    author: str
    category: str
    published_iso: str
    published_natural: str
    url: str
    body: str
    body_tokens: int
    meta_text: str
    contextual_text: str


def format_date_variants(iso_str: str) -> Tuple[str, str]:
    """Return `(YYYY-MM-DD, 'October 7, 2023 Oct 7 2023 13:41:30')`."""
    if not iso_str:
        return "", ""
    m = re.match(r"(\d{4})-(\d{2})-(\d{2})(?:[T\s](\d{2}:\d{2}:\d{2}))?", iso_str.strip())
    if not m:
        return iso_str.strip()[:10], iso_str.strip()
    y, mo, d = int(m.group(1)), int(m.group(2)), int(m.group(3))
    hms = m.group(4) or ""
    try:
        dt = datetime(y, mo, d)
    except ValueError:
        return f"{y:04d}-{mo:02d}-{d:02d}", hms
    iso_day = dt.strftime("%Y-%m-%d")
    full_month = MONTH_NAMES[mo - 1]
    short_month = MONTH_ABBR[mo - 1]
    natural = f"{full_month} {d}, {y} {short_month} {d} {y}"
    if hms:
        natural = f"{natural} {hms}"
    return iso_day, natural


def chunk_document_body(
    body: str,
    max_tokens: int = 256,
    overlap_tokens: int = 32,
) -> List[str]:
    """Sentence-aware chunker guaranteeing every chunk has <= `max_tokens` estimated tokens."""
    clean = re.sub(r"[ \t]+", " ", body.replace("\r\n", "\n")).strip()
    if not clean:
        return []

    raw_sents = [s.strip() for s in SENTENCE_SPLIT_RE.split(clean) if s.strip()]
    # Break any overlong sentence into word windows strictly under max_tokens
    units: List[Tuple[str, int]] = []
    for sent in raw_sents:
        toks = estimate_tokens(sent)
        if toks <= max_tokens:
            units.append((sent, toks))
        else:
            words = sent.split()
            cur: List[str] = []
            for w in words:
                cand = " ".join(cur + [w])
                if cur and estimate_tokens(cand) > max_tokens:
                    piece = " ".join(cur)
                    units.append((piece, estimate_tokens(piece)))
                    # small overlap
                    cur = cur[-12:] + [w]
                    while estimate_tokens(" ".join(cur)) > max_tokens and len(cur) > 1:
                        cur.pop(0)
                else:
                    cur.append(w)
            if cur:
                piece = " ".join(cur)
                units.append((piece, estimate_tokens(piece)))

    chunks: List[str] = []
    idx = 0
    n = len(units)
    while idx < n:
        buf: List[Tuple[str, int]] = []
        tok_sum = 0
        j = idx
        while j < n:
            u_text, u_tok = units[j]
            if buf and tok_sum + u_tok > max_tokens:
                break
            buf.append((u_text, u_tok))
            tok_sum += u_tok
            j += 1

        chunk_text = " ".join(x[0] for x in buf).strip()
        # Safety trim in case joining spaces slightly nudges token count
        if estimate_tokens(chunk_text) > max_tokens and len(buf) > 1:
            buf.pop()
            j -= 1
            chunk_text = " ".join(x[0] for x in buf).strip()

        if chunk_text:
            chunks.append(chunk_text)

        if j >= n:
            break

        # Rewind for sentence overlap (~overlap_tokens)
        back_toks = 0
        next_idx = j
        k = j - 1
        while k > idx and back_toks + units[k][1] <= overlap_tokens:
            back_toks += units[k][1]
            next_idx = k
            k -= 1
        idx = max(idx + 1, next_idx)

    return chunks


def extract_entities(text: str) -> List[str]:
    """Deterministic lightweight entity/phrase extractor for Passage-Entity Graph."""
    if not text:
        return []
    found: List[str] = []
    seen: Set[str] = set()
    lead_strip = frozenset(
        {"the", "of", "and", "for", "in", "on", "at", "to", "did", "does", "was", "were", "is", "are", "who", "what", "which", "after", "before", "when", "where", "while", "how", "considering", "between", "following"}
    )
    for match in ENTITY_RE.finditer(text):
        span = re.sub(r"\s+", " ", match.group(0).strip())
        low = span.lower()
        parts = low.split()
        while parts and parts[0] in lead_strip:
            parts.pop(0)
        while parts and parts[-1] in ("the", "of", "and", "for", "in", "on", "at", "to"):
            parts.pop()
        if not parts:
            continue
        norm = " ".join(parts)
        if len(norm) < 2 or norm in GENERIC_ENTITY_STOPS:
            continue
        if len(parts) == 1 and (len(norm) <= 2 and not span.isupper()):
            continue
        if norm not in seen:
            seen.add(norm)
            found.append(norm)
            if len(parts) >= 2:
                for p in parts:
                    if len(p) >= 5 and p not in GENERIC_ENTITY_STOPS and p not in STOPWORDS and p not in seen:
                        seen.add(p)
                        found.append(p)
    return found


def extract_dates_from_text(text: str) -> List[str]:
    """Extract normalized `YYYY-MM-DD` dates mentioned in a query."""
    found: List[str] = []
    seen: Set[str] = set()
    for m in DATE_PATTERN_1.finditer(text):
        mo = MONTH_MAP.get(m.group(1).lower())
        d = int(m.group(2))
        y = int(m.group(3))
        if mo and 1 <= d <= 31:
            iso = f"{y:04d}-{mo:02d}-{d:02d}"
            if iso not in seen:
                seen.add(iso)
                found.append(iso)
    for m in DATE_PATTERN_2.finditer(text):
        d = int(m.group(1))
        mo = MONTH_MAP.get(m.group(2).lower())
        y = int(m.group(3))
        if mo and 1 <= d <= 31:
            iso = f"{y:04d}-{mo:02d}-{d:02d}"
            if iso not in seen:
                seen.add(iso)
                found.append(iso)
    for m in DATE_PATTERN_ISO.finditer(text):
        iso = f"{m.group(1)}-{m.group(2)}-{m.group(3)}"
        if iso not in seen:
            seen.add(iso)
            found.append(iso)
    return found


def build_fts_or_query(text: str, max_terms: int = 28) -> str:
    """Build a safe SQLite FTS5 OR query from distinctive terms."""
    tokens = WORD_RE.findall(text)
    cleaned: List[str] = []
    seen: Set[str] = set()
    for tok in tokens:
        low = tok.lower()
        if len(low) < 2 or low in STOPWORDS:
            continue
        # Remove apostrophes/hyphens for safe FTS5 tokenizing
        safe = re.sub(r"[^a-z0-9]", "", low)
        if len(safe) < 2 or safe in STOPWORDS or safe in seen:
            continue
        seen.add(safe)
        cleaned.append(f'"{safe}"')
        if len(cleaned) >= max_terms:
            break
    return " OR ".join(cleaned)


def cosine_sim(a: Sequence[float], b: Sequence[float]) -> float:
    dot = 0.0
    na = 0.0
    nb = 0.0
    for x, y in zip(a, b):
        dot += x * y
        na += x * x
        nb += y * y
    if na <= 0.0 or nb <= 0.0:
        return 0.0
    return dot / math.sqrt(na * nb)


class MultiHopHybridIndex:
    """In-memory SQLite FTS5 + Passage-Entity Graph PPR + Pluggable Dense & Rerank Index."""

    def __init__(
        self,
        corpus: Sequence[Dict[str, Any]],
        max_chunk_tokens: int = 256,
        overlap_tokens: int = 32,
    ):
        t0 = time.perf_counter()
        self.max_chunk_tokens = max_chunk_tokens
        self.overlap_tokens = overlap_tokens
        self.chunks: List[ChunkRecord] = []
        self.doc_to_chunks: Dict[int, List[int]] = defaultdict(list)
        self.known_sources: List[str] = []
        self.source_to_chunks: Dict[str, Set[int]] = defaultdict(set)
        self.date_to_chunks: Dict[str, Set[int]] = defaultdict(set)

        # Graph structures (Passage-Entity Bipartite + Intra-Article Adjacency)
        self.chunk_entities: List[List[Tuple[str, float]]] = []
        self.entity_to_chunks: Dict[str, List[Tuple[int, float]]] = defaultdict(list)
        self.entity_idf: Dict[str, float] = {}
        self.chunk_vectors: Optional[List[List[float]]] = None

        source_set: Set[str] = set()
        chunk_id = 0
        for doc_id, doc in enumerate(corpus):
            title = str(doc.get("title") or "").strip()
            source = str(doc.get("source") or "").strip()
            author = str(doc.get("author") or "").strip()
            category = str(doc.get("category") or "").strip()
            url = str(doc.get("url") or "").strip()
            iso_day, nat_date = format_date_variants(str(doc.get("published_at") or ""))
            if source:
                source_set.add(source)

            meta_text = f"Title: {title} | Source: {source} | Date: {iso_day} {nat_date} | Author: {author} | Category: {category}"
            pieces = chunk_document_body(
                str(doc.get("body") or ""),
                max_tokens=max_chunk_tokens,
                overlap_tokens=overlap_tokens,
            )
            for c_idx, piece in enumerate(pieces):
                toks = estimate_tokens(piece)
                contextual = f"[{source} | {iso_day} | {title}]\n{piece}"
                rec = ChunkRecord(
                    chunk_id=chunk_id,
                    doc_id=doc_id,
                    chunk_idx=c_idx,
                    title=title,
                    source=source,
                    author=author,
                    category=category,
                    published_iso=iso_day,
                    published_natural=nat_date,
                    url=url,
                    body=piece,
                    body_tokens=toks,
                    meta_text=meta_text,
                    contextual_text=contextual,
                )
                self.chunks.append(rec)
                self.doc_to_chunks[doc_id].append(chunk_id)
                if source:
                    self.source_to_chunks[source.lower()].add(chunk_id)
                if iso_day:
                    self.date_to_chunks[iso_day].add(chunk_id)
                chunk_id += 1

        # Sort known sources longest-first for unambiguous substring matching in queries
        self.known_sources = sorted(source_set, key=len, reverse=True)

        # Build SQLite FTS5 tables (raw body AND metadata-augmented)
        self._conn = sqlite3.connect(":memory:")
        self._conn.execute(
            "CREATE VIRTUAL TABLE chunks_raw_fts USING fts5(chunk_id UNINDEXED, body, tokenize='unicode61 remove_diacritics 2')"
        )
        self._conn.execute(
            "CREATE VIRTUAL TABLE chunks_meta_fts USING fts5(chunk_id UNINDEXED, meta, body, tokenize='unicode61 remove_diacritics 2')"
        )
        self._conn.executemany(
            "INSERT INTO chunks_raw_fts (chunk_id, body) VALUES (?, ?)",
            [(c.chunk_id, c.body) for c in self.chunks],
        )
        self._conn.executemany(
            "INSERT INTO chunks_meta_fts (chunk_id, meta, body) VALUES (?, ?, ?)",
            [(c.chunk_id, c.meta_text, c.body) for c in self.chunks],
        )
        self._conn.commit()

        # Build Passage-Entity Bipartite Graph
        self._build_entity_graph()
        self.build_ms = (time.perf_counter() - t0) * 1000.0

    def _build_entity_graph(self) -> None:
        n_chunks = len(self.chunks)
        raw_chunk_ents: List[List[str]] = []
        ent_df: Counter = Counter()

        for c in self.chunks:
            # Extract entities from body + title (NOT source name, which is too broad a hub)
            ents = extract_entities(f"{c.title}. {c.body}")
            raw_chunk_ents.append(ents)
            for e in ents:
                ent_df[e] += 1

        max_df = max(5, int(n_chunks * 0.12))
        for e, df in ent_df.items():
            if 2 <= df <= max_df:
                self.entity_idf[e] = math.log((n_chunks + 1.0) / (df + 1.0)) + 1.0

        self.chunk_entities = [[] for _ in range(n_chunks)]
        for cid, ents in enumerate(raw_chunk_ents):
            weighted: List[Tuple[str, float]] = []
            for e in ents:
                idf = self.entity_idf.get(e)
                if idf is not None:
                    weighted.append((e, idf))
                    self.entity_to_chunks[e].append((cid, idf))
            # Normalize outgoing weights from chunk -> entities
            tot = sum(w for _, w in weighted)
            if tot > 0:
                self.chunk_entities[cid] = [(e, w / tot) for e, w in weighted]

        # Normalize outgoing weights from entity -> chunks
        for e, postings in list(self.entity_to_chunks.items()):
            tot = sum(w for _, w in postings)
            if tot > 0:
                self.entity_to_chunks[e] = [(cid, w / tot) for cid, w in postings]

    def build_dense_index(
        self,
        client: OpenAICompatibleClient,
        embed_model: str,
        dimensions: Optional[int] = None,
        batch_size: int = 10,
    ) -> float:
        t0 = time.perf_counter()
        texts = [c.contextual_text for c in self.chunks]
        self.chunk_vectors = client.embed_batch(
            model=embed_model,
            texts=texts,
            dimensions=dimensions,
            batch_size=batch_size,
        )
        return (time.perf_counter() - t0) * 1000.0

    # ------------------------------------------------------------------
    # Retrieval primitives
    # ------------------------------------------------------------------

    def search_fts_raw(self, query: str, top_k: int = 40) -> List[Tuple[int, float]]:
        fts_q = build_fts_or_query(query)
        if not fts_q:
            return []
        rows = self._conn.execute(
            "SELECT chunk_id, bm25(chunks_raw_fts) AS score FROM chunks_raw_fts WHERE chunks_raw_fts MATCH ? ORDER BY score LIMIT ?",
            (fts_q, top_k),
        ).fetchall()
        return [(int(r[0]), -float(r[1])) for r in rows]

    def search_fts_meta(self, query: str, top_k: int = 40) -> List[Tuple[int, float]]:
        fts_q = build_fts_or_query(query)
        if not fts_q:
            return []
        # Weight meta column slightly higher than raw body so source+date+title act as strong anchors
        rows = self._conn.execute(
            "SELECT chunk_id, bm25(chunks_meta_fts, 1.35, 1.0) AS score FROM chunks_meta_fts WHERE chunks_meta_fts MATCH ? ORDER BY score LIMIT ?",
            (fts_q, top_k),
        ).fetchall()
        return [(int(r[0]), -float(r[1])) for r in rows]

    def search_dense(
        self,
        query_vec: Sequence[float],
        top_k: int = 40,
    ) -> List[Tuple[int, float]]:
        if not self.chunk_vectors or not query_vec:
            return []
        scored = [
            (cid, cosine_sim(query_vec, vec))
            for cid, vec in enumerate(self.chunk_vectors)
        ]
        scored.sort(key=lambda x: x[1], reverse=True)
        return scored[:top_k]

    def run_passage_entity_ppr(
        self,
        seed_weights: Dict[int, float],
        query_entities: Sequence[str],
        alpha: float = 0.55,
        iterations: int = 4,
        top_k: int = 40,
        frontier_cap: int = 250,
    ) -> List[Tuple[int, float]]:
        """HippoRAG 2-style Personalized PageRank over the Passage-Entity Bipartite Graph.

        Prunes tiny frontier probabilities each iteration so 4-hop propagation
        completes in ~10-15 ms on CPU with virtually identical top-40 rankings.
        """
        if not seed_weights:
            return []

        total_seed = sum(max(0.0, w) for w in seed_weights.values())
        if total_seed <= 0:
            return []
        p_chunk: Dict[int, float] = {cid: w / total_seed for cid, w in seed_weights.items() if w > 0}

        q_ent_set = {e for e in query_entities if e in self.entity_to_chunks}
        p_ent: Dict[str, float] = {}
        if q_ent_set:
            ent_tot = sum(self.entity_idf.get(e, 1.0) for e in q_ent_set)
            if ent_tot > 0:
                p_ent = {e: self.entity_idf.get(e, 1.0) / ent_tot for e in q_ent_set}

        r_chunk: Dict[int, float] = dict(p_chunk)

        for _ in range(iterations):
            # Step 1: Active Chunks -> Entities
            next_ent: Dict[str, float] = defaultdict(float)
            for e, pw in p_ent.items():
                next_ent[e] += alpha * pw
            for cid, mass in r_chunk.items():
                if mass < 1e-4:
                    continue
                edges = self.chunk_entities[cid]
                if not edges:
                    continue
                spread = (1.0 - alpha) * mass
                for e, w in edges:
                    next_ent[e] += spread * w

            # Keep top frontier entities for fast sparse propagation
            if len(next_ent) > frontier_cap:
                active_ents = sorted(next_ent.items(), key=lambda x: x[1], reverse=True)[:frontier_cap]
            else:
                active_ents = list(next_ent.items())

            # Step 2: Entities -> Chunks + intra-article adjacent chunk continuity
            next_chunk: Dict[int, float] = defaultdict(float)
            for cid, pw in p_chunk.items():
                next_chunk[cid] += alpha * pw

            for e, mass in active_ents:
                if mass < 1e-4:
                    continue
                postings = self.entity_to_chunks.get(e)
                if not postings:
                    continue
                spread = (1.0 - alpha) * 0.80 * mass
                for cid, w in postings:
                    next_chunk[cid] += spread * w

            for cid, mass in r_chunk.items():
                if mass < 1e-4:
                    continue
                c = self.chunks[cid]
                doc_chunks = self.doc_to_chunks[c.doc_id]
                pos = c.chunk_idx
                neighbors: List[int] = []
                if pos > 0:
                    neighbors.append(doc_chunks[pos - 1])
                if pos + 1 < len(doc_chunks):
                    neighbors.append(doc_chunks[pos + 1])
                if neighbors:
                    share = ((1.0 - alpha) * 0.20 * mass) / len(neighbors)
                    for nb in neighbors:
                        next_chunk[nb] += share

            if len(next_chunk) > frontier_cap:
                r_chunk = dict(sorted(next_chunk.items(), key=lambda x: x[1], reverse=True)[:frontier_cap])
            else:
                r_chunk = dict(next_chunk)

        ranked = sorted(r_chunk.items(), key=lambda x: x[1], reverse=True)
        return ranked[:top_k]

    # ------------------------------------------------------------------
    # Multi-Slot Query Decomposition (Deterministic Source/Date/Clause)
    # ------------------------------------------------------------------

    def decompose_query_slots(self, query: str) -> List[Dict[str, Any]]:
        """Decompose a multi-hop query into targeted sub-slots using corpus source vocabulary, dates, times, and clauses."""
        q_low = query.lower()

        source_hits: List[Tuple[int, int, str]] = []
        used_spans: List[Tuple[int, int]] = []
        for src in self.known_sources:
            src_low = src.lower()
            if len(src_low) < 3:
                continue
            for m in re.finditer(rf"(?<!\w){re.escape(src_low)}(?!\w)", q_low):
                s0, s1 = m.start(), m.end()
                if any(not (s1 <= u0 or s0 >= u1) for u0, u1 in used_spans):
                    continue
                used_spans.append((s0, s1))
                source_hits.append((s0, s1, src))
        source_hits.sort(key=lambda x: x[0])

        date_hits: List[Tuple[int, int, str]] = []
        for pat, fn in (
            (DATE_PATTERN_1, lambda m: (int(m.group(3)), MONTH_MAP[m.group(1).lower()], int(m.group(2)))),
            (DATE_PATTERN_2, lambda m: (int(m.group(3)), MONTH_MAP[m.group(2).lower()], int(m.group(1)))),
            (DATE_PATTERN_ISO, lambda m: (int(m.group(1)), int(m.group(2)), int(m.group(3)))),
        ):
            for m in pat.finditer(query):
                y, mo, d = fn(m)
                date_hits.append((m.start(), m.end(), f"{y:04d}-{mo:02d}-{d:02d}"))
        date_hits.sort(key=lambda x: x[0])

        time_hits: List[Tuple[int, int, str]] = [(m.start(), m.end(), m.group(1)) for m in TIME_PATTERN_HMS.finditer(query)]

        slots: List[Dict[str, Any]] = []

        # Case A: Multiple explicit source mentions
        if len(source_hits) >= 2:
            # Determine if sources are clustered tightly together (e.g. "both The Verge and TechCrunch")
            tight_group = (source_hits[-1][0] - source_hits[0][1]) < 45
            shared_ents = " ".join(extract_entities(query)[:6])
            for i, (s0, s1, src) in enumerate(source_hits):
                if tight_group:
                    # All sources share the full query topic; constrain each slot to its source
                    slot_date = date_hits[i][2] if i < len(date_hits) else (date_hits[0][2] if len(date_hits) == 1 else None)
                    slot_time = time_hits[i][2] if i < len(time_hits) else None
                    slot_query = f"{src} {slot_date or ''} {slot_time or ''} {query}".strip()
                else:
                    # Clean non-overlapping span starting right at this source mention until next source mention
                    left = s0 if i > 0 else 0
                    right = source_hits[i + 1][0] if i + 1 < len(source_hits) else len(query)
                    local_clause = query[left:right].strip()
                    # If the very first source appears late (after a shared intro), include intro context too
                    prefix_ctx = query[: source_hits[0][0]].strip() if (i > 0 and source_hits[0][0] > 35) else ""
                    slot_date = None
                    for d0, _d1, iso in date_hits:
                        if left <= d0 <= right:
                            slot_date = iso
                            break
                    slot_time = None
                    for t0, _t1, hms in time_hits:
                        if left <= t0 <= right:
                            slot_time = hms
                            break
                    slot_query = f"{src} {slot_date or ''} {slot_time or ''} {prefix_ctx} {local_clause} {shared_ents}".strip()
                slots.append({"query": slot_query, "source": src.lower(), "date": slot_date, "time": slot_time})
            return slots

        # Case B: Multiple explicit dates or HH:MM:SS timestamps
        if len(date_hits) >= 2 or len(time_hits) >= 2:
            src_only = source_hits[0][2].lower() if len(source_hits) == 1 else None
            anchors = date_hits if len(date_hits) >= 2 else time_hits
            is_date = len(date_hits) >= 2
            shared_ents = " ".join(extract_entities(query)[:6])
            for i, (a0, a1, val) in enumerate(anchors):
                left = anchors[i - 1][1] if i > 0 else 0
                right = anchors[i + 1][0] if i + 1 < len(anchors) else len(query)
                local_clause = query[left:right].strip()
                slot_query = f"{src_only or ''} {val} {local_clause} {shared_ents}".strip()
                slots.append(
                    {
                        "query": slot_query,
                        "source": src_only,
                        "date": val if is_date else None,
                        "time": val if not is_date else None,
                    }
                )
            return slots

        # Case C: Clause-based split on comparison/bridge keywords
        src_only = source_hits[0][2].lower() if len(source_hits) == 1 else None
        date_only = date_hits[0][2] if len(date_hits) == 1 else None
        time_only = time_hits[0][2] if len(time_hits) == 1 else None
        clauses = re.split(r"\b(?:and the|while the|whereas|compared to|as well as|versus|vs\.)\b", query, flags=re.IGNORECASE)
        if len(clauses) >= 2:
            shared_ents = " ".join(extract_entities(query)[:4])
            for cl in clauses[:3]:
                cl = cl.strip()
                if len(cl.split()) >= 4:
                    slots.append({"query": f"{cl} {shared_ents}".strip(), "source": src_only, "date": date_only, "time": time_only})
        elif src_only or date_only or time_only:
            slots.append({"query": query, "source": src_only, "date": date_only, "time": time_only})

        return slots

    # ------------------------------------------------------------------
    # Unified Retrieve API (supports clean ablations)
    # ------------------------------------------------------------------

    def retrieve(
        self,
        query: str,
        method: str = "bm25_okf_ppr_slots",
        top_k: int = 10,
        client: Optional[OpenAICompatibleClient] = None,
        embed_model: Optional[str] = None,
        embed_dim: Optional[int] = None,
        rerank_model: Optional[str] = None,
        extra_subqueries: Optional[Sequence[str]] = None,
    ) -> List[Dict[str, Any]]:
        """Retrieve top_k fair <=256-token chunks under the requested ablation method."""
        if method == "bm25_raw":
            hits = self.search_fts_raw(query, top_k=top_k)
            return self._format_results(hits)

        if method == "bm25_okf_meta":
            hits = self.search_fts_meta(query, top_k=top_k)
            return self._format_results(hits)

        # ---- Stage 1: Multi-channel RRF Fusion (Meta-BM25 + Raw-BM25 + Slots + Optional Dense) ----
        rrf_k = 40.0
        fused: Dict[int, float] = defaultdict(float)

        meta_hits = self.search_fts_meta(query, top_k=45)
        raw_hits = self.search_fts_raw(query, top_k=35)
        for rank, (cid, _) in enumerate(meta_hits, start=1):
            fused[cid] += 1.4 / (rrf_k + rank)
        for rank, (cid, _) in enumerate(raw_hits, start=1):
            fused[cid] += 0.9 / (rrf_k + rank)

        # Optional Dense Embedding channel
        query_vec: Optional[List[float]] = None
        if method in ("hybrid_dense_ppr", "hybrid_dense_ppr_rerank") and self.chunk_vectors and client and embed_model:
            vecs = client.embed_batch(model=embed_model, texts=[query], dimensions=embed_dim, batch_size=1)
            query_vec = vecs[0] if vecs else None
            if query_vec:
                dense_hits = self.search_dense(query_vec, top_k=45)
                for rank, (cid, _) in enumerate(dense_hits, start=1):
                    fused[cid] += 1.5 / (rrf_k + rank)

        # Multi-Slot Decomposition channel (critical for multi-source / temporal / comparison hops!)
        slots = self.decompose_query_slots(query)
        slot_top_chunks: List[List[int]] = []
        if slots:
            slot_weight = 1.6 / max(1, len(slots))
            for slot in slots:
                s_hits = self.search_fts_meta(slot["query"], top_k=50)
                # Apply strict metadata boost when slot specifies an explicit source or date
                s_src = slot.get("source")
                s_date = slot.get("date")
                s_time = slot.get("time")
                rescored: List[Tuple[int, float]] = []
                for rank, (cid, score) in enumerate(s_hits, start=1):
                    c = self.chunks[cid]
                    bonus = 1.0
                    if s_src and c.source.lower() == s_src:
                        bonus *= 1.65
                    elif s_src and c.source.lower() != s_src:
                        bonus *= 0.45
                    if s_date and c.published_iso == s_date:
                        bonus *= 1.60
                    if s_time and s_time in c.published_natural:
                        bonus *= 1.85
                    rescored.append((cid, (1.0 / (25.0 + rank)) * bonus))
                rescored.sort(key=lambda x: x[1], reverse=True)
                slot_top_chunks.append([cid for cid, _ in rescored[:6]])
                for rank, (cid, _) in enumerate(rescored[:25], start=1):
                    fused[cid] += slot_weight / (25.0 + rank)

        # Optional LLM-generated subqueries (if provided)
        if extra_subqueries:
            sub_w = 1.2 / len(extra_subqueries)
            for sq in extra_subqueries:
                sq_hits = self.search_fts_meta(sq, top_k=25)
                for rank, (cid, _) in enumerate(sq_hits, start=1):
                    fused[cid] += sub_w / (30.0 + rank)
                if query_vec is not None and client and embed_model and self.chunk_vectors:
                    sq_vec = client.embed_batch(model=embed_model, texts=[sq], dimensions=embed_dim, batch_size=1)[0]
                    for rank, (cid, _) in enumerate(self.search_dense(sq_vec, top_k=25), start=1):
                        fused[cid] += sub_w / (30.0 + rank)

        # ---- Stage 2: Passage-Entity Graph PPR (HippoRAG 2 style) ----
        seed_ranked = sorted(fused.items(), key=lambda x: x[1], reverse=True)[:15]
        seed_map = {cid: score for cid, score in seed_ranked}
        q_entities = extract_entities(query)
        ppr_hits = self.run_passage_entity_ppr(seed_map, q_entities, alpha=0.55, iterations=4, top_k=40)
        for rank, (cid, _) in enumerate(ppr_hits, start=1):
            fused[cid] += 0.70 / (35.0 + rank)

        candidates = sorted(fused.items(), key=lambda x: x[1], reverse=True)[:35]

        # ---- Stage 3: Optional Cross-Encoder Reranker ----
        if method == "hybrid_dense_ppr_rerank" and client and rerank_model and candidates:
            cand_ids = [cid for cid, _ in candidates[:28]]
            cand_texts = [self.chunks[cid].contextual_text for cid in cand_ids]
            reranked = client.rerank(
                model=rerank_model,
                query=query,
                documents=cand_texts,
                top_n=min(20, len(cand_ids)),
            )
            # Fuse reranker score with prior RRF+PPR rank so multi-slot diversity is preserved
            final_scores: Dict[int, float] = {}
            for r_rank, (local_idx, rel_score) in enumerate(reranked, start=1):
                cid = cand_ids[local_idx]
                prior = fused.get(cid, 0.0)
                final_scores[cid] = 0.72 * rel_score + 0.28 * (1.0 / (10.0 + r_rank)) + 0.15 * prior
            candidates = sorted(final_scores.items(), key=lambda x: x[1], reverse=True)

        # ---- Stage 4: Multi-Article / Multi-Slot Diverse Top-K Selection ----
        # Prevent a single long roundup article from taking >2 slots while multi-hop evidence sits in other articles
        selected: List[Tuple[int, float]] = []
        selected_ids: Set[int] = set()
        doc_counts: Counter = Counter()
        cand_score_map = dict(candidates)

        # First reserve the top-1 highest-scoring candidate from each distinct detected slot (if in top pool)
        if len(slot_top_chunks) >= 2:
            for s_list in slot_top_chunks:
                for cid in s_list:
                    doc_id = self.chunks[cid].doc_id
                    if cid not in selected_ids and doc_counts[doc_id] < 2:
                        selected.append((cid, cand_score_map.get(cid, fused.get(cid, 0.0))))
                        selected_ids.add(cid)
                        doc_counts[doc_id] += 1
                        break

        # Fill remaining slots by score with soft cap (<=2 chunks per article first, then relax if needed)
        for max_per_doc in (2, 4, top_k):
            for cid, score in candidates:
                if len(selected) >= top_k:
                    break
                if cid in selected_ids:
                    continue
                doc_id = self.chunks[cid].doc_id
                if doc_counts[doc_id] >= max_per_doc:
                    continue
                selected.append((cid, score))
                selected_ids.add(cid)
                doc_counts[doc_id] += 1
            if len(selected) >= top_k:
                break

        # Re-sort selected top-K by score so MRR/MAP stay maximal
        selected.sort(key=lambda x: x[1], reverse=True)
        return self._format_results(selected[:top_k])

    def _format_results(self, hits: Sequence[Tuple[int, float]]) -> List[Dict[str, Any]]:
        out: List[Dict[str, Any]] = []
        for cid, score in hits:
            c = self.chunks[cid]
            out.append(
                {
                    "chunk_id": c.chunk_id,
                    "doc_id": c.doc_id,
                    "title": c.title,
                    "source": c.source,
                    "published_at": c.published_iso,
                    "url": c.url,
                    "score": round(float(score), 6),
                    "body_tokens": c.body_tokens,
                    # Strictly the <=256-token body chunk (zero gaming!)
                    "text": c.body,
                }
            )
        return out
