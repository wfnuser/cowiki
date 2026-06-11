# Branch View, Submission & Merge Model

## The Problem

In Team Spaces, a user sees a merged view of `main` (approved content) + their `user/{id}` branch (drafts). This creates several issues:

1. **Which version to show?** If a user edited page X (on their branch) and page X also exists on main, which version do they see? Currently we try user branch first, fall back to main — but this is a hack, not a design.

2. **Search confusion.** If we index both main and user branch pages, a search for "docker" might return two versions of the same page. If we only index main, draft pages are unsearchable. If we only index user branch, pages the user hasn't touched are unsearchable.

3. **Stale drafts.** User edits page X on their branch. Meanwhile, someone else updates page X on main (via approved submission). User's branch has a stale copy. No mechanism to detect or resolve this.

4. **Page identity.** Two pages with the same slug on different branches — are they the "same" page or different pages? Git treats them as different file versions, but the user thinks of them as one document.

## Current Workaround

- `loadSpacePages` merges main + user branch (dedup by slug, user branch wins)
- `selectPage` tries user branch first, falls back to main
- Search is client-side only (no DB indexing yet), so no duplicate results — but this won't scale

## Possible Solutions

### A. "Overlay" model (like OverlayFS)
User branch is a transparent overlay on main. Reading always goes through overlay: if the file exists on user branch, use it; otherwise fall back to main. This is what we do now, but formalized.

**Pros:** Simple mental model. User sees "their version" of the wiki.
**Cons:** No way to see "what's on main" vs "what I changed" without explicit diff. Stale draft problem remains.

### B. "Draft marker" model
Pages on user branch that differ from main are explicitly marked as "draft". User sees one unified list with draft badges. Search indexes the user's view (overlay of main + drafts).

**Pros:** Clear visual distinction. Search works naturally.
**Cons:** Need to compute diff on every load to know which pages are drafts.

### C. "Working copy" model (like Git checkout)
User's branch is a full copy of main at the time they branched. Any page they haven't touched is identical to main. When main updates, user can "rebase" (pull latest main into their branch).

**Pros:** No overlay logic needed — user branch IS the full state. Search just indexes user branch.
**Cons:** Every user has a full copy of every page. Rebasing is complex.

## Decision

We adopt approach **C (working copy)** for the view, plus a concrete **submission + merge lifecycle**. Together these resolve all four problems above.

### View — read the user's branch directly (approach C)

A user always sees **their own `user/{id}` branch**, nothing else. The branch is a full tree (it was forked from `main`), so:

- **Which version (1):** always the branch's version — no "try branch, fall back to main" hack.
- **Search (2):** index the branch's tree (the user's effective view) — no cross-branch dedup.
- **Page identity (4):** a page is its slug on the user's branch; `main` is just where merged content lands.

Pages the user hasn't touched stay equal to `main` because the branch is kept current by **rebasing onto `main`** (see lifecycle). Staleness (3) is therefore a rebase concern, not a view concern.

### Branches

- **`user/{id}`** — the user's long-lived working branch (what they see and edit). Every write re-commits with `parent = merge-base(user/{id}, main)`, so the branch carries **exactly one in-progress working commit** on top of `main` — not a pile of autosave commits. Continuing a task keeps amending that one commit.
- **`pr/{submission-id}`** — a **snapshot branch** created at submit time, kept separate from the live `user/{id}` so review is stable. It is the unit that gets reviewed and merged.

### Lifecycle

1. **Edit** → amend the single working commit on `user/{id}`.
2. **Submit** (whole branch; user chooses when, typically after finishing one task) → rebase the work onto the current `main`, then snapshot it as `pr/{id}` with a frozen base commit `S`.
3. **Review changes** → made through the **review UI** against `pr/{id}`: a single review-fix commit `R` is added on top of `S` and **amended** across further change requests. `S` never moves, so `S` vs `R` is always a clean "what review changed" diff.
4. **Merge** → squash `S + R` into one commit, rebase onto the current `main`, **fast-forward**. `main` stays **linear** — no two-parent merge commits. A rebase conflict returns **409** for the author to resolve.
5. **After merge** → `user/{id}` is rebased onto the new `main` (lazily, on next access). Because the merged content came from this user, this is usually a no-op or trivial; genuine conflicts are surfaced for the user to resolve.

### Deliberately out of scope (for now)

- **No branch switching in the main editing UI.** A user only ever edits `user/{id}` directly; editing a PR happens inside the review screen, against `pr/{id}`.
- **One in-flight submission per branch** (matches the "finish a task, then submit" workflow). Concurrent PRs per user are not supported yet.

## Impact

- View reads a single branch — no overlay/dedup logic; search indexes the user's branch tree.
- Replaces the placeholder reviewer-authored re-commit (no conflict detection) with a real rebase + squash merge; see #56.
- A submission references the frozen `pr/{id}` snapshot, **not** the live `user/{id}` tip — edits made after submit can never silently change what was reviewed.
- `main` history is linear: one squash commit per merged submission, authored by the submitter.
- Stale drafts (problem 3) are resolved by rebasing `user/{id}` onto `main`; conflicts are the user's to resolve.
