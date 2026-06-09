# Refactor Epic drafts (for review)

First drafts of the **deferred structural Epics** from `docs/refactor-roadmap.md` — the ones that were *not* implemented in the overnight bugfix pass because they're large, decision-heavy, or risky to do unattended. Each draft gives a concrete target structure, real signatures/sketches, and an **Open decisions for review** section. They are design drafts, not PRs yet — once the direction + decisions are agreed, each becomes a focused implementation PR.

| Draft | Epic | Key decision(s) to settle |
|-------|------|---------------------------|
| [C](./C-mainlayout-decomposition.md) | Decompose `MainLayout` into hooks | routing ownership; hook granularity |
| [D](./D-submission-status-enum.md) | Submission status enum + status presentation | `status` typing (enum end-to-end?); labels/colors |
| [E](./E-frontmatter-and-pages-split.md) | Frontmatter module + split `pages.rs` | **title fallback strategy**; module location |
| [F2](./F2-git-module-split.md) | Split `git.rs` + nested-write race fix | `spawn_blocking` boundary; in-mem tree vs detached index |
| [G](./G-test-organization.md) | Test reorganization | per-test isolation strategy |

Not included (already P0/P1 issues, not refactors): #55 authz, #56/#57 crashes, #26 multi-tenancy, #58 SSRF, #59 credentials.
