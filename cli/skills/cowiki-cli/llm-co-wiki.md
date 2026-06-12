# LLM Co-Wiki — Local Agent Compile Workflow

Detailed guide for Path 2 of the cowiki content workflow: local agent compile
with entity extraction, concept formation, and cross-referencing.

Inspired by [karpathy-llm-wiki](https://github.com/Astro-Han/karpathy-llm-wiki),
adapted for cowiki's multi-directory architecture and CLI.

---

## Architecture

```
personal-<id>/ (workspace)
├── wiki/          ← General knowledge pages
├── entities/      ← Extracted entities (people, projects, events, organizations)
└── concepts/      ← Patterns, decisions, conventions, mental models
```

Unlike a flat `wiki/` directory, cowiki separates **entities** (nouns) from
**concepts** (ideas) so cross-references stay organized as the knowledge base grows.

---

## Phase 1: Gather Context

Before adding new knowledge, understand what already exists.

### 1.1 Scan All Directories

```bash
cowiki list -w <ws> --dir all
```

This returns a merged tree of `wiki/`, `entities/`, and `concepts/`. Use it to:
- Identify existing pages that may be affected by new content
- Find candidate cross-reference targets
- Avoid duplicating existing entities/concepts

### 1.2 Read Related Pages

```bash
# Read pages that overlap with the new content's topic
cowiki read -w <ws> <related-page> --dir wiki
cowiki read -w <ws> <entity-name> --dir entities
cowiki read -w <ws> <concept-name> --dir concepts
```

### 1.3 Search for Connections

```bash
cowiki search "keyword or phrase"
```

Use semantic search to find pages that may be conceptually related even if they
don't share exact keywords. Cowiki's search spans all directories.

---

## Phase 2: Entity Extraction

Entities are named things: people, projects, organizations, events, tools,
papers, books. They are the **nouns** of your knowledge base.

### 2.1 Identify Entities

From new content (a document, article, or conversation), extract:

| Category | Examples | Slug Pattern |
|----------|----------|-------------|
| People | authors, researchers, historical figures | `people/<name>` |
| Projects | open-source projects, internal initiatives | `projects/<name>` |
| Organizations | companies, labs, institutions | `orgs/<name>` |
| Events | conferences, launches, milestones | `events/<name>-<year>` |
| Artifacts | papers, books, tools, datasets | `papers/<short-title>`, `tools/<name>` |

### 2.2 Check for Existing Entities

Before creating a new entity page, check if it already exists:

```bash
cowiki list -w <ws> --dir entities/<category>
cowiki read -w <ws> <category>/<name> --dir entities
```

### 2.3 Create Entity Pages

```bash
# Create a person entity
cowiki write -w <ws> people/alice --dir entities --title "Alice Chen" --body "\
## Bio

Senior ML researcher at ExampleLab. Focus: transformer architectures.

## Notable Work

- Paper: \"Attention Is All You Need\" (2017, co-author)
- Project: [llm-compiler](https://github.com/example/llm-compiler)

## Related Concepts

- [[concepts/attention-mechanism]]
- [[concepts/transformer-architecture]]
- [[concepts/llm-compilation]]

## References

- [ExampleLab Profile](https://example.com/alice)
- Interview: 2026-05-12, personal communication
"

# Create a project entity
cowiki write -w <ws> projects/my-project --dir entities --title "My Project" --body "\
## Overview

Open-source tool for X. Started 2025, 2.3k stars.

## Key Contributors

- [[entities/people/alice]] — lead architect
- [[entities/people/bob]] — core maintainer

## Related Concepts

- [[concepts/open-source-sustainability]]
"
```

### 2.4 Entity Page Convention

Every entity page should include:
- **Title**: display name (via `--title`)
- **Category**: implicit from subdirectory (e.g., `people/`, `projects/`)
- **Cross-references**: `[[entities/...]]` and `[[concepts/...]]` links
- **Sources**: where the information came from (URL, conversation date, etc.)

---

## Phase 3: Concept Extraction

Concepts are ideas, patterns, decisions, and mental models. They are the
**verbs and frameworks** of your knowledge base.

### 3.1 Identify Concepts

From new content, extract:

| Category | Examples |
|----------|----------|
| Patterns | design patterns, architectural patterns, recurring solutions |
| Decisions | architecture decisions (ADR-style), trade-offs, why-X-not-Y |
| Conventions | coding standards, naming rules, team practices |
| Mental Models | frameworks, heuristics, ways of thinking |
| Definitions | precise definitions of domain-specific terms |

### 3.2 Check for Existing Concepts

```bash
cowiki list -w <ws> --dir concepts
cowiki search "concept keyword"
```

### 3.3 Create Concept Pages

```bash
cowiki write -w <ws> patterns/error-handling --dir concepts --title "Error Handling Pattern" --body "\
## Context

When building CLI tools that interact with remote APIs...

## Problem

Errors from different sources (network, validation, auth) need consistent
presentation to the user.

## Solution

Use a layered error type with user-friendly messages:

```
ApiError → CliError → user-facing message
```

## Examples

- cowiki-cli uses \`CowikiClient\` → \`ApiError.fromResponse()\` → \`printError()\`
- See implementation at [cowiki-cli error.ts](https://github.com/wfnuser/cowiki)

## Related

- [[concepts/decisions/cli-error-strategy]]
- [[entities/projects/cowiki-cli]]
- [[wiki/cli-design-principles]]
"

cowiki write -w <ws> decisions/use-git-for-storage --dir concepts --title "ADR: Git as Storage Backend" --body "\
## Status

Accepted (2026-05)

## Context

We need a storage backend for wiki pages. Options considered:

1. PostgreSQL with JSONB
2. Git repository (filesystem + git2)
3. Object storage (S3)

## Decision

Use Git as the primary storage backend.

## Rationale

- Natural version history via git commits
- Branch-based draft workflow
- No additional infrastructure beyond what we already have
- Filesystem access for debugging and direct editing

## Consequences

- Search requires separate PostgreSQL FTS index
- Concurrent writes need branch-level locking
- Large binary files not suitable
"
```

### 3.4 Concept Page Convention

- **Context/Problem/Solution** structure for patterns
- **Status/Context/Decision/Rationale/Consequences** for ADRs
- Cross-references to related entities and wiki pages
- Code examples where helpful

---

## Phase 4: Cross-Referencing

The value of a knowledge base compounds when pages link to each other.

### 4.1 Link Syntax

Use wiki-style `[[path]]` links within page bodies:

```
[[wiki/page-name]]           → link to wiki page
[[entities/people/alice]]    → link to entity
[[concepts/patterns/x]]      → link to concept
```

These are rendered as navigable links in the cowiki web UI.

### 4.2 Cross-Reference Rules

| When... | Add link to... |
|---------|---------------|
| An entity is mentioned in a concept page | The entity page |
| A concept applies to an entity | The concept page |
| A wiki page discusses a person/project | The entity page |
| A wiki page describes a pattern | The concept page |
| Two entities are related (e.g., person → project) | Both entity pages |

### 4.3 Updating Existing Pages

When adding new content, scan existing pages for cross-reference opportunities:

```bash
# 1. Find pages that mention the new entity/concept by name
cowiki search "Alice Chen"

# 2. Read candidates
cowiki read -w <ws> some-page --dir wiki

# 3. If the page discusses the entity/concept, add a cross-reference
# (re-write the page with added [[link]])
cowiki write -w <ws> some-page --dir wiki \
  --title "Updated Title" \
  --body "Updated body with [[entities/people/alice]] link..."
```

### 4.4 Bidirectional Linking

When you create entity `people/alice` and link to `concepts/transformer`,
also update `concepts/transformer` to link back to `entities/people/alice`:

```bash
# Read the concept page
cowiki read -w <ws> transformer --dir concepts

# Re-write with added backlink (copy existing body, add link section)
cowiki write -w <ws> transformer --dir concepts \
  --title "Transformer Architecture" \
  --body "Existing content...

## Referenced By

- [[entities/people/alice]] — co-author of original paper
"
```

---

## Phase 5: Wiki Pages (Synthesis)

After entities and concepts are extracted, create or update wiki pages that
synthesize the full picture.

### 5.1 When to Create a Wiki Page

- You have gathered multiple sources on one topic
- A topic spans several entities and concepts
- You need a narrative overview (not just a definition)

### 5.2 Wiki Page Structure

```bash
cowiki write -w <ws> llm-training --dir wiki --title "LLM Training: A Survey" --body "\
## Overview

Training large language models involves three main stages...

## Key Entities

- [[entities/people/alice]] — pioneered the transformer architecture
- [[entities/projects/gpt-4]] — largest known deployment

## Key Concepts

- [[concepts/attention-mechanism]]
- [[concepts/rlhf]]

## References

- [Attention Is All You Need](https://arxiv.org/abs/1706.03762)
- [RLHF Paper](https://arxiv.org/abs/2203.02155)
"
```

---

## Phase 6: Maintenance

### 6.1 Discover Orphan Entities

```bash
# Find entities with no incoming links
# (scan all wiki/concept pages for [[entities/...]] references)
cowiki list -w <ws> --dir entities
cowiki search "entities/"
```

### 6.2 Update Stale Content

When new information supersedes old content:
1. Read the outdated page
2. Append new findings with date annotation
3. Update cross-references

```bash
cowiki read -w <ws> old-concept --dir concepts
cowiki write -w <ws> old-concept --dir concepts \
  --title "Updated Concept" \
  --body "Previous content...

## Update (2026-06)

New findings suggest..."
```

### 6.3 Merge Duplicates

If two pages cover the same topic:
1. Read both pages
2. Merge content into the more canonical page
3. Replace the duplicate with a redirect note
4. Update all cross-references

---

## Quick Reference: cowiki CLI for LLM Wiki

| Operation | Command |
|-----------|---------|
| Scan all dirs | `cowiki list -w <ws> --dir all` |
| List entities | `cowiki list -w <ws> --dir entities` |
| List concepts | `cowiki list -w <ws> --dir concepts` |
| Read a page | `cowiki read -w <ws> <slug> --dir <dir>` |
| Create entity | `cowiki write -w <ws> <category>/<name> --dir entities --title "..." --body "..."` |
| Create concept | `cowiki write -w <ws> <category>/<name> --dir concepts --title "..." --body "..."` |
| Create wiki page | `cowiki write -w <ws> <slug> --dir wiki --title "..." --body "..."` |
| Search | `cowiki search "query"` |
| Nested path write | `cowiki write -w <ws> people/alice --dir entities --title "Alice" --body "..."` |
| Subdir list | `cowiki list -w <ws> --dir entities/people` |

---

## When to Use Path 1 vs Path 2

| Scenario | Path |
|----------|------|
| External URL or large document | Path 1: `cowiki ingest` → `cowiki compile` |
| Agent-generated synthesis from multiple sources | Path 2: Local Agent Compile |
| Cross-referencing across existing pages | Path 2 (Path 1 doesn't handle cross-refs) |
| Simple note-taking | Path 2: direct `cowiki write` |
