# cowiki Compile System Design

> Status: v4 | Date: 2026-06-07 | Issue: [#15](https://github.com/wfnuser/cowiki/issues/15)

## Overview

A **two-stage compile pipeline** with **wiki-page-centric knowledge architecture** and **decoupled agent communication**.

- **ShallowCompile (sync)**: Agent explores source directories, produces wiki pages + entity pages + concept pages in markdown. Deduplicates and merges against existing content. Inserts metadata into PSQL indices.
- **Lint (async, human-triggered)**: Agent health-checks the wiki — detects contradictions, duplicates, orphan nodes, broken wikilinks. Personal Space: manual trigger. Team Space: post-review-approve hook.

**Core principle:** Wiki pages are the center. Entities are navigation bridges between wiki pages. Entities and concepts live as markdown files (FS source of truth) with PSQL metadata tables for search and dedup.

## Data Architecture

### The Navigation Model

```
wiki/infra/docker-networking.md ──[[docker]]──→ entities/docker.md
       ↑                                          │  [[kubernetes]]
       │                                          ↓
wiki/guides/deployment.md ←── [[docker]] ── entities/kubernetes.md
       │                                                 │
       │ [[containerization]]                            │ [[containerization]]
       ↓                                                 ↓
concepts/containerization.md ←────────────────────────────┘
```

- **Wiki pages** are the center — each page is a self-contained knowledge unit
- **Entities** are bridges — named things (technologies, people, events) that appear across wiki pages
- **Concepts** are themes — abstract topics that span multiple pages and entities
- **Wikilinks** (`[[name]]`) connect everything — wiki→entity, entity→entity, wiki→concept
- **Backlinks** — each entity page has `## Mentioned In` auto-updated by agent

### FS Directory Structure

```
workspace/
  wiki/                         ← wiki pages (source of truth)
    infra/
      docker-networking.md
      kubernetes-architecture.md
    guides/
      deployment.md
  entities/                     ← named entities
    docker.md
    kubernetes.md
  concepts/                     ← abstract themes
    containerization.md
    microservices.md
  queries/                      ← agent-digested Q&A (reserved)
  sources/                      ← raw ingested sources
    sha256_abc123/
      manifest.json
      content.md
  .cowiki/state.json
```

### PSQL Tables (Metadata Indices)

FS is source of truth. PSQL tables are derived indices for search, dedup, and fast lookup. Each can be dropped and rebuilt from FS content.

**wiki_pages** — one row per wiki markdown file:

```sql
CREATE TABLE wiki_pages (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    path        TEXT NOT NULL,              -- relative FS path
    title       TEXT NOT NULL,
    summary     TEXT,
    embedding   vector(1536),
    space       TEXT NOT NULL,
    branch      TEXT NOT NULL,
    created_at  TIMESTAMPTZ DEFAULT now(),
    updated_at  TIMESTAMPTZ DEFAULT now(),
    UNIQUE (path, space, branch)
);
```

**entities** — one row per entity markdown file:

```sql
CREATE TABLE entities (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    path        TEXT NOT NULL,              -- relative FS path, e.g. entities/docker.md
    name        TEXT NOT NULL,              -- canonical label
    type        TEXT NOT NULL,              -- Technology, Person, Event, Organization
    aliases     TEXT[] DEFAULT '{}',
    summary     TEXT,
    embedding   vector(1536),
    space       TEXT NOT NULL,
    created_at  TIMESTAMPTZ DEFAULT now(),
    updated_at  TIMESTAMPTZ DEFAULT now(),
    UNIQUE (path, space)
);
```

**concepts** — one row per concept markdown file:

```sql
CREATE TABLE concepts (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    path        TEXT NOT NULL,              -- relative FS path, e.g. concepts/containerization.md
    name        TEXT NOT NULL,              -- canonical label
    aliases     TEXT[] DEFAULT '{}',
    summary     TEXT,
    embedding   vector(1536),
    space       TEXT NOT NULL,
    created_at  TIMESTAMPTZ DEFAULT now(),
    updated_at  TIMESTAMPTZ DEFAULT now(),
    UNIQUE (path, space)
);
```

All page types (wiki, entity, concept) use YAML frontmatter to store metadata inline in the markdown file. The PSQL tables index a subset of frontmatter fields for search and dedup. Full frontmatter stays in FS markdown — PSQL is a fast-lookup cache, not the canonical record.

**Entity frontmatter example** (`entities/docker.md`):
```yaml
---
title: "Docker"
type: entity
aliases: [docker-engine, Docker CE]
created: 2026-01-15
tags: [infrastructure, containers]
---
```

**Concept frontmatter example** (`concepts/containerization.md`):
```yaml
---
title: "Containerization"
type: concept
aliases: [container technology]
created: 2026-01-15
tags: [infrastructure, architecture]
---
```

## ShallowCompile: Source → Wiki + Entities + Concepts (Sync)

### Pipeline

1. **Identity Check** — lookup identity hash in `.cowiki/state.json`; skip if compiled
2. **List Sources** — enumerate source dirs in `sources/`
3. **Acquire Agent** — from space pool
4. **Agent Execution** — agent:
   - Reads source dirs (source FS tools: `ls`, `grep`, `read`)
   - **Deduplicates** against existing wiki/entities/concepts (reads via `read_wiki`)
   - Creates/updates wiki pages (`create_wiki`, `edit_wiki`)
   - Creates/updates entity pages (`create_entity`, `edit_entity`)
   - Creates/updates concept pages (`create_concept`, `edit_concept`)
   - Updates backlinks in entity pages (`## Mentioned In`)
   - Deletes redundant pages (`rm_wiki`)
5. **Persist** — pages written to git by agent
6. **PSQL sync** — upsert wiki_pages, entities, concepts rows with embeddings
7. **pgvector** — Personal Space: immediately. Team Space: defer to review approve.
8. **Return** — page summary + SSE events to frontend

### Agent Tools — ShallowCompile

**Source FS Access (read-only):**

| Tool | Description |
|------|-------------|
| `ls` | List source directory |
| `grep` | Search within source directory |
| `read` | Read source file |

**Wiki FS Manipulate:**

| Tool | Description |
|------|-------------|
| `create_wiki` | Create new wiki page |
| `edit_wiki` | Edit existing wiki page |
| `read_wiki` | Read wiki page (for dedup) |
| `rm_wiki` | Delete wiki page |

**Entity FS Manipulate:**

| Tool | Description |
|------|-------------|
| `create_entity` | Create entity markdown file |
| `edit_entity` | Edit entity markdown (incl. backlinks) |
| `read_entity` | Read entity for dedup/merge |
| `rm_entity` | Delete entity |

**Concept FS Manipulate:**

| Tool | Description |
|------|-------------|
| `create_concept` | Create concept markdown file |
| `edit_concept` | Edit concept markdown |
| `read_concept` | Read concept for dedup |
| `rm_concept` | Delete concept |

### Dedup & Merge Strategy

During ShallowCompile, the agent:

1. **Entity dedup** — before creating a new entity, reads existing entities. If similar (by embedding or name), merges: adds new aliases, updates summary, appends new wiki page to `## Mentioned In`
2. **Concept dedup** — same pattern for concepts
3. **Wiki page dedup** — checks if source content is already covered by an existing wiki page. If redundant, skips creation
4. **Cross-space scope** — dedup scope is per-space. Personal Space entities don't conflict with Team Space entities

### pgvector Timing

| Space | pgvector Insert |
|-------|----------------|
| Personal Space | Immediately after ShallowCompile |
| Team Space | On review approve |

## Lint: Wiki Health Check (Async, Human-Triggered)

Replaces the previous "DeepIntegrate" concept. No more knowledge graph extraction — entities and concepts are already created during ShallowCompile. Lint verifies quality.

### Trigger

- **Personal Space**: Manually triggered by user (`POST /api/lint`)
- **Team Space**: Post-review-approve hook (after pgvector insert)
- No automatic cron in current version

### What Lint Checks

| Issue | Detection | Agent Action |
|-------|-----------|-------------|
| **Contradiction** | Two wiki pages make conflicting claims | Flag for human review |
| **Duplicate** | Two pages/entities cover the same topic with high overlap | Suggest merge |
| **Orphan** | Entity with zero `## Mentioned In` backlinks | Flag for cleanup |
| **Broken link** | `[[wikilink]]` points to non-existent page | Fix or remove |
| **Missing backlink** | Wiki page mentions entity but entity doesn't link back | Auto-fix |
| **Stale content** | Page hasn't been updated after newer sources ingested | Flag |

### Agent Tools — Lint

| Tool | Description |
|------|-------------|
| `read_wiki` | Read wiki page |
| `read_entity` | Read entity page |
| `read_concept` | Read concept page |
| `ls_wiki` | List wiki directory |
| `ls_entities` | List entities directory |
| `ls_concepts` | List concepts directory |
| `edit_wiki` | Fix broken links in wiki page |
| `edit_entity` | Fix backlinks in entity |
| `rm_wiki` | Remove orphan page |

### Output

Lint produces `ReviewItem` entries (contradiction, duplicate, orphan, etc.) streamed via SSE. Human reviews and resolves each item — accept fix, dismiss, or manually address.

## Review: Two-Stage

### Stage 1: Shallow Review

- After ShallowCompile, wiki pages are listed for review
- **Independent per-page** — each page can be reviewed separately
- Human inspects each page (content, structure, wikilinks)
- Approve → page published. Reject → page removed.

### Stage 2: Lint Review

- After Lint produces review items
- **Cross-page** — contradictions, duplicates span multiple pages
- Human inspects each issue and decides: merge, fix, dismiss
- Periodic — human-triggered, not automatic

## Agentic Search

### Flow

```
User query
    │
    ▼
pgvector search → Top-K wiki pages (by embedding similarity)
    │
    ▼
Agent reads top-K pages (read_wiki)
    │
    ▼
Agent follows [[wikilinks]] → reads entity pages → follows more wikilinks
    │
    ▼
Agent synthesizes answer with citations
    │
    ▼ (optional)
Answer filed into queries/ directory for future reference
```

### Key difference from RAG

RAG retrieves chunks and the LLM re-derives knowledge on every query. Agentic search:

1. Finds the most relevant **wiki pages** (already-compiled knowledge, not raw chunks)
2. Follows **wikilinks** to explore related entities and concepts
3. Synthesizes from **structured, cross-referenced** content
4. Can **file** good answers back into `queries/`

## Concurrency Model

### ShallowCompile — Parallel

- Multiple requests concurrent. Queue if pool full. 503 if full.

### Lint — Per-Space Mutex

- One lint run per space at a time. 409 if running.
- Different spaces run concurrently.

### Agent Pool

| Space | Pool Size |
|-------|-----------|
| Personal | N=1 (fixed) |
| Team | N configurable, tier-gated |

**Tier gating:**

| Tier | Personal | Team |
|------|----------|------|
| Free | N=1 | N=1 |
| Pro | N=1 | N=2–4 |
| Enterprise | N=1 | N=2–16 |

## Observability — SSE

```
GET /api/compile/:id/events  →  SSE stream

event: phase       {phase: "shallow"|"lint", status: "started"|"completed"}
event: agent-start {agent_id, harness_type}
event: llm-round   {round, token_count}
event: tool-call   {tool, args}
event: tool-result {tool, success, summary}
event: entity      {entity, type}
event: lint-issue  {type: "contradiction"|"duplicate"|"orphan"|..., pages: [...]}
event: error       {message, recoverable}
```

## Harness Matrix

| Harness | Task Type | Backend | For |
|---------|-----------|---------|-----|
| `compile-simple` | shallow_compile | Python deepagents (HTTP) | Source → wiki + entities + concepts |
| `lint-wiki` | lint | deepagents (HTTP) | Health-check: contradictions, duplicates, orphans |
| `agentic-search` | search | deepagents (HTTP) | Vector search → wikilink exploration |
| `review` | review_submission | Swink Agent (HTTP) | Review operations (future) |

All agents are separate HTTP/gRPC processes.

## API Surface

| Endpoint | Method | Description |
|----------|--------|-------------|
| `POST /api/compile` | POST | ShallowCompile: sources → wiki + entities + concepts |
| `GET /api/compile/:id/events` | GET | SSE agent event stream |
| `POST /api/lint` | POST | Trigger lint for a space |
| `GET /api/search` | GET | Agentic search (vector → wikilinks → answer) |
| `GET /api/pages/:path` | GET | Get wiki page content + backlinks |
| `GET /api/entities/:name` | GET | Entity detail + mentioned-in pages + related entities |
| `GET /api/concepts/:name` | GET | Concept detail + related pages |

## State Persistence

`.cowiki/state.json` in git. Tracks identity hashes and all outputs per source for cascade-on-delete:

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
    "sha256_abc123": {
      "wiki": ["infra/docker-networking.md"],
      "entities": ["entities/docker.md"],
      "concepts": ["concepts/containerization.md"]
    }
  }
}
```

## Page Deletion

Sequential cleanup:

1. **Git** — `rm_wiki(path)` / `rm_entity(path)` / `rm_concept(path)`
2. **PSQL** — DELETE corresponding row from metadata table
3. **Backlinks** — agent removes references from entity `## Mentioned In` sections
4. **Compile State** — update `.cowiki/state.json`

## What This Design Does NOT Include

- PostgreSQL knowledge graph (facts table, page_entities junction) — replaced by wikilinks + FS
- DeepIntegrate as entity extraction — replaced by ShallowCompile entity creation + Lint health-check
- Neo4j migration — no graph DB needed
- Auto-scaling agent pools
- Agent harness implementations (existing projects)

## Related

- Issue [#15](https://github.com/wfnuser/cowiki/issues/15) — Compile system redesign
- Issue [#14](https://github.com/wfnuser/cowiki/issues/14) — HTML view layer
- Issue [#31](https://github.com/wfnuser/cowiki/issues/31) — Multi-format source ingestion
- Issue [#44](https://github.com/wfnuser/cowiki/issues/44) — Branch-aware search and merge
- Issue [#48](https://github.com/wfnuser/cowiki/issues/48) — No snapshots: delete+rebuild
- llm_wiki pattern — `wiki/` + `entities/` + `concepts/` + wikilinks + lint
