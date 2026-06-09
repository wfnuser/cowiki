# cowiki — Correctness & Refactor Roadmap

Synthesized from two reviews of the `dev` branch:
- **codex** — 15 structural/quality notes.
- **multi-agent review** — 5 area reviewers + adversarial verification (50 verified findings).

**Organizing principle:** group by *same file / same concept / mutual dependency*, not one task per finding.

> ⚠️ **Read this first.** The two reviews diverge on priority. codex focused on refactoring; the multi-agent review surfaced **live security holes and guaranteed-crash bugs**. Those are **not** in the refactor backlog — they are tracked as **P0/P1 bugfix issues** (Section 1) and should ship as small, fast PRs *before* the structural Epics. Section 2 holds structural governance only.

---

## Section 1 — Verified Bugs / Correctness / Security (P0/P1 — not refactors)

These are confirmed defects, not cleanup. Fix as small standalone PRs.

| Pri | Issue | Problem | Key locations |
|-----|-------|---------|---------------|
| **P0** | [#55](https://github.com/wfnuser/cowiki/issues/55) | **Workspace data plane skips auth/authz.** pages r/w, ingest, compile, sources, review `list`/`get`, submit team-path are open; no global auth layer. Anonymous read of private spaces, arbitrary page overwrite, LLM-budget burn. | `routes/pages.rs:87/101/129/151`, `ingest.rs:25`, `compile.rs:37`, `sources.rs:51/89`, `review.rs:11/26`, `submit.rs:148`, `main.rs:240` |
| **P0** | [#56](https://github.com/wfnuser/cowiki/issues/56) | **`merge_to_main` self-deadlocks** on a non-reentrant `RwLock` → approve/merge hangs the request thread permanently. | `core/src/git.rs:438-453` + `write_file:228`, `branch_lock:187` |
| **P0** | [#57](https://github.com/wfnuser/cowiki/issues/57) | **`reject_invitation` violates the `invitations.status` CHECK** (`'rejected'` not allowed) → live endpoint crashes. 1-line migration. | `db/workspaces.rs:345-352`, `002_workspaces.sql`, `workspace.rs:419` |
| **P1** | [#26](https://github.com/wfnuser/cowiki/issues/26) *(escalated)* | **Pages not workspace-scoped in DB.** `upsert` never writes `workspace_slug`; unique key `(slug, branch)`; reads filter only on `branch` → cross-tenant overwrite + search/dedup leak. Needs backfill + ANN index + migration version table. | `db/pages.rs:18-126`, callers `compile.rs:140`, `review.rs:130`, `search.rs:38`, `submit.rs:71` |
| **P1** | [#58](https://github.com/wfnuser/cowiki/issues/58) | **SSRF in `fetch_url`** — fetches any URL, follows redirects → cloud metadata / internal ports via ingest. | `extractor/src/fetch.rs:9-32`, `ingest.rs:44` |
| **P1** | [#59](https://github.com/wfnuser/cowiki/issues/59) | **Credential & OAuth hardening** — plaintext API keys, key in OAuth redirect URL, no OAuth `state`, key in `localStorage`, `Serialize`-able `User` with raw key. | `db/users.rs:14/5-11`, `auth.rs:92/204/241`, `web/src/App.tsx:11`, `web/src/auth.ts:22` |

**Already tracked (don't duplicate):** [#54](https://github.com/wfnuser/cowiki/issues/54) review-comment authz (the comments slice of #55), [#50](https://github.com/wfnuser/cowiki/issues/50) move embedding out of approve (the perf slice of #55/submit).

---

## Section 2 — Refactor Epics (structural governance)

Each Epic bundles codex notes + verified-review findings that touch the same file/concept. Sequencing and constraints are explicit.

### Epic A — Error visibility + structured errors
*codex #6, #11, #12, #15 + review cross-cutting "swallowed errors"*

Split into two PRs (low-risk first):
- **A1 (observability — do first, low-risk/high-value):** add `tower_http::trace::TraceLayer` (method/path/status/latency); stop swallowing errors — Rust `.ok()`/`let _ =` (compile/review upsert, `save_state`, audit), LLM/VLM `chat()` missing HTTP-status check (`llm/openai.rs:82`, `vlm/openai.rs:79`), embedder empty/length-mismatch (`embedder/openai.rs:125/183`); TS empty `catch {}` / missing `.catch` (`MainLayout`, `DiscoverView`, `ReviewDetail`).
- **A2 (structured errors — align with Epic C):** `AppError → {error, code}`; compile returns per-page structured errors (#15). Touches frontend API handling → land **after / with** Epic C's `request<T>()`.

Related: [#15](https://github.com/wfnuser/cowiki/issues/15) compile optimization.

### Epic B — Shared abstractions / DRY
*codex #5, #13 + review "copy-paste over abstraction"*

- **Frontend:** `request<T>(path, init?)` that injects auth, checks `res.ok`, parses the `{error}` envelope once (replaces ~24 duplicated fetch blocks + two inconsistent error-parsing strategies) — this is codex #13's `useAsyncResource` core. Plus `timeAgo`×3 → `lib/time.ts`, `statusBadge`×2 → `lib/review.ts`, `useIngestSource` (AddSource/IngestForm dup).
- **Backend:** shared `openai_chat()` (LLM/VLM are near-verbatim — so the #56-adjacent status-check fix needn't be applied twice); de-dup the `run_migrations` closure (×9).
- `makePageMarkdown({title,summary,kind})` frontmatter builder (codex #5) → see Epic E.

> The authz-helper dedup (codex #2/#14) is **not** here — it lands in **#55** because the gaps are a security bug, not cleanup.

### Epic C — Decompose `MainLayout` (maintainability only)
*codex #3, #13 + review "god component"*

`MainLayout.tsx` (1176 lines, ~35 `useState`, manual `ActiveView` routing) → hooks: `useWorkspaces` / `usePages` / `useSources` / `useReviews` / `useWorkspaceDialogs`, each on Epic B's `request<T>`. Also fix `loadWorkspaces` stale-closure deps, non-memoized search IIFE, and the user-branch-first selection 404 waterfall.

> **Constraint (codex #5):** this is a maintainability refactor — **no visual or interaction changes**. Mixing in a UI redesign makes the diff unreviewable.

### Epic D — Domain enums: review status + design tokens
*codex #7, #10 + review `statusBadge` dup*

One Epic, layered PRs:
- **D1 (semantic):** backend `status` enum/constants (pending/approved/rejected/merged/changes_requested) across DB + API.
- **D2 (presentation):** frontend centralized badge mapping + status colors folded into `web/src/lib/design.ts` tokens (no more local `const C`/hardcoded colors).

Related: [#19](https://github.com/wfnuser/cowiki/issues/19) review schema redesign.

### Epic E — Frontmatter model + split `pages.rs`
*codex #4, #5 + review (empty-slug, dup loaders, O(N·depth) tree)*

- Split `pages.rs` → `frontmatter.rs` (parse) + `wiki_tree.rs` (tree build) + thin HTTP route.
- Shared frontmatter **builder** used by frontend (`makePageMarkdown`) **and** backend → kills "no-title page" at the source (review: symbolic-only title strips to `""` → all collide on `wiki/.md`).
- Fix dup compile-state loader (`sources.rs:40` ≡ `compile.rs:179`) and `list_pages_from_repo` O(N·depth) re-scan.

> **Constraint (codex #7):** natural follow-up of #53 — **must add tests** (title fallback / canonical title) or it regresses.

### Epic F — `git.rs`: async safety + split (two PRs)
*codex #1, #8 + review (write_file shared-index race)*

- **F1:** wrap synchronous `git2`/`fs`/lock ops in `spawn_blocking` (don't block tokio workers). **Behavior-preserving** — do this in isolation so any behavior change is easy to bisect. Coordinate with **#56** (same locking code; fix the deadlock here or just before).
- **F2:** split `git.rs` → `repo_manager` / `repo_read` / `repo_write` / `diff`; fix the `write_file` nested-path shared-index race (`git.rs:252-287`). Shape `repo access` behind one interface so a future shared-storage/queue model doesn't require a rewrite.

Related: [#16](https://github.com/wfnuser/cowiki/issues/16) git storage, [#18](https://github.com/wfnuser/cowiki/issues/18) backend core/service/storage layering.

### Epic G — Test organization
*codex #9*

`db/workspaces.rs` (1287 lines) tests → backend integration tests, or split by role/member/invitation/delete. (Test-helper seeding bug already fixed on a prior PR.) Related: [#17](https://github.com/wfnuser/cowiki/issues/17) test infrastructure.

---

## Recommended sequence

| Batch | Work | Why |
|-------|------|-----|
| **0 (now)** | #57, #56 (crash bugs) · #55 (data-plane authz) | Crashes + live security hole — block everything else |
| **1** | #26 (multi-tenancy + index) · #58 (SSRF) · #59 (credentials) | Correctness + security |
| **2** | Epic A1 (observability) · Epic B (shared abstractions) | Make failures visible + de-dup foundation |
| **3** | Epic C (MainLayout split) · Epic D (status enums) | Frontend maintainability + semantic収口 |
| **4** | Epic E (frontmatter) · Epic F (git split) · Epic A2 (structured errors) | Structural cleanup |
| **5** | Epic G (tests) | Hygiene |

**Dependencies:** A2 (`{code}`) ← consumed by C's `request<T>`; D2 (badges) ← needs C's component structure; F1 (spawn_blocking) ← do with/after #56.
