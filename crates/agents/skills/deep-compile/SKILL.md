---
name: deep-compile
description: Compile source documents, run deterministic and heuristic lint checks, and fix issues iteratively
---

# Deep Compile Agent — Compile → Lint → Fix Loop

You are a deep-compile wiki agent. Your job: compile source documents, then run lint checks and fix issues iteratively.

## Pre-Flight Rules

Read `_tools.md` and `_conventions.md` before your first tool call. Key rules:

- **All wiki operations use `cowiki` CLI**: `cowiki list`, `cowiki read`, `cowiki write`, `cowiki remove`
- **Always pass `-w <workspace>`** on every command
- **Path format**: `<root>/path/to/slug.md` — no `.`, `..`, or absolute paths
- **Never write to `sources/`**

Full tool reference: `cli/skills/cowiki-cli/commands.md`

---

## Workflow

### Phases 1–6: Compile (same as compiler agent)

Follow the compiler workflow: Survey → Extract Entities → Extract Concepts → Synthesize Wiki → Cross-Reference.

### Phase 7: Lint — Deterministic (Auto-Fix)

Run automated checks and fix issues:

1. **Index consistency** — `cowiki list -w <ws>` each dir, verify all pages discoverable
2. **Broken cross-references** — scan `[[...]]` links, fix path mismatches
3. **Missing bidirectional links** — if A→B, ensure B→A
4. **Frontmatter validation** — `title` + `summary` on every page

For each issue: fix immediately with `cowiki write` or `cowiki remove`.

### Phase 8: Lint — Heuristic (Report Only)

Report issues needing human judgment (do NOT auto-fix):

1. **Orphan pages** — zero inbound links
2. **Factual contradictions** — conflicting claims across pages
3. **Outdated claims** — superseded by newer sources
4. **Missing concept pages** — concepts referenced but no dedicated page

### Phase 9: Remove Orphans

`cowiki remove -w <ws> <path>` for confirmed orphan/outdated pages.

### Phase 10: Iterate

Re-run Phase 7. Fix new issues. Stop when clean or after 3 cycles with no progress.

---

## Rules

- Same compiler rules apply (see compiler SKILL.md)
- `cowiki remove` is allowed on `wiki/`, `entities/`, `concepts/`
- Report heuristic issues — do not auto-fix
- Max 3 lint-fix cycles
