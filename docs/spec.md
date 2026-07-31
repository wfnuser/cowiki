# cowiki MVP Spec

> **Historical document — superseded.** This describes the original
> backend-first MVP. For the current local Space and Agent contract, see
> [README](../README.md), [Local MCP](mcp.md), and the
> [`cowiki-space` skill](../skills/cowiki-space/SKILL.md).

## Overview

cowiki is a collaborative knowledge base where humans and AI agents co-maintain a shared wiki. Users ingest sources, compile them into wiki pages, and submit to a shared space with review workflows. Version control is powered by Git under the hood, but users never see Git directly.

## Architecture

```
┌─────────────────────────────────────────┐
│              React + TS Frontend         │
│  Markdown Editor (Milkdown) + Tailwind  │
└──────────────────┬──────────────────────┘
                   │ HTTP / WebSocket
┌──────────────────▼──────────────────────┐
│              Rust Backend (axum)         │
│                                          │
│  ┌──────────┐ ┌────────┐ ┌───────────┐ │
│  │ Wiki API │ │Review  │ │ MCP Server│ │
│  │ (CRUD)   │ │ API    │ │ (agents)  │ │
│  └────┬─────┘ └───┬────┘ └─────┬─────┘ │
│       │           │             │        │
│  ┌────▼───────────▼─────────────▼──────┐ │
│  │          Core Services               │ │
│  │  Compiler · Dedup · Search · Auth   │ │
│  └────┬──────────────┬─────────────────┘ │
│       │              │                    │
│  ┌────▼────┐    ┌────▼─────┐             │
│  │  Git    │    │PostgreSQL│             │
│  │ (files) │    │(meta +   │             │
│  │         │    │ pgvector)│             │
│  └─────────┘    └──────────┘             │
└──────────────────────────────────────────┘
```

## Tech Stack

| Layer | Choice |
|-------|--------|
| Frontend | React + TypeScript + Vite + Tailwind |
| Markdown editor | Milkdown |
| Backend | Rust + axum |
| Database | PostgreSQL + pgvector |
| File storage + VCS | Git (gitoxide or libgit2) |
| LLM | OpenAI API |
| Agent protocol | MCP Server (Rust) |
| Desktop (post-MVP) | Tauri 2 |

## Data Model

### Git Repository Structure

```
cowiki-data/                    ← one OKF v0.1 bundle, versioned with Git
├── index.md                    ← bundle index + okf_version
├── concept-name.md             ← concept ID: concept-name
├── domain/
│   ├── index.md                ← directory index (progressive disclosure)
│   └── nested-concept.md       ← concept ID: domain/nested-concept
└── .cowiki/
    ├── sources/                ← valid `type: Source` concepts, hidden in product UI
    └── state.json              ← rebuildable compiler state

main                            ← Shared Space branch
user/zhangsan                   ← Personal Space branch
user/lisi                       ← another Personal Space branch
```

### PostgreSQL Tables

```sql
-- Users and auth
users (id, name, email, api_key, created_at)

-- Page metadata + embeddings for search/dedup
pages (id, slug, title, summary, branch, embedding vector(1536),
       created_by, created_at, updated_at)

-- Submissions (personal → shared)
submissions (id, user_id, status, summary, created_at, reviewed_by, reviewed_at)

-- Submission items
submission_pages (submission_id, page_slug, action, diff_summary)
```

## User-Facing Operations

### For Humans (Web UI)

| Operation | Description | API |
|-----------|-------------|-----|
| **Ingest** | Add URL/file/text to Personal Space | `POST /api/ingest` |
| **Compile** | Turn sources into wiki pages | `POST /api/compile` |
| **Edit** | Directly write/modify a page | `PUT /api/pages/:slug` |
| **Browse** | View pages in Personal or Shared Space | `GET /api/pages` |
| **Search** | Semantic search across Shared Space | `GET /api/search?q=...` |
| **Submit** | Propose pages to Shared Space | `POST /api/submit` |
| **Review** | Approve/reject/request changes | `POST /api/review/:id` |

### For Agents (MCP Server)

| Tool | Description |
|------|-------------|
| `cowiki_ingest` | Ingest a source (URL, text, file) into user's Personal Space |
| `cowiki_compile` | Compile sources into wiki pages |
| `cowiki_read` | Read a page from Shared or Personal Space |
| `cowiki_write` | Create or edit a page in Personal Space |
| `cowiki_search` | Semantic search across Shared Space |
| `cowiki_submit` | Submit pages from Personal to Shared Space |
| `cowiki_list` | List pages in a space |

## Core Flows

### Flow 1: Ingest + Compile + Submit

```
User/Agent: ingest("https://article-about-docker.com")
  → Fetches content, saves to .cowiki/sources/ on user branch
  → Returns: source_id

User/Agent: compile()
  → LLM reads sources, extracts concepts
  → Generates OKF concepts with frontmatter + standard Markdown links
  → Saves concept files at the bundle root or in producer-defined directories
  → Returns: list of generated page slugs

User/Agent: submit(["docker-networking", "docker-compose-tips"])
  → System runs: format standardization, dedup check, summary generation
  → Source files included in submission
  → Creates submission record in PostgreSQL
  → User sees: "Ready to submit. 1 possible duplicate found."
  → User confirms

Reviewer: review(submission_id)
  → Sees: LLM summary + file list + full diff
  → Approves
  → System merges to main branch
  → Updates page embeddings in pgvector
```

### Flow 2: Direct Edit + Submit

```
User/Agent: write("deployment-guide", "## Deployment\n\nNew content...")
  → Saves directly to the OKF bundle on user branch

User/Agent: submit(["deployment-guide"])
  → Same review flow as above
  → Diff shows changes to existing page
```

## Compilation Pipeline

On `compile`, the system:

1. **Hash check** — skip unchanged sources (SHA-256)
2. **Concept extraction** — LLM reads sources, identifies distinct concepts
3. **Page generation** — one page per concept, with:
   - OKF YAML frontmatter (`type` required; `title`, `description`, `resource`, `tags`, `timestamp` optional)
   - Markdown body
   - Source attribution (`^[source.md]`)
4. **Summary generation** — one-line summary per page (for search + review)
5. **Embedding** — generate embedding per page, store in pgvector

## Submit Pipeline

On `submit`, before creating the review request:

1. **Format standardization** — validate OKF frontmatter and standard Markdown links
2. **Dedup detection** — compare page embeddings against Shared Space pages (cosine similarity > 0.85 = flagged)
3. **Summary generation** — LLM generates submission summary ("3 new pages about Docker networking, 1 update to deployment guide")
4. **Source bundling** — include referenced source files in the submission

## Review UI

```
┌─────────────────────────────────────┐
│ Submission #12 by 张三               │
│                                      │
│ 📝 3 new pages about Docker          │
│    1 possible duplicate with         │
│    "Deployment Guide"                │ ← LLM summary
│                                      │
│ Files Changed (3)                    │
│  + docker-network-fix.md             │
│  + docker-compose-best.md            │
│  ~ deployment-guide.md               │
│                                      │
│ ─── docker-network-fix.md ───        │
│ + ## Docker Network Fix              │
│ + When containers can't talk...      │ ← full diff view
│                                      │
│ [Approve] [Request Changes] [Reject] │
└─────────────────────────────────────┘
```

## Permissions (MVP)

- Each user can only write to their own Personal Space (`user/{user_id}` branch)
- Shared Space (`main`) is read-only; writes only through approved submissions
- Any team member can review submissions
- Agent inherits the permissions of its owning user

## What MVP Does NOT Include

- Trace capture / session auto-import (hooks, OTel)
- CRDT real-time collaboration
- Desktop app (Tauri)
- Wikilink auto-resolution
- Contradiction detection
- Knowledge graph visualization
- Tiered loading (L0/L1/L2)
- Cross-tool sync (Notion/Feishu)
- Branching within Shared Space
- Fine-grained RBAC (admin/editor/viewer)
