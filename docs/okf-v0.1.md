# Open Knowledge Format v0.1 compatibility

CoWiki Spaces use the selected repository root as an [Open Knowledge Format (OKF) v0.1](https://github.com/GoogleCloudPlatform/knowledge-catalog/blob/d44368c15e38e7c92481c5992e4f9b5b421a801d/okf/SPEC.md) bundle.

```text
space/
├── index.md                 # optional OKF root index; CoWiki creates it
├── log.md                   # optional OKF update log
├── architecture.md          # concept ID: architecture
├── teams/
│   ├── index.md             # progressive disclosure for this directory
│   └── customer.md          # concept ID: teams/customer
└── .cowiki/
    ├── sources/             # conforming `type: Source` concepts; hidden only in CoWiki UI
    └── legacy/              # byte-for-byte migration recovery artifacts, when needed
```

OKF does not prescribe domain directories. Names such as `entities/`, `concepts/`, `references/`, `tables/`, and `playbooks/` are producer choices, not standard sections. CoWiki displays arbitrary hierarchy and does not create fixed content categories.

Every non-reserved Markdown concept has parseable YAML frontmatter and a non-empty `type`:

```markdown
---
type: Note
title: Architecture
description: How the system is divided into components.
tags: [engineering]
---
```

CoWiki preserves unknown producer fields. `title`, `description`, `resource`, `tags`, and `timestamp` follow the standard but remain optional. Links between concepts are ordinary Markdown links. CoWiki's `.cowiki/` namespace is an additive producer convention: every source is stored as a Markdown concept there. Existing `.md` names remain readable; other filenames use a collision-free encoded `.md` path while retaining the original name in frontmatter. Non-Markdown state is outside OKF's document rules.

## Normative conformance matrix

This implementation follows every normative section of Google's `0.1 — Draft`; CoWiki conventions add behavior but do not replace standard semantics.

| Official section | Requirement | CoWiki behavior / verification |
| --- | --- | --- |
| §2 Terminology | Concept ID is the bundle-relative path without `.md` | API slugs and Git review paths use exactly that ID; no synthetic `wiki/` prefix |
| §3 Bundle structure | Arbitrary hierarchy of Markdown files | Space repository root is the bundle root; no required `entities/`/`concepts/` taxonomy |
| §3.1 Reserved names | `index.md` and `log.md` are never concepts | Classified and validated separately at every hierarchy level |
| §4 Concept documents | UTF-8 Markdown + parseable YAML frontmatter + body | Local writes normalize malformed concepts before commit; non-UTF-8 originals are archived byte-for-byte before a conforming replacement is written |
| §4.1 Frontmatter | Non-empty `type` is the only required field | Local writes supply it when absent; unknown types and unknown keys are accepted and preserved |
| §4.1 Recommended | `title`, `description`, `resource`, `tags`, `timestamp` | Accepted without becoming required; new pages use `title`/`description` |
| §4.2 Body | Standard Markdown; conventional sections are optional | Body is round-tripped without a Cowiki-only syntax requirement |
| §5 Cross-linking | Absolute bundle-relative and relative Markdown links | CoWiki authors standard Markdown links; broken links are never a conformance error |
| §6 Index | Optional at any directory; progressive-disclosure H1 sections; only root may have frontmatter | CoWiki preserves human prose and maintains a generated listing block after writes and migrations |
| §7 Log | Optional; no frontmatter; H1 title; ISO `YYYY-MM-DD` groups, newest first, flat list entries | Valid logs are preserved; invalid legacy logs are archived byte-for-byte and replaced by a valid empty log |
| §8 Citations | Optional numbered citations under `# Citations` | Preserved as standard Markdown; not falsely promoted from SHOULD to MUST |
| §9 Conformance tolerance | Do not reject missing optional fields, unknown types/keys, broken links, or missing indexes | Tests explicitly cover producer extensions and broken links |
| §11 Versioning | Optional root declaration; consumers must attempt best-effort future consumption | Writes `0.1`; unknown declared versions remain readable but are read-only so CoWiki never silently downgrades them |

The desktop local engine applies these rules directly to the selected repository. Its SQLite database is only a rebuildable search/backlink index; Markdown and Git remain the source of truth.

## Automatic migration from the legacy layout

Opening a pre-OKF repository normalizes the checked-out local branch before it is exposed to the UI or MCP:

| Legacy path | OKF-aligned path |
| --- | --- |
| Any existing concept path | Preserved in place; `wiki/`, `entities/`, and `concepts/` remain ordinary directories |
| Missing directory index | A generated `index.md` beside that directory's concepts |
| Undeclared legacy raw `sources/input.md` | `.cowiki/sources/input.md` |

Migration adds `type: Note` only where required and mirrors legacy `summary` into the standard `description` field. Already-conforming concepts remain byte-for-byte unchanged. A top-level `sources/` directory is inferred as legacy only when the bundle has no OKF version declaration and contains raw/non-conforming source files; declared 0.1 and future bundles keep `sources/` as an ordinary hierarchy with stable Concept IDs. Raw legacy sources become valid `type: Source` concepts. Unknown fields such as `page_id` are preserved. Nested index metadata is converted into visible Markdown because only the root index may carry frontmatter. Malformed legacy frontmatter is preserved as body text under a new valid frontmatter block. If a legacy and canonical path collide, the canonical file wins and the other blob is archived under `.cowiki/legacy/collisions/`; a malformed legacy `log.md` is likewise retained byte-for-byte under `.cowiki/legacy/`.

Migration is applied automatically only to a clean checked-out branch. Existing repositories receive one explicit `Migrate Space to OKF v0.1` commit. Before changing anything, the local engine snapshots every working-tree byte—including ignored and non-UTF-8 files—plus empty directories and the Git index. If normalization or commit creation fails, it restores that snapshot without changing HEAD. If local uncommitted changes exist, opening stops before changing files or refs and asks the caller to preserve or commit those changes first. A new repository receives only an initial OKF index commit; imported concept files remain ordinary uncommitted local work until the user submits them.

The implementation is pinned to OKF `0.1 — Draft`, official repository commit `d44368c15e38e7c92481c5992e4f9b5b421a801d` (inspected 2026-07-14).

## Verification

```bash
cargo test --manifest-path web/src-tauri/Cargo.toml -- --test-threads=1
cd web && npm test && npm run build
```

The desktop suite covers arbitrary hierarchy, full Concept IDs, reserved-file rules, permissive extension semantics, hidden source concepts, standard Markdown backlinks, review paths, lossless legacy migration, rollback, and idempotent re-open. Frontend contract tests ensure the UI does not recreate a fixed taxonomy or expose reserved/source documents as normal pages.
