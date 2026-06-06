# cowiki Compile System Design

> Status: revised v2 | Date: 2026-06-06 | Issue: [#15](https://github.com/wfnuser/cowiki/issues/15)

## Overview

A **two-phase compile pipeline** with a **decoupled agent communication layer**.

- **ShallowCompile (sync)**: Shallow wiki knowledge extraction — agent reads sources, organizes them into wiki pages with a self-determined directory structure. Returns immediately.
- **DeepIntegrate (async)**: Deep knowledge integration — agent extracts entities and fact triples from wiki pages, builds a knowledge graph. Trigger depends on space type.

Agents are existing frameworks (deepagents, agnt, Swink Agent) running as separate processes, connected via a shared protocol. The compiler orchestrates; agents execute. All agent actions are streamed to the frontend via SSE for full observability.

## Architecture

### Two-Phase Trigger Model

| Space | ShallowCompile Trigger | DeepIntegrate Trigger |
|-------|----------------|-----------------|
| **Personal Space** | `POST /api/compile` | Automatic, immediately after ShallowCompile |
| **Team Space** | `POST /api/compile` | After review approves the submission |

**Rationale:** ShallowCompile produces shallow wiki pages suitable for human review. DeepIntegrate performs deep knowledge integration (entity extraction, fact triples, cross-page linking) that should only happen after the pages have been reviewed and approved.

### System Layers

```
POST /api/compile
        │
┌───────▼──────────────────────────────────────────────────────┐
│  crates/core/compiler/          (business logic)              │
│                                                               │
│  ShallowCompile        DeepIntegrate: GraphBuilder         │
│  shallow wiki extraction        deep knowledge integration    │
│  (sync, returns immediately)    (async, per-space mutex)      │
│                                                               │
│  Agent Pool (per-space)                                       │
│  Personal Space: 1 agent        Team Space: N agents (config) │
└───────┬──────────────────────────────────────────────────────┘
        │ AgentRequest (protocol) + SSE event stream
┌───────▼──────────────────────────────────────────────────────┐
│  crates/core/src/client/        (server-internal API client)   │
│                                                               │
│  ApiClient → pages, compile, ingest, search, graph            │
│  Used by: compiler, agents (server-side components)           │
└──────────────────────────────────────────────────────────────┘
        │
┌───────▼──────────────────────────────────────────────────────┐
│  crates/agents/                 (communication layer)          │
│                                                               │
│  protocol.rs  ← AgentRequest / AgentResponse / Event types    │
│  client.rs    ← AgentClient: HTTP/gRPC client                 │
│  registry.rs  ← HarnessRegistry                               │
│  dispatch.rs  ← DispatchPolicy                                │
│  events.rs    ← SSE event stream types                        │
└───────┬──────────────────────────────────────────────────────┘
        │ HTTP / gRPC (agents are separate processes)
┌───────┬──────────────────────────────────────────────────────┐
│       ▼                ▼                ▼                     │
│  deepagents (HTTP)  agnt (HTTP bin)  Swink Agent (gRPC)       │
│  compile-simple     entity-extract   review (future)           │
│  compile-complex                                               │
│                                                                │
│  ── all agents are separate processes, remote-deployable ──    │
└──────────────────────────────────────────────────────────────┘

  cli/                    cowiki-mcp-server/
  (independent, HTTP)     (independent, HTTP)
  no crates dependency    no crates dependency
```

### Module Boundaries

| Crate | Owns | Depends On |
|-------|------|------------|
| `crates/core/src/client/` | ApiClient: pages, compile, ingest, search, graph HTTP calls (server-internal) | reqwest, core/models |
| `crates/core/src/compiler/` | Phase1::compile(), Phase2::build_graph(), SourceDispatch, AgentPool, EntityResolver | agents, core/models, core/client, db |
| `crates/agents/` | protocol, AgentClient (HTTP/gRPC only), HarnessRegistry, DispatchPolicy, SSE events | core/client |
| `crates/db/` | pages, entities, facts, page_entities tables | sqlx, pgvector |
| `cli/` | CLI tool (independent module, own HTTP client) | reqwest, clap — no crates dependency |
| `cowiki-mcp-server/` | MCP server (independent module, own HTTP client) | reqwest, rmcp — no crates dependency |

## Concurrency Model

### ShallowCompile — Parallel

- Multiple compile requests can run concurrently (different users, different sources)
- Each request gets an agent from the pool (or queues if all busy)
- No space-level lock needed — sources are independent

### DeepIntegrate — Per-Space Mutex

- Only one graph build per space at a time (`tokio::sync::Mutex<()>` keyed by space)
- Different spaces can run DeepIntegrate concurrently (spaces are isolated)
- If a build is already running for a space, new requests are queued or rejected with 409

### Agent Pool

| Space Type | Pool Size | Behavior |
|-----------|-----------|----------|
| Personal Space | N=1 (fixed) | Single agent, processes one task at a time |
| Team Space | N (dynamic, configurable) | Concurrent agents for ShallowCompile; DeepIntegrate uses 1 |

Team Space agent count is configurable via space settings. Enterprise tier can scale N up. DeepIntegrate always uses exactly 1 agent (graph build is serial within a space).

## Observability — SSE Event Stream

All agent actions are pushed to the frontend via **Server-Sent Events**:

```
GET /api/compile/:id/events  →  SSE stream

Event types:
  event: phase       {phase: "shallow", status: "started"}
  event: agent-start {agent_id, harness_type}
  event: llm-round   {round: 1, prompt_preview: "...", token_count: 5000}
  event: tool-call   {tool: "create_wiki", args: {path: "infra/docker-networking.md"}}
  event: tool-result {tool: "create_wiki", success: true, summary: "created"}
  event: entity      {entity: "Docker", type: "Technology", confidence: 0.95}
  event: fact        {subject: "Docker", predicate: "is-a", object: "container-runtime"}
  event: phase       {phase: "shallow", status: "completed", pages_count: 5}
  event: error       {message: "...", recoverable: true}
```

Frontend renders this as a live log, similar to CI/CD build output, with expandable tool call details and full LLM conversation visibility.

## ShallowCompile: Source → Wiki Pages (Sync, Shallow)

### Pipeline

1. **Hash Check** — SHA-256 each source vs `.cowiki/state.json`; skip unchanged
2. **Source Dispatch** — route by source type:
   - `.md` / `.txt` → passthrough
   - URL → `crates/extractor` (existing)
   - PDF, code repo, video → future harnesses
3. **Acquire Agent** — get an agent from the space's pool (queue if none available)
4. **Agent Execution** — agent runs independently, calling wiki tools to organize pages
5. **Persist & Return** — pages exist in git (written by agent); return summary to user
6. **Fire DeepIntegrate** — Personal Space: immediately. Team Space: fire event for post-review trigger.

### Wiki Tools (Agent-Accessible)

The agent operates on the wiki via these tools. All tools work within the space's git branch — the agent does not use raw filesystem syscalls.

| Tool | Parameters | Description |
|------|-----------|-------------|
| `ls_wiki` | `path?` | List wiki directory contents |
| `mkdir_wiki` | `path` | Create a wiki subdirectory |
| `create_wiki` | `path, content` | Create a new wiki page at the given path |
| `edit_wiki` | `path, content` | Edit an existing wiki page |
| `read_wiki` | `path` | Read a wiki page's content |
| `rm_wiki` | `path` | Delete a wiki page or empty directory |

**Agent autonomy:** The agent decides its own directory structure and file naming. The compiler does not impose a fixed `wiki/{slug}.md` convention. Agents can create nested directories, use semantic file names, and organize pages around themes.

All wiki tool implementations call the shared `crates/core/src/client/` module, which goes through the HTTP API to the server, which performs git operations via libgit2.

### Error Handling

| Error | Response |
|-------|----------|
| LLM timeout | Return 500; no partial writes; hash check prevents re-processing on retry |
| Invalid LLM output | Agent self-corrects via tool feedback; max retries configurable |
| Git write failure | Return error to agent; agent can retry or report failure |
| Embedder failure | Page written to git without embedding; periodic backfill catches up |
| Agent pool exhausted | Queue the request; return 503 if queue full |

## DeepIntegrate: Knowledge Graph (Async, Deep)

### Trigger

| Space | Trigger |
|-------|---------|
| Personal Space | Auto-fired after ShallowCompile completes |
| Team Space | Fired when a submission is **approved** (review → approve hook) |

### Pipeline

1. **Acquire Mutex** — per-space lock; 409 Conflict if already running
2. **Load Pages** — read all wiki pages in the space from git (full pass)
3. **Agent Execution** — agent reads pages, autonomously extracts entities and facts
4. **Store Graph** — agent calls graph tools to persist entities, facts, and page-entity links
5. **Release Mutex**
6. **Periodic Rebuild** — `POST /api/compile/graph/rebuild` triggers full reconciliation for consistency

### Graph Tools (Agent-Accessible)

The agent autonomously decides what entities and facts to extract. It only needs graph read/write tools — no extraction-specific tools.

| Tool | Parameters | Description |
|------|-----------|-------------|
| `query_entity` | `name, space?` | Look up an entity by name (for disambiguation) |
| `find_similar` | `embedding, threshold?` | Find similar entities by embedding similarity |
| `upsert_entity` | `name, type, aliases?, description?` | Create or update an entity node |
| `upsert_fact` | `subject, predicate, object, confidence?` | Create or update a fact triple |
| `link_page_entity` | `page_path, entity_name, relevance?` | Associate a wiki page with an entity |
| `list_entities` | `page_path?` | List entities, optionally filtered by page |

### Fact Model

Facts are triples: `(subject-entity, predicate, object-entity-or-literal)` with provenance:

```json
{
  "subject": "Docker",
  "predicate": "is-a",
  "object": "container-runtime",
  "page_path": "infra/docker-networking.md",
  "source_name": "article-1.md",
  "confidence": 0.95
}
```

### Execution Modes

| Mode | Trigger | Scope |
|------|---------|-------|
| Incremental | Personal Space: auto after ShallowCompile. Team Space: auto after review approve. | All pages in space (deep pass) |
| Periodic full | Manual or cron (`POST /api/compile/graph/rebuild`). Default: daily off-peak. Configurable. | All pages in space (reconciliation) |

## Data Model

### New PostgreSQL Tables

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

### Entity vs Page-Entity

- **`entities`** — knowledge graph **nodes** themselves (e.g., "Docker", "Kubernetes"). Exist independently, with name, type, aliases, embedding. One entity can appear across many pages.
- **`page_entities`** — **many-to-many junction** between pages and entities. Records "page X mentions entity Y with relevance Z". Enables bidirectional traversal: given a page → its entities; given an entity → pages mentioning it.

```
entities          page_entities         wiki pages (git)
────────          ─────────────         ───────────────
Docker  ◄────────► infra/docker.md
  │      mentions
  ├──────◄────────► infra/k8s.md
  │
K8s    ◄──────────► infra/k8s.md
```

## Page Deletion

Deletion cleans all three stores sequentially:

1. **Git** — agent called `rm_wiki(path)` → git rm + commit
2. **Vector DB** — `DELETE FROM pages WHERE page_path=$1`
3. **Knowledge Graph** — `DELETE FROM page_entities` → `DELETE FROM facts` → mark orphan entities
4. **Compile State** — update `.cowiki/state.json` if source→page mapping affected

**Consistency strategy:** Git is the source of truth. PG tables are derived views — always rebuildable from git content. Periodic graph rebuild detects and repairs stale data. Orphan entities: 7-day grace before automatic purge.

## Agent Communication Layer

### Protocol

```json
// POST /agent/run
{
  "task_type": "compile_page | build_graph | review_submission",
  "system_prompt": "...",
  "user_input": "...",
  "workspace_path": "/data/team-space/wiki/",
  "tools": [
    { "name": "create_wiki", "description": "...", "parameters": { ... } }
  ],
  "config": {
    "max_rounds": 20,
    "token_budget": 100000,
    "model": "claude-sonnet-4-6"
  }
}

// Response
{
  "success": true,
  "output": { ... },
  "usage": { "input_tokens": 5000, "output_tokens": 1200 },
  "rounds": 5
}
```

### SSE Event Stream

Events flow from agent → compiler → SSE → frontend:

```jsonl
{"type":"phase","phase":"shallow","status":"started","timestamp":"..."}
{"type":"agent-start","agent_id":"agent-3","harness_type":"compile-simple"}
{"type":"llm-round","round":1,"prompt_preview":"...","token_count":5000}
{"type":"tool-call","tool":"create_wiki","args":{"path":"infra/docker.md"},"round":1}
{"type":"tool-result","tool":"create_wiki","success":true,"summary":"created page"}
{"type":"llm-round","round":2,"prompt_preview":"...","token_count":3000}
{"type":"tool-call","tool":"create_wiki","args":{"path":"infra/k8s.md"},"round":2}
{"type":"tool-result","tool":"create_wiki","success":true,"summary":"created page"}
{"type":"phase","phase":"shallow","status":"completed","pages_count":2}
```

### Harness Matrix

| Harness | Task Type | Backend | Used For |
|---------|-----------|---------|----------|
| `compile-simple` | compile_page | Python deepagents (HTTP) | Text/markdown sources (MVP) |
| `compile-complex` | compile_page | deepagents full (HTTP) | PDF, code repos, videos (future) |
| `entity-extract` | build_graph | Rust agnt (HTTP standalone binary) | DeepIntegrate: entity + fact extraction |
| `review` | review_submission | Swink Agent / deepagents (HTTP) | Review operations (future) |

### AgentClient

**All agents are separate processes**, deployed remotely and reached exclusively via HTTP or gRPC. No embedded / in-process agent mode — this keeps process management clean, enables horizontal scaling across machines, and supports heterogeneous language runtimes (Python, Rust, etc.).

| Transport | Use Case |
|-----------|----------|
| HTTP (REST) | Python deepagents, Rust agnt standalone binary |
| gRPC | Swink Agent (RPC daemon), high-throughput scenarios |

### Agent Pool Configuration

Pool size is per-space dynamically configurable. Frontend exposes the setting; the server enforces tier limits. Team Space pool size scales with paid tier.

```json
// Team Space settings — configurable via frontend
{
  "agent_pool": {
    "compile_page": {
      "size": 4,
      "harness": "compile-simple"
    },
    "build_graph": {
      "size": 1,
      "harness": "entity-extract"
    }
  }
}
```

**Tier gating:**

| Tier | Personal Space | Team Space |
|------|---------------|------------|
| Free | N=1 (fixed) | N=1 (fixed) |
| Pro | N=1 (fixed) | N=2–4 (configurable) |
| Enterprise | N=1 (fixed) | N=2–16 (configurable) |

Frontend queries tier limits from the server; pool size controls are disabled above the tier max.

## Shared API Client

Located at `crates/core/src/client/` — all HTTP calls to the wiki server live here:

```
crates/core/src/client/
├── mod.rs        ← ApiClient struct (base URL, auth token)
├── pages.rs      ← create, edit, read, ls, mkdir, rm wiki pages
├── compile.rs    ← compile, graph/rebuild
├── ingest.rs     ← ingest sources
├── search.rs     ← semantic search
├── submit.rs     ← submit, review
└── graph.rs      ← query_entity, list_entities, ...
```

**Consumers (server-side):**
- `crates/core/src/compiler/` — ShallowCompile and DeepIntegrate orchestration
- `crates/agents/` — agent tool implementations call ApiClient
- Agent harness tools — wiki ops and graph ops wrap ApiClient calls

**Independent modules:**
- `cli/` — standalone CLI tool with its own thin HTTP client. Does not depend on crates/.
- `cowiki-mcp-server/` — standalone MCP server with its own thin HTTP client. Does not depend on crates/.

These modules are intentionally decoupled — they communicate with the wiki server purely via HTTP and can be implemented in any language.

## State Persistence

`.cowiki/state.json` lives in the git repo alongside sources and wiki pages:

```json
{
  "sources": {
    "article-1.md": "abc123hash...",
    "notes.txt":     "def456hash..."
  },
  "source_pages": {
    "article-1.md": ["infra/docker-networking.md", "infra/container-basics.md"],
    "notes.txt":     ["guides/deployment-tips.md"]
  }
}
```

Tracks: (1) source hashes for skip detection, (2) source→page mapping for cascade-on-delete.
Note: page paths are now agent-determined, not system-generated slugs.

## API Surface

| Endpoint | Method | Description |
|----------|--------|-------------|
| `POST /api/compile` | POST | ShallowCompile: compile sources → wiki pages (sync). Fires DeepIntegrate depending on space. |
| `GET /api/compile/:id/events` | GET | SSE stream: agent actions, LLM rounds, tool calls, results |
| `GET /api/pages/:path/graph` | GET | Get entities + facts for a specific wiki page |
| `GET /api/entities/:id` | GET | Entity detail + related entities + pages mentioning it |
| `POST /api/graph/search` | POST | Search entities + facts (future: graph traversal) |
| `POST /api/compile/graph/rebuild` | POST | Trigger full graph rebuild for a space (periodic reconciliation) |

## Testing Strategy

| Layer | What | How |
|-------|------|-----|
| client | ApiClient request/response round-trips | Unit tests with mock HTTP server |
| protocol | Serialization, schema validation, event types | Unit tests |
| ShallowCompile | Hash check, dispatch, agent pool, wiki tool calls | Integration tests with test git repo |
| DeepIntegrate | Graph tools, entity resolution, mutex contention | Integration tests with test DB |
| SSE events | Event stream correctness, ordering | Integration tests |
| GraphStore | CRUD, FK cascade, orphan detection | Integration tests with test PG |
| Deletion | Three-store cleanup cascade | Integration tests |
| Concurrency | ShallowCompile parallel, DeepIntegrate mutex per-space | Stress tests |

## What This Design Does NOT Include

- Auto-scaling agent pools (MVP: fixed N, manually configurable)
- Multi-hop graph traversal queries (deferred to Neo4j migration)
- Real-time graph visualization API (deferred to issue #14)
- Cross-space entity merging
- Fact confidence scoring beyond LLM-reported values
- Agent harness implementations (deepagents, agnt, etc.) — these are existing projects

## Related

- Issue [#15](https://github.com/wfnuser/cowiki/issues/15) — Compile system redesign
- Issue [#14](https://github.com/wfnuser/cowiki/issues/14) — HTML view layer (future consumer of graph data)
- `crates/core/src/compiler.rs` — existing single-phase compiler
- `crates/server/src/routes/compile.rs` — existing compile route
- `docs/spec.md` — overall cowiki MVP spec
