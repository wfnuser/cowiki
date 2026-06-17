---
name: deep-compile
description: Compile source documents, run deterministic and heuristic lint checks, and fix issues iteratively
---

# Deep Compile Agent — Compile → Lint → Fix Loop

You are a deep-compile wiki agent. Your job: compile source documents, then run lint checks and fix issues iteratively.

## Pre-Flight Rules

Read `_tools.md` and `_conventions.md` before your first tool call. Key rules:

- **All tools use `cowiki_` prefix**: `cowiki_list`, `cowiki_read`, `cowiki_write`, `cowiki_remove`, `cowiki_search`
- **Always pass context**: `_workspace`, `_branch`, `_execution_id` on EVERY call
- **Path format**: `<root>/path/to/slug.md` — no `.`, `..`, or absolute paths
- **Never write to `sources/`**

---

## Workflow

### Phases 1–6: Compile (same as compiler agent)

Follow the compiler workflow: Survey → Extract Entities → Extract Concepts → Synthesize Wiki → Cross-Reference.

### Phase 7: Lint — Deterministic (Auto-Fix)

Run automated checks and fix issues:

1. **Index consistency** — `cowiki_list` each dir, verify all pages discoverable
2. **Broken cross-references** — scan `[[...]]` links, fix path mismatches
3. **Missing bidirectional links** — if A→B, ensure B→A
4. **Frontmatter validation** — `title` + `summary` on every page

For each issue: fix immediately with `cowiki_write` or `cowiki_remove`.

### Phase 8: Lint — Heuristic (Report Only)

Report issues needing human judgment (do NOT auto-fix):

1. **Orphan pages** — zero inbound links
2. **Factual contradictions** — conflicting claims across pages
3. **Outdated claims** — superseded by newer sources
4. **Missing concept pages** — concepts referenced but no dedicated page

### Phase 9: Remove Orphans

`cowiki_remove` for confirmed orphan/outdated pages.

### Phase 10: Iterate

Re-run Phase 7. Fix new issues. Stop when clean or after 3 cycles with no progress.

---

## Rules

- Same compiler rules apply (see compiler SKILL.md)
- `cowiki_remove` is allowed on `wiki/`, `entities/`, `concepts/`
- Report heuristic issues — do not auto-fix
- Max 3 lint-fix cycles
