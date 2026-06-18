# PR #102 Review Fixes — Design Spec

**Date:** 2026-06-18  
**PR:** https://github.com/wfnuser/cowiki/pull/102  
**Review:** wfnuser (Changes Requested)  
**Status:** Design

---

## Overview

Address all issues raised in the PR #102 review across four severity tiers:
🔴 Critical (1), 🟠 High (6), 🟡 Medium (7), 🟢 Low (4).

The overarching principle: follow existing codebase patterns. Auth guards, config loading,
error handling — all have established idioms in the repo.

---

## 1. 🔴 Critical

### 1.1 Restore `compile_ws` Authorization

**File:** `crates/server/src/routes/compile.rs`  
**Pattern:** Match `write_page_ws` (`pages.rs:250–260`) exactly.

**Current (no auth):**
```rust
pub async fn compile_ws(
    State(state): State<Arc<AppState>>,
    Path(ws_slug): Path<String>,
    Json(input): Json<CompileRequest>,
) -> Result<Json<CompileResponse>> {
```

**After fix:**
```rust
pub async fn compile_ws(
    State(state): State<Arc<AppState>>,
    Path(ws_slug): Path<String>,
    headers: axum::http::HeaderMap,
    Json(input): Json<CompileRequest>,
) -> Result<Json<CompileResponse>> {
    // Auth: membership + EditContent + own branch only
    let guard = crate::routes::guard::require_membership(&state, &headers, &ws_slug).await?;
    crate::routes::guard::require(&guard, crate::routes::guard::Permission::EditContent)?;
    super::pages::require_own_branch(&input.branch, guard.user.id)?;

    let repo = state.repo_manager.get(&ws_slug)
        .map_err(|e| AppError::Internal(format!("repo error: {e}")))?;
    super::pages::ensure_user_branch_if_needed(&repo, &input.branch)?;
    // ... rest unchanged
```

**Rationale:** Every other mutating endpoint in the server requires these three guards.
The agent path makes each call more expensive (subprocess, up to 20 LLM rounds), so
the auth gap is proportionally more dangerous.

---

## 2. 🟠 High

### 2.1 Authenticate SSE Event Stream

**Files:** `crates/server/src/main.rs` (handler), `web/src/hooks/useCompileStream.ts` (client)

**Problem:** `/api/agents/{ws}/events` takes no headers, subscribes to a process-wide
`broadcast::channel(256)`, and broadcasts every workspace's agent activity to any
anonymous client.

**Fix — backend:**

1. Add `headers: axum::http::HeaderMap` to `agent_events` signature
2. Authenticate: `let guard = guard::require_membership(&state, &headers, &ws).await?`
3. Add `workspace` field to `AgentEvent` (tagged at emission time)
4. Filter stream: `if event.workspace != ws { continue; }` in the relay loop
5. Move `CorsLayer::permissive()` to only non-sensitive routes — agent events gets
   restricted CORS (credentials required)

**Fix — frontend:**

`EventSource` cannot send `Authorization: Bearer` headers. Replace with `fetch` +
`ReadableStream`:

```typescript
// Replace: const es = new EventSource(url);
// With:
const response = await fetch(url, {
  headers: authHeaders(),  // from api.ts
});
const reader = response.body!.getReader();
// Parse SSE frames manually from the stream
```

Add a reusable `authHeaders()` helper to `api.ts` that reads the stored API key / token
and returns `{ Authorization: "Bearer ..." }`. The stream token pattern (short-lived,
single-use token passed via query param) was rejected — never put auth in URL query strings.

### 2.2 MCP Server Auth + Slug Validation

**Files:** `crates/mcp/src/lib.rs`, `crates/core/src/gateway.rs`

**Problem (a):** Every MCP tool does `ctx._workspace.as_deref().unwrap_or("default")`
and passes it to `WikiRepoManager::get`, which never validates the slug. Caller controls
the workspace → cross-tenant access + path traversal (`_workspace: "../../tmp/pwn"`).

**Fix — gateway slug validation:**

Add to `WikiFsGateway::execute` (or `WikiRepoManager::get`), before the first filesystem access:

```rust
fn validate_workspace_slug(slug: &str) -> Result<(), String> {
    if slug.is_empty() || slug.contains("..") || slug.contains('/') || slug.contains('\\') {
        return Err(format!("invalid workspace slug: {slug}"));
    }
    Ok(())
}
```

**Problem (b):** No auth on the MCP transport. Only the `127.0.0.1:9380` bind prevents
remote exploitation — that bind is now load-bearing.

**Fix — MCP server:**

MCP server runs on loopback only and is spawned by the cowiki server process. Auth is
via a shared secret token:

1. Server generates a random token at startup, stores it in `AgentManager`
2. MCP binary accepts `--auth-token <token>` CLI arg
3. Server passes the token when spawning MCP binary (or via env `COWIKI_MCP_AUTH_TOKEN`)
4. MCP server validates the `Authorization: Bearer <token>` header on every request
5. Workspace scope is bound at MCP server startup (single-tenant per MCP instance), not
   derived from caller parameters

The `_workspace` / `_branch` params are removed from tool schemas — the MCP server is
configured with a single workspace at startup and all operations are scoped to it.

### 2.3 Fix `cowiki_search` Table + Scope

**File:** `crates/core/src/gateway.rs:286–293`

**Problem:** SQL queries `wiki_pages` (nonexistent table) with no workspace filter.

**Real schema** (from migrations 005, 011):

| Migration | Table | Key columns |
|-----------|-------|-------------|
| 005 | `pages` | `slug, title, summary, body, branch, hash, embedding, tsv` |
| 011 | `pages_workspace_scope` | added `workspace_slug` to `pages` |

**Fix:** Follow the pattern from `crates/db/src/pages.rs`:

```sql
SELECT slug, ts_rank(tsv, query) AS rank
FROM pages, plainto_tsquery('english', $1) AS query
WHERE tsv @@ query
  AND workspace_slug = $2
ORDER BY rank DESC
LIMIT 20
```

`branch` filtering is intentionally omitted for search — agents need to see what exists
before deciding whether to write.

### 2.4 Agent Hard Timeout (Idle → Kill) + Soft Timeout (Total → Warn)

**Files:** `crates/agents/src/harness/pi.rs`, `crates/agents/src/types/protocol.rs`,
`crates/utils/src/lib.rs`

**Design rationale:** A pure total timeout is too blunt — agents doing legitimate work
(20 rounds, large files) would be killed mid-task. Instead, use two independent timers:

| Timeout | Trigger | Action | Default |
|---------|---------|--------|---------|
| **Hard timeout** (`hard_timeout_secs`) | stdout produces zero lines for N seconds | `child.kill()`, return `AgentError::Timeout` | 300s |
| **Soft timeout** (`soft_timeout_secs`) | total elapsed since connect | `tracing::warn!`, stream continues | 1800s |

**Config source:** New `[agent]` section in `cowiki.conf` and env vars:

```toml
[agent]
hard_timeout_secs = 300
soft_timeout_secs = 1800
max_concurrent_per_workspace = 4
```

Env fallback: `COWIKI_AGENT_HARD_TIMEOUT_SECS`, `COWIKI_AGENT_SOFT_TIMEOUT_SECS`,
`COWIKI_AGENT_MAX_CONCURRENT`.

**Implementation (pi.rs read-loop):**

```rust
let hard_timeout = Duration::from_secs(config.hard_timeout_secs);
let soft_timeout = Duration::from_secs(config.soft_timeout_secs);
let started_at = Instant::now();

loop {
    tokio::select! {
        line = lines.next_line() => {
            match line {
                Ok(Some(line)) => {
                    // Reset idle timer on every stdout line
                    // ... parse NDJSON event ...
                }
                Ok(None) => break,  // stdout closed
                Err(e) => { /* log, break */ }
            }
        }
        _ = tokio::time::sleep(hard_timeout) => {
            // No output for hard_timeout_secs → kill
            tracing::error!(agent=%name, "hard timeout: no output for {hard_timeout:?}");
            child.kill().await.ok();
            return Err(AgentError::Timeout("hard timeout: agent unresponsive".into()));
        }
        _ = tokio::time::sleep(soft_timeout.saturating_sub(started_at.elapsed())) => {
            // Total elapsed > soft_timeout_secs → warn only
            tracing::warn!(agent=%name, elapsed=?started_at.elapsed(),
                "soft timeout: agent still running, continuing...");
        }
    }
}
```

The idle timer resets on every parsed stdout line (not every read — only successful NDJSON
events, so garbage output won't reset it).

### 2.5 Per-Workspace Concurrency Cap

**Files:** `crates/agents/src/manager/mod.rs`, `crates/utils/src/lib.rs`

**Design:** A global semaphore would let one busy workspace starve others. Instead,
maintain a per-workspace semaphore map.

```rust
pub struct AgentManager {
    // ... existing fields ...
    /// Per-workspace concurrency semaphores
    concurrency: RwLock<HashMap<String, Arc<tokio::sync::Semaphore>>>,
    /// Max concurrent agents per workspace (from config)
    max_concurrent: usize,
}
```

**In `dispatch_agent`:**

```rust
let sem = {
    let mut map = self.concurrency.write().await;
    map.entry(workspace.clone())
       .or_insert_with(|| Arc::new(Semaphore::new(self.max_concurrent)))
       .clone()
};
let _permit = sem.acquire().await
    .map_err(|_| AgentError::PoolExhausted)?;
// ... spawn pi, wait for result ...
// _permit drops here, releases slot
```

N concurrent requests on workspace A queue behind the semaphore; workspace B is
unaffected. Quota is explicit in config (`max_concurrent_per_workspace`, default 4).

---

## 3. 🟡 Medium

### 3.1 Wire `stop_agent`

**File:** `crates/agents/src/manager/mod.rs` + `process.rs`

**Problem:** `AgentProcess.child` is always `None` — `processes` map is never populated
with a live `Child`. `stop_agent` → `process::stop_agent` tries to `.kill()` a process
that doesn't exist → silent no-op.

**Fix:** In `spawn_and_wait`, after spawning the child, insert it into the processes map:

```rust
let child = cmd.spawn()...;
self.insert_process(name, AgentProcess {
    status: AgentStatus::Running,
    agent_id: task.task_id.clone(),
    last_active: Instant::now(),
    manifest: manifest.clone(),
    child: Some(child),  // <-- actually store the handle
}).await;
```

Then `stop_agent` can `.take()` the child and `.kill()` it. After the process exits
(via `wait()`), remove the entry from the map.

### 3.2 Delete Dead Code: `crates/db/src/embed.rs`

**File:** `crates/db/src/embed.rs` (292 lines)

**Evidence it's dead:** `crates/db/src/lib.rs` has no `pub mod embed;`. The file is
never compiled. It's an out-of-date copy of `crate::core::ai::embedder::openai`,
missing nv-embed truncation fixes landed in this PR.

**Action:** `rm crates/db/src/embed.rs`. Nothing references it.

### 3.3 Delete Dead Code: `web/src/components/CompileDrawer.tsx`

**File:** `web/src/components/CompileDrawer.tsx` (505 lines)

**Evidence it's dead:** Only referenced in a comment. `MainLayout` renders
`AgentDrawer` → `AgentLogView`. The tabbed drawer absorbed its functionality.

**Action:** `rm web/src/components/CompileDrawer.tsx`. Check for any imports and
remove them (there should be none).

### 3.4 Don't Discard Indexing Errors

**File:** `crates/core/src/compile.rs:333`

```rust
// Before (silent failure):
let _ = cowiki_db::pages::upsert(db, ...).await;

// After (logged warning):
if let Err(e) = cowiki_db::pages::upsert(db, ...).await {
    tracing::warn!(slug = %slug, error = %e,
        "page written to git but DB upsert failed — page not searchable");
}
```

The page is already written to git (by the agent via MCP), so we don't fail the compile.
But we must log the indexing failure so operators know the page won't appear in search.

### 3.5 Validate Embedder Batch Response Length

**File:** `crates/core/src/ai/embedder/openai.rs:203–211`

**Problem:** `try_embed_batch_inner` maps `resp.data` into results without checking
`data.len() == texts.len()`. If the provider returns fewer embeddings than requested,
the caller gets truncated results with no error.

**Fix:**

```rust
let results: Vec<_> = resp.data.into_iter()
    .map(|d| EmbedResult { vector: d.embedding })
    .collect();

if results.len() != texts.len() {
    return Err(format!(
        "embedder batch: expected {} embeddings, got {}",
        texts.len(), results.len()
    ));
}

// Guard against zero-length embeddings
for (i, r) in results.iter().enumerate() {
    if r.vector.is_empty() {
        return Err(format!(
            "embedder batch: empty embedding at index {i}"
        ));
    }
}
```

### 3.6 Fix SSE Reconnect Logic

**File:** `web/src/hooks/useCompileStream.ts:140–156`

**Problem:** `es.onerror` calls `es.close()` but never sets `eventSourceRef.current = null`.
The reconnect guard at line 147 checks `eventSourceRef.current === null` → never fires.

**Fix:**

```typescript
es.onerror = () => {
  es.close();
  eventSourceRef.current = null;  // <-- add this line
  if (reconnectCount.current < MAX_RECONNECT_ATTEMPTS) {
    reconnectCount.current++;
    setReconnecting(true);
    const delay = Math.min(1000 * Math.pow(2, reconnectCount.current - 1), 4000);
    setTimeout(() => {
      if (eventSourceRef.current === null) {  // now this guard works
        connectSSE();
      }
    }, delay);
  } else {
    // ...
  }
};
```

Also remove the `mode` field from `AgentEvent` type — it's declared but never emitted
by the server (connected event only has `type: "connected"` and `message`).

### 3.7 Update Stale Documentation

**Files:** `docs/mcp.md`, `README.md`, `docs/config.md`

**Problem:** `docs/mcp.md` still documents the old REST proxy with Bearer auth.
`README.md:78` and `docs/config.md` reference `cowiki-mcp-server` as a standalone
proxy when it no longer exists.

**Fixes:**

`docs/mcp.md`:
- Remove references to the old REST proxy architecture
- Document the current rmcp-based MCP server with Streamable HTTP transport
- Document the 5 tools: `cowiki_list`, `cowiki_read`, `cowiki_write`, `cowiki_remove`, `cowiki_search`
- Document the `_workspace`/`_branch`/`_execution_id` context params
- Note that the MCP server runs on loopback only (127.0.0.1:9380)

`README.md`:
- Remove `cowiki-mcp-server` from the architecture overview
- Add `crates/mcp/` to the crate listing
- Document `COWIKI_MCP_PORT` env var

`docs/config.md`:
- Add `[mcp-server]` section for MCP port config
- Remove references to the deleted `cowiki.conf.example`
- Add `[agent]` section documentation

---

## 4. 🟢 Low

### 4.1 COWIKI_MCP_PORT Ambiguity

**Problem:** `crates/mcp/src/main.rs:21` reads `COWIKI_MCP_PORT` as a full bind address
(e.g. `127.0.0.1:9380`), but `crates/server/src/main.rs:171` reads it as port-only (`"9380"`).
If an operator sets `COWIKI_MCP_PORT=127.0.0.1:9380`, the standalone MCP binary works
but the server's `http://127.0.0.1:{127.0.0.1:9380}/mcp/sse` breaks.

**Fix:** Standardize on port-only. The MCP binary binds `127.0.0.1:{port}` unconditionally
(since it should never listen on a non-loopback address). Server constructs the URL as
`http://127.0.0.1:{port}/mcp/sse`.

### 4.2 Token Counter Overflow

**File:** `crates/utils/src/token_usage.rs`

**Problem:** Token counters are `u32`. In debug mode, overflow panics; in release, it wraps
silently. Long-running servers will eventually overflow.

**Fix:** Change `u32` → `u64` in `TokenUsage` struct fields and `TokenUsageTracker::record`
parameters.

### 4.3 Dead `CompileMode::Pipeline` Branch

**File:** `crates/core/src/compile.rs:212–248`

**Problem:** Both `use_pipeline == true` and `use_pipeline == false` call
`compile_via_agent(...)` with identical arguments. The `Pipeline` variant exists but the
fixed 4-step pipeline was moved to the cowiki_compiler agent harness. `CompileMode` is
dead weight.

**Fix:** Remove `CompileMode` enum and the `mode` field from `CompileRequest`. Keep the
`CompileRequest` struct minimal (just `branch`). Remove the dead `use_pipeline` branch.

### 4.4 Remove `tracing::warn!` Log Spam

**File:** `crates/core/src/compile.rs:161`

```rust
tracing::warn!(branch = %branch, count = source_files.len(), files = ?source_files,
    "compile: listed sources");
```

This logs every source file path at `warn!` level on every compile. Use `debug!` instead.

### 4.5 `/api/usage` May Be Unauthenticated

**File:** `crates/server/src/main.rs:40–47`

The `get_usage` handler takes no headers — anyone can query LLM/embedder token usage
counts. This leaks operational metrics. Add `extract_user` guard (authenticated users
only, no workspace membership needed — just needs a valid API key).

---

## 5. Implementation Order

1. **Config first** — `AgentConfig` in `cowiki_utils` (unblocks #1.1, #2.4, #2.5)
2. **Critical auth** — `compile_ws` guards (#1.1)
3. **High severity** — SSE auth (#2.1), MCP auth (#2.2), search fix (#2.3), timeouts (#2.4), concurrency (#2.5)
4. **Medium** — dead code deletion, error handling, reconnect fix, docs (#3.1–#3.7)
5. **Low** — misc cleanups (#4.1–#4.5)
6. **Verify** — `cargo build`, `cargo test`, `cargo clippy`, manual e2e compile test

---

## 6. Non-Goals (for this PR fix)

- Full MCP multi-tenancy (the MCP server is single-workspace by design for now)
- Agent streaming mode (OneShot is the only implemented mode; stream registry is
  scaffolding for future use)
- Frontend visual redesign of the agent drawer
- Adding new agent task types beyond compile/deep-compile/review
