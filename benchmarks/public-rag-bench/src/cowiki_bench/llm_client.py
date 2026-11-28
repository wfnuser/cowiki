"""Provider-agnostic OpenAI-compatible + DashScope API client with persistent SQLite caching.

Zero third-party dependencies (uses Python stdlib `urllib.request` and `sqlite3`).
NEVER prints, logs, or persists API keys.
"""

from __future__ import annotations

import concurrent.futures
import hashlib
import json
import os
import sqlite3
import threading
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Dict, List, Optional, Sequence, Tuple


def load_secret_from_env_or_file(
    env_names: Sequence[str],
    env_file: Optional[Path] = None,
) -> Optional[str]:
    """Read an API key from process env vars or an explicit `.env` file."""
    for name in env_names:
        val = os.environ.get(name)
        if val and val.strip():
            return val.strip()
    if env_file and env_file.is_file():
        for raw_line in env_file.read_text(encoding="utf-8").splitlines():
            line = raw_line.strip()
            if not line or line.startswith("#") or "=" not in line:
                continue
            k, v = line.split("=", 1)
            k = k.strip()
            if k in env_names:
                val = v.strip().strip('"').strip("'")
                if val:
                    return val
    return None


@dataclass
class ChatResult:
    content: str
    prompt_tokens: int
    completion_tokens: int
    cached: bool
    latency_ms: float


class PersistentApiCache:
    """Thread-safe SQLite content-addressed cache for chat, embedding, and rerank calls."""

    def __init__(self, db_path: Path):
        self.db_path = db_path
        db_path.parent.mkdir(parents=True, exist_ok=True)
        self._lock = threading.Lock()
        self._conn = sqlite3.connect(str(db_path), timeout=30.0, check_same_thread=False)
        self._conn.execute("PRAGMA journal_mode=WAL;")
        self._conn.execute("PRAGMA busy_timeout=30000;")
        self._conn.execute(
            """
            CREATE TABLE IF NOT EXISTS kv_cache (
                cache_key TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                created_at REAL NOT NULL
            )
            """
        )
        self._conn.commit()

    def get(self, key: str) -> Optional[Any]:
        with self._lock:
            row = self._conn.execute(
                "SELECT payload_json FROM kv_cache WHERE cache_key = ?", (key,)
            ).fetchone()
        if row is None:
            return None
        return json.loads(row[0])

    def put(self, key: str, kind: str, payload: Any) -> None:
        raw = json.dumps(payload, ensure_ascii=False)
        with self._lock:
            self._conn.execute(
                "INSERT OR REPLACE INTO kv_cache (cache_key, kind, payload_json, created_at) VALUES (?, ?, ?, ?)",
                (key, kind, raw, time.time()),
            )
            self._conn.commit()

    def close(self) -> None:
        with self._lock:
            self._conn.close()


class OpenAICompatibleClient:
    """Minimal OpenAI-compatible client supporting chat completions, embeddings, and DashScope/Jina rerank."""

    def __init__(
        self,
        api_key: str,
        base_url: str,
        cache: Optional[PersistentApiCache] = None,
        timeout_sec: float = 90.0,
        max_retries: int = 5,
    ):
        self._api_key = api_key
        self.base_url = base_url.rstrip("/")
        self.cache = cache
        self.timeout_sec = timeout_sec
        self.max_retries = max_retries
        self.stats = {"api_calls": 0, "cache_hits": 0, "prompt_tokens": 0, "completion_tokens": 0}
        self._stats_lock = threading.Lock()

    def _hash_key(self, kind: str, payload: Dict[str, Any]) -> str:
        blob = json.dumps({"kind": kind, "base": self.base_url, "payload": payload}, sort_keys=True, ensure_ascii=False)
        return hashlib.sha256(blob.encode("utf-8")).hexdigest()

    def _post_json(self, url: str, body: Dict[str, Any], extra_headers: Optional[Dict[str, str]] = None) -> Dict[str, Any]:
        headers = {
            "Content-Type": "application/json",
            "Authorization": f"Bearer {self._api_key}",
        }
        if extra_headers:
            headers.update(extra_headers)
        data = json.dumps(body).encode("utf-8")
        last_err: Optional[Exception] = None
        for attempt in range(self.max_retries):
            req = urllib.request.Request(url, data=data, headers=headers, method="POST")
            try:
                with urllib.request.urlopen(req, timeout=self.timeout_sec) as resp:
                    return json.loads(resp.read().decode("utf-8"))
            except urllib.error.HTTPError as err:
                err_body = err.read().decode("utf-8", errors="replace")[:400]
                last_err = RuntimeError(f"HTTP {err.code} from {url}: {err_body}")
                if err.code in (429, 500, 502, 503, 504) and attempt + 1 < self.max_retries:
                    time.sleep(1.5 * (2 ** attempt))
                    continue
                raise last_err from None
            except Exception as err:
                last_err = err
                if attempt + 1 < self.max_retries:
                    time.sleep(1.5 * (2 ** attempt))
                    continue
                raise
        assert last_err is not None
        raise last_err

    def chat(
        self,
        model: str,
        messages: List[Dict[str, str]],
        temperature: float = 0.0,
        max_tokens: int = 1024,
        json_mode: bool = False,
        extra_body: Optional[Dict[str, Any]] = None,
    ) -> ChatResult:
        t0 = time.perf_counter()
        body: Dict[str, Any] = {
            "model": model,
            "messages": messages,
            "temperature": temperature,
            "max_tokens": max_tokens,
        }
        if json_mode:
            body["response_format"] = {"type": "json_object"}
        if extra_body:
            body.update(extra_body)

        key = self._hash_key("chat", body)
        if self.cache is not None:
            hit = self.cache.get(key)
            if hit is not None:
                with self._stats_lock:
                    self.stats["cache_hits"] += 1
                    self.stats["prompt_tokens"] += int(hit.get("prompt_tokens", 0))
                    self.stats["completion_tokens"] += int(hit.get("completion_tokens", 0))
                return ChatResult(
                    content=hit["content"],
                    prompt_tokens=int(hit.get("prompt_tokens", 0)),
                    completion_tokens=int(hit.get("completion_tokens", 0)),
                    cached=True,
                    latency_ms=(time.perf_counter() - t0) * 1000.0,
                )

        resp = self._post_json(f"{self.base_url}/chat/completions", body)
        content = resp["choices"][0]["message"]["content"] or ""
        usage = resp.get("usage", {})
        pt = int(usage.get("prompt_tokens", 0))
        ct = int(usage.get("completion_tokens", 0))
        with self._stats_lock:
            self.stats["api_calls"] += 1
            self.stats["prompt_tokens"] += pt
            self.stats["completion_tokens"] += ct
        if self.cache is not None:
            self.cache.put(key, "chat", {"content": content, "prompt_tokens": pt, "completion_tokens": ct})
        return ChatResult(
            content=content,
            prompt_tokens=pt,
            completion_tokens=ct,
            cached=False,
            latency_ms=(time.perf_counter() - t0) * 1000.0,
        )

    def embed_batch(
        self,
        model: str,
        texts: Sequence[str],
        dimensions: Optional[int] = None,
        batch_size: int = 10,
        max_workers: int = 6,
    ) -> List[List[float]]:
        results: List[Optional[List[float]]] = [None] * len(texts)
        missing_indices: List[int] = []
        missing_keys: List[str] = []

        for idx, txt in enumerate(texts):
            payload = {"model": model, "text": txt, "dimensions": dimensions}
            key = self._hash_key("embed", payload)
            cached = self.cache.get(key) if self.cache is not None else None
            if cached is not None:
                results[idx] = cached
                with self._stats_lock:
                    self.stats["cache_hits"] += 1
            else:
                missing_indices.append(idx)
                missing_keys.append(key)

        batches: List[Tuple[List[int], List[str], List[str]]] = []
        for start in range(0, len(missing_indices), batch_size):
            b_idx = missing_indices[start : start + batch_size]
            b_keys = missing_keys[start : start + batch_size]
            b_texts = [texts[i] for i in b_idx]
            batches.append((b_idx, b_keys, b_texts))

        def _fetch_batch(batch: Tuple[List[int], List[str], List[str]]) -> List[Tuple[int, str, List[float]]]:
            b_idx, b_keys, b_texts = batch
            body: Dict[str, Any] = {"model": model, "input": b_texts}
            if dimensions is not None:
                body["dimensions"] = dimensions
            resp = self._post_json(f"{self.base_url}/embeddings", body)
            with self._stats_lock:
                self.stats["api_calls"] += 1
            data_rows = sorted(resp["data"], key=lambda x: x["index"])
            out_rows: List[Tuple[int, str, List[float]]] = []
            for i, k, row in zip(b_idx, b_keys, data_rows):
                vec = [float(v) for v in row["embedding"]]
                out_rows.append((i, k, vec))
            return out_rows

        if len(batches) <= 1 or max_workers <= 1:
            for b in batches:
                for i, k, vec in _fetch_batch(b):
                    results[i] = vec
                    if self.cache is not None:
                        self.cache.put(k, "embed", vec)
        else:
            with concurrent.futures.ThreadPoolExecutor(max_workers=max_workers) as pool:
                for batch_res in pool.map(_fetch_batch, batches):
                    for i, k, vec in batch_res:
                        results[i] = vec
                        if self.cache is not None:
                            self.cache.put(k, "embed", vec)

        return [r if r is not None else [] for r in results]

    def rerank(
        self,
        model: str,
        query: str,
        documents: Sequence[str],
        top_n: int = 10,
        rerank_url: Optional[str] = None,
    ) -> List[Tuple[int, float]]:
        """Call DashScope native rerank API or Jina/Cohere/OpenAI-compatible `/rerank` endpoint."""
        if not documents:
            return []
        url = rerank_url
        is_dashscope = False
        if not url:
            if "dashscope.aliyuncs.com" in self.base_url:
                is_dashscope = True
                url = "https://dashscope.aliyuncs.com/api/v1/services/rerank/text-rerank/text-rerank"
            else:
                url = f"{self.base_url}/rerank"
        elif "services/rerank/text-rerank" in url:
            is_dashscope = True

        cache_payload = {"model": model, "query": query, "documents": list(documents), "top_n": top_n, "url": url}
        key = self._hash_key("rerank", cache_payload)
        if self.cache is not None:
            hit = self.cache.get(key)
            if hit is not None:
                with self._stats_lock:
                    self.stats["cache_hits"] += 1
                return [(int(x[0]), float(x[1])) for x in hit]

        if is_dashscope:
            body = {
                "model": model,
                "input": {"query": query, "documents": list(documents)},
                "parameters": {"top_n": min(top_n, len(documents)), "return_documents": False},
            }
            resp = self._post_json(url, body)
            raw_results = resp.get("output", {}).get("results", [])
        else:
            body = {
                "model": model,
                "query": query,
                "documents": list(documents),
                "top_n": min(top_n, len(documents)),
            }
            resp = self._post_json(url, body)
            raw_results = resp.get("results", [])

        with self._stats_lock:
            self.stats["api_calls"] += 1
        ranked = [(int(r["index"]), float(r["relevance_score"])) for r in raw_results]
        if self.cache is not None:
            self.cache.put(key, "rerank", ranked)
        return ranked
