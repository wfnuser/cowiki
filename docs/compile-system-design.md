# cowiki Compile System Design

> Status: v3 | Date: 2026-06-07 | Issue: [#15](https://github.com/wfnuser/cowiki/issues/15)

## Overview

A **two-stage compile pipeline** with a **decoupled agent communication layer**.

- **ShallowCompile (sync)**: Agent explores source directories (with `ls`/`grep`/`read` tools) and organizes extracted knowledge into wiki pages via `create_wiki`/`rm_wiki`. Personal Space: pages immediately inserted into pgvector. Team Space: pages held until review approve.
- **DeepIntegrate (async, per-space mutex)**: Two sequential agent calls — first reorganizes wiki directory structure, then extracts entities and fact triples to build a knowledge graph. Trigger depends on space type.

Agents are existing frameworks (deepagents, agnt, Swink Agent) running as separate HTTP/gRPC processes. All agent actions are streamed to the frontend via SSE for full observability.

## Data Flow

```
Personal Space:
  ShallowCompile              DeepIntegrate
  ─────────────               ────────────
  Source Dir → Wiki Pages     Wiki Reorg + Graph
               + pgvector ↑

Team Space:
  ShallowCompile    Submit → Review → Approve     DeepIntegrate
  ─────────────     ─────────────────────────     ────────────
  Source Dir →      pgvector insert +              Wiki Reorg
  Wiki Pages        DeepIntegrate trigger          + Graph
  (NO pgvector)
```

## Architecture

### Trigger Model

| Space | ShallowCompile | pgvector Insert | DeepIntegrate |
|-------|---------------|-----------------|---------------|
| **Personal Space** | `POST /api/compile` | Immediately after ShallowCompile | Immediately after ShallowCompile |
| **Team Space** | `POST /api/compile` | On review approve | On review approve |

**Rationale:** Team Space pages should not pollute the search index before human review. pgvector insertion and knowledge graph construction both happen after approval.

### System Layers

```
POST /api/compile
        │
┌───────▼──────────────────────────────────────────────────────┐
│  crates/core/compiler/          (business logic)              │
│                                                               │
│  ShallowCompile                  DeepIntegrate                │
│  source dir → wiki pages        per-space mutex:              │
│  (sync, returns immediately)    1) wiki reorg                 │
│                                 2) graph build                │
│  Agent Pool (per-space)         (async)                       │
└──────────────────────────────────────────────────────────────┘
        │ AgentRequest (protocol) + SSE event stream
        ▼
  crates/agents/  (communication layer)
```

### Module Boundaries

| Crate | Owns | Depends On |
|-------|------|------------|
| `crates/core/src/client/` | ApiClient: pages, compile, ingest, search, graph (server-internal) | reqwest, core/models |
| `crates/core/src/compiler/` | ShallowCompile, DeepIntegrate, SourceDispatch, AgentPool | agents, core/models, core/client, db |
| `crates/agents/` | protocol, AgentClient (HTTP/gRPC), HarnessRegistry, DispatchPolicy, SSE events | core/client |
| `crates/db/` | pages, entities, facts, page_entities tables | sqlx, pgvector |
| `cli/` | CLI tool (independent, own HTTP client) | reqwest, clap — no crates dep |
| `cowiki-mcp-server/` | MCP server (independent, own HTTP client) | reqwest, rmcp — no crates dep |

## Source Directory Structure

Each ingested source becomes a **directory** (built by extractor, per issue [#31](https://github.com/wfnuser/cowiki/issues/31)). Content varies by source type.

### Identity & Dedup

- **Identity hash** = `SHA256(source_type + source_identity)` where identity is URL/filename/repo name
- **Ingest dedup**: reject/skip if identity hash already exists in the space
- **Compile semantic dedup**: agent compares source content against existing wiki pages, skips redundant material
- `.cowiki/state.json` tracks identity hashes and source→page mappings

### Minimum Standard: `manifest.json`

Every source directory must contain a `manifest.json`:

```json
{
  "type": "pdf",
  "source_url": "https://arxiv.org/abs/1706.03762",
  "title": "Attention Is All You Need",
  "extracted_at": "2026-06-07T10:00:00Z",
  "files": ["content.md", "metadata.json", "headings.json", "attachments/"]
}
```

### Example Structures

```
sources/
  sha256_abc123/              ← identity hash directory
    manifest.json
    content.md                ← always present
    metadata.json             ← author, date, domain
    headings.json             ← document structure
    attachments/              ← figures, code, PDF pages

  sha256_def456/              ← GitHub repo
    manifest.json
    README.md
    issues.json
    code/                     ← key source files

  sha256_ghi789/              ← YouTube video
    manifest.json
    transcript.md
    metadata.json             ← title, channel, duration
```

Agent explores with `ls`/`grep`/`read` and handles any structure. No fixed schema beyond `manifest.json`.

## Concurrency Model

### ShallowCompile — Parallel

- Multiple requests run concurrently
- Each gets an agent from the pool (queues if busy, 503 if full)
- No space-level lock needed

### DeepIntegrate — Per-Space Mutex

- One run per space at a time (`tokio::sync::Mutex<()>` keyed by space)
- Contention: 409 Conflict
- Inside mutex: **two sequential agent calls** (serial avoids page_path drift)
  1. **Wiki Reorganization** — restructure directories, rename/move pages
  2. **Knowledge Graph** — read final pages, extract entities + facts

### Agent Pool

| Space Type | Pool Size |
|-----------|-----------|
| Personal Space | N=1 (fixed) |
| Team Space | N (dynamic, tier-gated, frontend configurable) |

DeepIntegrate always uses 1 agent (serial within space).

## Observability — SSE

```
GET /api/compile/:id/events  →  SSE stream

event: phase       {phase: "shallow"|"deep", status: "started"|"completed"}
event: agent-start {agent_id, harness_type}
event: llm-round   {round, token_count}
event: tool-call   {tool, args}
event: tool-result {tool, success, summary}
event: entity      {entity, type, confidence}
event: fact        {subject, predicate, object}
event: error       {message, recoverable}
```

## ShallowCompile: Source → Wiki Pages (Sync)

### Pipeline

1. **Identity Check** — lookup identity hash in `.cowiki/state.json`; skip if compiled
2. **List Sources** — enumerate source directories in `sources/`
3. **Acquire Agent** — from space pool
4. **Agent Execution** — explore source dirs, create wiki pages
5. **Persist** — pages written to git by agent
6. **pgvector** — Personal Space: insert embeddings immediately. Team Space: defer to review approve.
7. **Return** — page summary to user
8. **Fire DeepIntegrate** — Personal Space: immediately. Team Space: set trigger on approve hook.

### Agent Tools

**Source FS Access (read-only, source directories):**

| Tool | Parameters | Description |
|------|-----------|-------------|
| `ls` | `path?` | List source directory contents |
| `grep` | `pattern, path?` | Search text within source directory |
| `read` | `path` | Read a file from the source directory |

**Wiki FS Manipulate (create/delete only — no editing existing pages):**

| Tool | Parameters | Description |
|------|-----------|-------------|
| `create_wiki` | `path, content` | Create a new wiki page |
| `rm_wiki` | `path` | Delete a wiki page or empty directory |

Agent explores source dirs, understands content, creates wiki pages with self-determined directory structure. No editing, moving, or reading wiki pages — ShallowCompile starts fresh.

### Error Handling

| Error | Response |
|-------|----------|
| LLM timeout | 500; no partial writes; identity check protects retry |
| Invalid output | Agent self-corrects via tool feedback |
| Git write fail | Error to agent; retry or report |
| Embedder fail | Page written; periodic backfill |
| Pool exhausted | Queue; 503 if full |

## DeepIntegrate: Wiki Reorg + Knowledge Graph (Async)

### Trigger

| Space | Trigger |
|-------|---------|
| Personal Space | Auto after ShallowCompile |
| Team Space | Review approve hook |

### Pipeline

1. **Acquire Mutex** — per-space lock; 409 if running
2. **(Team Space only) Insert into pgvector** — generate embeddings for all pending wiki pages
3. **Agent Call 1: Wiki Reorganization** — restructure directory layout
4. **Agent Call 2: Knowledge Graph** — extract entities + facts, build graph
5. **Release Mutex**
6. **Periodic Rebuild** — `POST /api/compile/graph/rebuild`

### Why Two Sequential Calls?

Wiki reorg changes `page_path`. If graph extraction ran concurrently, facts would record stale paths. Serial guarantees stable paths.

### Agent Tools

**Call 1: Wiki Reorganization (full wiki FS tools):**

| Tool | Parameters | Description |
|------|-----------|-------------|
| `ls_wiki` | `path?` | List wiki directory |
| `read_wiki` | `path` | Read wiki page content |
| `mkdir_wiki` | `path` | Create subdirectory |
| `mv_wiki` | `old_path, new_path` | Move/rename page or directory |
| `edit_wiki` | `path, content` | Edit existing page |
| `rm_wiki` | `path` | Delete page or empty directory |

**Call 2: Knowledge Graph (read wiki + graph tools):**

| Tool | Parameters | Description |
|------|-----------|-------------|
| `read_wiki` | `path` | Read wiki page content |
| `query_entity` | `name` | Look up entity by name |
| `find_similar` | `embedding, threshold?` | Similar entity search |
| `upsert_entity` | `name, type, aliases?, description?` | Create/update entity |
| `upsert_fact` | `subject, predicate, object, confidence?` | Create/update fact triple |
| `link_page_entity` | `page_path, entity_name, relevance?` | Link page to entity |
| `list_entities` | `page_path?` | List entities |

**Note:** DeepIntegrate has NO `create_wiki`. All pages from ShallowCompile. Agent reorganizes and links only.

### Fact Model

Triples: `(subject-entity, predicate, object-entity-or-literal)` with provenance:

```json
{
  "subject": "Docker",
  "predicate": "is-a",
  "object": "container-runtime",
  "page_path": "infra/docker-networking.md",
  "source_name": "sha256_abc123",
  "confidence": 0.95
}
```

### Execution Modes

| Mode | Trigger | Scope |
|------|---------|-------|
| Incremental | Personal: after ShallowCompile. Team: after review approve. | All pages (deep) |
| Periodic | Cron / `POST /api/compile/graph/rebuild`. Daily off-peak. | All pages (reconciliation) |

## Data Model

```sql
CREATE TABLE entities (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name        TEXT NOT NULL,
    type        TEXT NOT NULL,
    aliases     TEXT[] DEFAULT '{}',
    description TEXT,
    embedding   vector(1536),
    space       TEXT NOT NULL,
    is_orphaned BOOLEAN DEFAULT false,
    created_at  TIMESTAMPTZ DEFAULT now(),
    updated_at  TIMESTAMPTZ DEFAULT now()
);

CREATE TABLE facts (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    subject_id  UUID NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    predicate   TEXT NOT NULL,
    object_id   UUID REFERENCES entities(id) ON DELETE CASCADE,
    object_lit  TEXT,
    page_path   TEXT NOT NULL,
    source_name TEXT,
    confidence  REAL DEFAULT 1.0,
    space       TEXT NOT NULL,
    created_at  TIMESTAMPTZ DEFAULT now()
);

CREATE TABLE page_entities (
    page_path   TEXT NOT NULL,
    entity_id   UUID NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    relevance   REAL DEFAULT 1.0,
    space       TEXT NOT NULL,
    PRIMARY KEY (page_path, entity_id)
);
```

### GraphStore Trait

```rust
pub trait GraphStore {
    async fn query_entity(&self, name: &str, space: &str) -> Result<Option<Entity>>;
    async fn find_similar(&self, embedding: &[f32], threshold: f32, space: &str) -> Result<Vec<Entity>>;
    async fn upsert_entity(&self, entity: &Entity) -> Result<Entity>;
    async fn upsert_fact(&self, fact: &Fact) -> Result<Fact>;
    async fn link_page_entity(&self, page_path: &str, entity_id: Uuid, relevance: f32, space: &str) -> Result<()>;
    async fn list_entities(&self, page_path: Option<&str>, space: &str) -> Result<Vec<Entity>>;
    async fn delete_by_page(&self, page_path: &str, space: &str) -> Result<()>;
    async fn find_orphan_entities(&self, space: &str) -> Result<Vec<Entity>>;
}
```

MVP: `PgGraphStore`. Future: `Neo4jGraphStore`.

## Page Deletion

Sequential three-store cleanup:

1. **Git** — `rm_wiki(path)` → git rm + commit
2. **Vector DB** — `DELETE FROM pages WHERE page_path=$1`
3. **Knowledge Graph** — `DELETE FROM page_entities` → `DELETE FROM facts` → mark orphans
4. **Compile State** — update `.cowiki/state.json`

## Agent Communication Layer

### Protocol

```json
{
  "task_type": "shallow_compile | wiki_reorganize | build_graph",
  "system_prompt": "...",
  "user_input": "...",
  "workspace_path": "/data/team-space/",
  "tools": [...],
  "config": { "max_rounds": 20, "token_budget": 100000 }
}
```

### Harness Matrix

| Harness | Task Type | Backend | For |
|---------|-----------|---------|-----|
| `compile-simple` | shallow_compile | Python deepagents | Source → wiki pages |
| `compile-complex` | shallow_compile | deepagents full | Complex sources (future) |
| `wiki-reorganize` | wiki_reorganize | deepagents | Directory restructuring |
| `entity-extract` | build_graph | Rust agnt (HTTP) | Entity + fact extraction |
| `review` | review_submission | Swink Agent | Review (future) |

All agents are separate HTTP/gRPC processes — no embedded mode.

### Agent Pool

Per-space, frontend-configurable, tier-gated:

| Tier | Personal | Team |
|------|----------|------|
| Free | N=1 | N=1 |
| Pro | N=1 | N=2–4 |
| Enterprise | N=1 | N=2–16 |

## State Persistence

`.cowiki/state.json` in git:

```json
{
  "sources": {
    "sha256_abc123": {
      "identity_hash": "sha256_abc123",
      "source_type": "url",
      "source_identity": "https://example.com/article"
    }
  },
  "source_pages": {
    "sha256_abc123": ["infra/docker-networking.md", "infra/container-basics.md"]
  }
}
```

## API Surface

| Endpoint | Method | Description |
|----------|--------|-------------|
| `POST /api/compile` | POST | ShallowCompile: sources → wiki pages (sync) |
| `GET /api/compile/:id/events` | GET | SSE agent event stream |
| `GET /api/pages/:path/graph` | GET | Entities + facts for a page |
| `GET /api/entities/:id` | GET | Entity detail + related pages |
| `POST /api/graph/search` | POST | Search entities + facts |
| `POST /api/compile/graph/rebuild` | POST | Full graph rebuild |

## Testing Strategy

| Layer | What | How |
|-------|------|-----|
| Source dir | manifest parsing, identity hash | Unit tests |
| Protocol | Serialization, event types | Unit tests |
| ShallowCompile | Agent pool, source FS + wiki tools | Integration (test git) |
| DeepIntegrate C1 | Wiki reorg tools | Integration |
| DeepIntegrate C2 | Graph tools, entity resolution | Integration (test DB) |
| SSE events | Stream ordering | Integration |
| GraphStore | CRUD, FK cascade, orphans | Integration (test PG) |
| Deletion | Three-store cascade | Integration |
| pgvector timing | Team: deferred; Personal: immediate | Integration |
| Concurrency | Shallow parallel, Deep mutex | Stress tests |

## Related

- Issue [#15](https://github.com/wfnuser/cowiki/issues/15) — Compile system redesign
- Issue [#14](https://github.com/wfnuser/cowiki/issues/14) — HTML view layer
- Issue [#31](https://github.com/wfnuser/cowiki/issues/31) — Multi-format source ingestion
- Issue [#44](https://github.com/wfnuser/cowiki/issues/44) — Branch-aware search and merge
