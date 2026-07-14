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
    └── state.json           # rebuildable CoWiki compiler state
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
| §4 Concept documents | UTF-8 Markdown + parseable YAML frontmatter + body | Validator reports `utf-8` or `frontmatter`; writers normalize before commit |
| §4.1 Frontmatter | Non-empty `type` is the only required field | Validator enforces it; unknown types and unknown keys are accepted and preserved |
| §4.1 Recommended | `title`, `description`, `resource`, `tags`, `timestamp` | Accepted without becoming required; new pages use `title`/`description` |
| §4.2 Body | Standard Markdown; conventional sections are optional | Body is round-tripped without a Cowiki-only syntax requirement |
| §5 Cross-linking | Absolute bundle-relative and relative Markdown links | CoWiki authors standard Markdown links; broken links are never a conformance error |
| §6 Index | Optional at any directory; progressive-disclosure H1 sections; only root may have frontmatter | CoWiki preserves human prose and maintains a generated listing block after writes and migrations; bundle validation detects stale present indexes |
| §7 Log | Optional; no frontmatter; H1 title; ISO `YYYY-MM-DD` groups, newest first, flat list entries | `log-frontmatter`, `log-heading`, `log-date`, `log-order`, `log-entry` rules; standard `*`, `-`, and `+` bullets are accepted |
| §8 Citations | Optional numbered citations under `# Citations` | Preserved as standard Markdown; not falsely promoted from SHOULD to MUST |
| §9 Conformance tolerance | Do not reject missing optional fields, unknown types/keys, broken links, or missing indexes | Tests explicitly cover producer extensions and broken links |
| §11 Versioning | Optional root declaration; consumers must attempt best-effort future consumption | Writes `0.1`; an unknown declared version is not used as a reason to reject readable content |

`cowiki_core::okf::validate_bundle` checks all Markdown files, including producer-defined and hidden directories. `WikiRepo::validate_okf_branch` applies the same validator to an exact Git branch tree, so validation is branch-aware and does not depend on the working directory.

## Automatic migration from the legacy layout

Opening a pre-OKF repository creates an explicit Git migration commit on every local branch:

| Legacy path | OKF-aligned path |
| --- | --- |
| `wiki/topic.md` | `topic.md` |
| `wiki/team/_index.md` | `team/index.md` |
| `sources/input.md` | `.cowiki/sources/input.md` |

Migration adds `type: Note` only where required and mirrors legacy `summary` into the standard `description` field. Already-conforming concepts remain byte-for-byte unchanged. Raw legacy sources become valid `type: Source` concepts. Unknown fields such as `page_id` are preserved. Frontmatter-only legacy folder indexes retain their title as the new H1. Malformed legacy frontmatter is preserved as body text under a new valid frontmatter block. If a legacy and canonical path collide, the canonical file wins and the other blob is archived under `.cowiki/legacy/collisions/`; a malformed legacy `log.md` is likewise retained byte-for-byte under `.cowiki/legacy/`.

Migration is applied automatically only to a clean checked-out branch. After its migration commit, CoWiki checks out the new tree and synchronizes the Git index so the filesystem, index, and branch ref agree. If local uncommitted changes exist, opening stops before changing refs and asks the caller to preserve or commit those changes first.

The implementation is pinned to OKF `0.1 — Draft`, official repository commit `d44368c15e38e7c92481c5992e4f9b5b421a801d` (inspected 2026-07-14).

## Verification

```bash
cargo test -p cowiki-core --test okf_conformance
cargo test -p cowiki-core --test okf_repo_layout
cargo test --workspace -- --test-threads=1
```

The conformance suite covers fresh bundles, every required field, all reserved-file rules, permissive optional/extension semantics, hidden source concepts, review Concept IDs, lossless legacy migration, and idempotent re-open.
