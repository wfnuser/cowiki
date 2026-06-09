# cowiki Compile System Design

> Status: v5 | Date: 2026-06-09 | Issue: [#15](https://github.com/wfnuser/cowiki/issues/15)

## Overview

A **two-stage compile pipeline** that transforms extracted source folders (from `memany-extractor`) into structured wiki knowledge with **wiki-page-centric architecture** and **decoupled agent communication**.

```
Ingest (memany-extractor)          Compile (cowiki)
─────────────────────────          ─────────────────
WebApp   ─┐
LocalFS  ─┤                       ShallowCompile         DeepCompile
RemoteFS ─┼─ SourceFolder ────→  (sync)          →     (async, human-triggered)
Text     ─┘  (manifest.json       sources → wiki         health-check:
             + content files       + entities            contradictions,
             + directory tree)     + concepts            duplicates, orphans,
                                   + wikilinks           broken links)
```

- **ShallowCompile (sync)**: Agent explores source folders in `sources/`, reads `manifest.json` and content files, produces wiki pages + entity pages + concept pages in markdown. Deduplicates against existing content using `content_hash` from the extractor. Inserts metadata into PSQL indices.
- **DeepCompile (async, human-triggered)**: Agent health-checks the wiki — detects contradictions, duplicates, orphan nodes, broken wikilinks. Personal Space: manual trigger. Team Space: post-review-approve hook.

**Core principle:** Wiki pages are the center. Entities are navigation bridges between wiki pages. Entities and concepts live as markdown files (FS source of truth) with PSQL metadata tables for search and dedup.

**Separation of concerns:** Extraction (fetching raw content → structured Markdown) is handled by the external `memany-extractor` crate. Compile (Markdown → wiki pages + entities + concepts + relationships) is cowiki's domain. See [Related](#related) for the extractor design proposal.

## Folder Source Types

Cowiki ingests content from four source types, each producing a structured `SourceFolder` via `memany-extractor`:

| Source Type | Origin | How It Works | Typical Input |
|-------------|--------|-------------|---------------|
| **WebApp** | Web application / documentation site | Headless Chrome (chromiumoxide) crawls multiple pages, converts HTML to Markdown via processor chain | Documentation site URL, web app |
| **LocalFS** | Local filesystem directory | `walkdir` traverses directory, hard-links files, routes through file processors by MIME type | `/home/user/docs/`, mounted drive |
| **RemoteFS** | Remote filesystem | SSH/SFTP or AWS S3 fetches files to a temporary work directory, then processes them | `ssh://server/path/`, `s3://bucket/prefix/` |
| **Text** | Inline text / Markdown string | Passthrough — validates Markdown structure, extracts frontmatter metadata | Pasted text, piped input from CLI |

Each source is extracted **before** ShallowCompile runs. The extraction layer (`memany-extractor`) produces a `SourceFolder` containing `manifest.json`, processed content files (Markdown), and assets — preserving the original directory tree structure. ShallowCompile consumes these structured folders as input.

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
  sources/                      ← extracted source folders
    webapp-docs-site/           ← WebApp source (directory tree preserved)
      manifest.json
      index.md
      guides/
        getting-started.md
        deployment.md
      assets/
        diagram.png
    localfs-project-notes/      ← LocalFS source (directory tree preserved)
      manifest.json
      design-notes.md
      meeting-notes.md
    text-cli-pipe/              ← Text source (flat)
      manifest.json
      content.md
  entities/                     ← named entities
    docker.md
    kubernetes.md
  concepts/                     ← abstract themes
    containerization.md
    microservices.md
  queries/                      ← agent-digested Q&A (reserved)
  .cowiki/state.json
```

### manifest.json (From memany-extractor)

Each source folder contains a `manifest.json` produced by `memany-extractor`. The compile system **reads** these manifests — it does not generate them.

| Field | Use in Compile |
|-------|---------------|
| `content_hash` | Identity check for ShallowCompile dedup |
| `source_type` | `"webapp"`, `"localfs"`, `"remotefs"`, or `"text"` — informs agent strategy |
| `source_location` | Original source address (URL, path) for traceability |
| `files[]` | File manifest with `role` (primary/asset/raw), `status` (ok/skipped/error), and `path` |
| `processor_log` | Extractor statistics for debugging extraction issues |

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

1. **Identity Check** — read `manifest.json` from each source folder; lookup `content_hash` in `.cowiki/state.json`; skip if already compiled
2. **List Source Folders** — enumerate folders in `sources/`
3. **Acquire Agent** — from space pool
4. **Agent Execution** — agent:
   - Surveys source folders via `ls_sources` (names, types, freshness)
   - Reads `manifest.json` via `read_manifest` to understand content structure and extractor status
   - Browses directory trees via `ls_source_dir` (preserves hierarchy — sibling files suggest related pages, subdirectories suggest nested topics, parent directory names suggest categories)
   - Reads content files via `read_source`
   - **Deduplicates** against existing wiki/entities/concepts (reads via `read_wiki`, `read_entity`, `read_concept`)
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

**Source Folder Exploration (read-only):**

| Tool | Description |
|------|-------------|
| `ls_sources` | List source folders in `sources/` (name, source_type, content_hash, freshness) |
| `read_manifest` | Read `manifest.json` from a source folder — file list with roles, statuses, processor log |
| `ls_source_dir` | List contents of a directory within a source folder — preserves hierarchy |
| `read_source` | Read a specific extracted content file from a source folder |

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

### Directory Tree Utilization

The agent uses the preserved directory structure from extraction to inform wiki page organization:

| Structure Signal | Wiki Inference |
|-----------------|----------------|
| Sibling files in same directory | Related pages — likely share entities and concepts |
| Nested subdirectories | Topic hierarchy — subdirectory name becomes a category or parent page |
| Parent directory name | Implicit category or grouping label |
| `assets/` directory | Media references — images embedded in content |
| `manifest.json` file count + roles | Content scope — how many primary vs asset vs raw files |

### Dedup & Merge Strategy

During ShallowCompile, the agent:

1. **Source-level dedup** — checks `content_hash` in manifest.json against `.cowiki/state.json`. If the exact same source folder was already compiled, skip.
2. **Entity dedup** — before creating a new entity, reads existing entities. If similar (by embedding or name), merges: adds new aliases, updates summary, appends new wiki page to `## Mentioned In`
3. **Concept dedup** — same pattern for concepts
4. **Wiki page dedup** — checks if source content is already covered by an existing wiki page. If redundant, skips creation
5. **Cross-space scope** — dedup scope is per-space. Personal Space entities don't conflict with Team Space entities

### pgvector Timing

| Space | pgvector Insert |
|-------|----------------|
| Personal Space | Immediately after ShallowCompile |
| Team Space | On review approve |

## DeepCompile: Wiki Health Check (Async, Human-Triggered)

Replaces the previous "DeepIntegrate" concept. No more knowledge graph extraction — entities and concepts are already created during ShallowCompile. DeepCompile verifies quality.

### Trigger

- **Personal Space**: Manually triggered by user (`POST /api/lint`)
- **Team Space**: Post-review-approve hook (after pgvector insert)
- No automatic cron in current version

### What DeepCompile Checks

| Issue | Detection | Agent Action |
|-------|-----------|-------------|
| **Contradiction** | Two wiki pages make conflicting claims | Flag for human review |
| **Duplicate** | Two pages/entities cover the same topic with high overlap | Suggest merge |
| **Orphan** | Entity with zero `## Mentioned In` backlinks | Flag for cleanup |
| **Broken link** | `[[wikilink]]` points to non-existent page | Fix or remove |
| **Missing backlink** | Wiki page mentions entity but entity doesn't link back | Auto-fix |
| **Stale content** | Page hasn't been updated after newer sources ingested | Flag |

### Agent Tools — DeepCompile

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

DeepCompile produces `ReviewItem` entries (contradiction, duplicate, orphan, etc.) streamed via SSE. Human reviews and resolves each item — accept fix, dismiss, or manually address.

## Review: Two-Stage

### Stage 1: Shallow Review

- After ShallowCompile, wiki pages are listed for review
- **Independent per-page** — each page can be reviewed separately
- Human inspects each page (content, structure, wikilinks)
- Approve → page published. Reject → page removed.

### Stage 2: DeepCompile Review

- After DeepCompile produces review items
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

### DeepCompile — Per-Space Mutex

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

`.cowiki/state.json` in git. Tracks identity information and all outputs per source folder for cascade-on-delete:

```json
{
  "sources": {
    "webapp-docs-site": {
      "content_hash": "sha256:abc123...",
      "source_type": "webapp",
      "source_location": "https://example.com/docs"
    },
    "localfs-project-notes": {
      "content_hash": "sha256:def456...",
      "source_type": "localfs",
      "source_location": "/home/user/project-notes"
    }
  },
  "source_pages": {
    "webapp-docs-site": {
      "wiki": ["infra/docker-networking.md"],
      "entities": ["entities/docker.md"],
      "concepts": ["concepts/containerization.md"]
    },
    "localfs-project-notes": {
      "wiki": ["guides/deployment.md"],
      "entities": [],
      "concepts": []
    }
  }
}
```

Identity check uses `content_hash` from `manifest.json` (produced by `memany-extractor`), not a separately computed hash. Source type and location are recorded for traceability.

## Page Deletion

Sequential cleanup:

1. **Git** — `rm_wiki(path)` / `rm_entity(path)` / `rm_concept(path)`
2. **PSQL** — DELETE corresponding row from metadata table
3. **Backlinks** — agent removes references from entity `## Mentioned In` sections
4. **Compile State** — update `.cowiki/state.json`

## What This Design Does NOT Include

- **File-level extraction** (PDF, DOCX, PPTX, CSV, etc.) — handled by `memany-extractor` (see [`docs/plans/2026-06-07-extractor-design-proposal.md`](plans/2026-06-07-extractor-design-proposal.md))
- **Ingest API** (`POST /api/ingest`) — extraction layer concern; this design assumes sources are already extracted into `sources/`
- **PostgreSQL knowledge graph** (facts table, page_entities junction) — replaced by wikilinks + FS
- **DeepIntegrate as entity extraction** — replaced by ShallowCompile entity creation + DeepCompile health-check
- **Neo4j migration** — no graph DB needed
- **Auto-scaling agent pools**
- **Agent harness implementations** (existing projects)

## Related

- [`docs/agent-integration-design.md`](../agent-integration-design.md) — Agent runtime integration (PiAgent, protocol, session routing)
- [`docs/agent-integration-architecture.html`](../agent-integration-architecture.html) — Agent integration architecture diagram
- Issue [#15](https://github.com/wfnuser/cowiki/issues/15) — Compile system redesign
- Issue [#14](https://github.com/wfnuser/cowiki/issues/14) — HTML view layer
- Issue [#31](https://github.com/wfnuser/cowiki/issues/31) — Multi-format source ingestion
- Issue [#44](https://github.com/wfnuser/cowiki/issues/44) — Branch-aware search and merge
- Issue [#48](https://github.com/wfnuser/cowiki/issues/48) — No snapshots: delete+rebuild
- [`docs/plans/2026-06-07-extractor-design-proposal.md`](plans/2026-06-07-extractor-design-proposal.md) — Per-format extractor design
- `memany-extractor` crate — Source extraction implementation (external Rust library)
- llm_wiki pattern — `wiki/` + `entities/` + `concepts/` + wikilinks + lint
