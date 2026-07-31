# Review Diff UI (Issue #10, slice 1) — Design

**Goal:** Give cowiki a GitHub-PR-style review experience — first slice: a per-workspace **Reviews** tab showing the pending-submission queue, and a **document-friendly diff view** for each submission, with Approve/Reject wired.

**Scope (this slice):** entry point + queue + read-friendly diff + approve/reject. **Out of scope:** comments, `changes_requested`/resubmit, notifications, pagination/filtering (rest of Issue #10).

---

## UX / entry point

Chosen: **A′ — per-space tabbed page (GitHub repo style)**, built incrementally ("route 1"):
- A workspace **tab bar** (`Wiki · Sources · Reviews·N`) at the top of the main content pane.
- `Wiki`/`Sources` keep the existing sidebar-tree behaviour for now; only **Reviews** is newly built.
- Reviews tab shows **only on team (public) workspaces** — personal spaces use `skip_review` and have no submissions.
- URLs: `/{owner}/{ws}/reviews` (queue) and `/{owner}/{ws}/reviews/{id}` (one submission's diff), consistent with the existing `/{owner}/{ws}/{slug}` scheme. Driven by the existing `ActiveView` state; `navigate()` keeps the URL in sync (best-effort deep-linking).

## Diff rendering — document-friendly

The backend already returns per-file whole-file old/new content: `getReview(ws, id) → ReviewDetail { submission, diffs: FileDiff[] }`, `FileDiff = { path, old_content: string|null, new_content: string|null }`.

`DiffView` is a **pure presentational component**: input `FileDiff[]`, output the rendered diff. Internally it supports two render modes behind one interface (so the algorithm can change without touching callers):

- **Rendered (default) — "③"**: render each side's markdown to an HTML string (`marked`), compute a visual diff of the two HTML strings (`htmldiff-js`, which wraps changes in `<ins>`/`<del>`), sanitize (`dompurify`), and render. Produces the formatted document with inline strikethrough deletions + highlighted insertions — the wiki-reader-friendly view.
- **Source — "②" (fallback/toggle)**: word-level diff of the raw markdown source (`diff`'s `diffWordsWithSpace`), inline add/remove spans. Always works; shown via a per-view "Rendered / Source" toggle and used automatically if the rendered path throws.

Per-file framing: `old_content === null` → new file (all insert); `new_content === null` → deleted file (all delete); otherwise modified.

## Components (clean boundaries, each independently testable)

- `web/src/components/review/DiffView.tsx` — pure: `FileDiff[] → rendered diff`; owns the ②/③ logic + toggle. No data fetching.
- `web/src/components/review/ReviewList.tsx` — fetches `listReviews(ws)`; renders the queue (slug, author, file count, age, status); row click → navigate to detail.
- `web/src/components/review/ReviewDetail.tsx` — fetches `getReview(ws, id)`; header (title/author/status) + Approve/Reject buttons (call `reviewAction`, then refresh/return to list); body = `<DiffView diffs=… />`.
- `web/src/components/review/WorkspaceTabs.tsx` — the `Wiki · Sources · Reviews·N` tab bar; shows the pending count (derived from `listReviews(ws).length`); only renders Reviews for public workspaces; drives `ActiveView` + `navigate`.
- `web/src/pages/MainLayout.tsx` — extend `ActiveView` with `{kind:'review-list'; workspaceSlug}` and `{kind:'review-detail'; workspaceSlug; id}`; render `WorkspaceTabs` in the main-pane header; render `ReviewList`/`ReviewDetail` for the new kinds.

## Data flow

```
WorkspaceTabs ──click Reviews──▶ ActiveView{review-list} ──▶ ReviewList ──listReviews(ws)──▶ queue
        row click ──▶ navigate /{owner}/{ws}/reviews/{id} ──▶ ActiveView{review-detail}
                          └──▶ ReviewDetail ──getReview(ws,id)──▶ DiffView(diffs)
        Approve/Reject ──reviewAction(ws,id,action)──▶ back to ReviewList
```

No backend changes — the workspace-scoped review endpoints already exist (PR #25 base).

## Error handling

- List/detail fetch errors → inline error message + retry; empty queue → friendly empty state ("No pending reviews").
- `reviewAction` failure (e.g. 403 not owner/writer) → toast/message, no state change.
- `DiffView` rendered-mode exception → automatic fallback to source mode (never blank).

## Testing

- `DiffView` (pure) — unit tests: new file (all insert), deleted file (all delete), modified (mixed), and source-mode word diff. Vitest + Testing Library.
- `ReviewList`/`ReviewDetail` — component tests with mocked `api.ts` (queue renders rows; approve calls `reviewAction` and transitions).
- Manual smoke: team workspace → submit a page → Reviews·1 → open → see rendered diff → Approve → queue empties.

## New dependencies

`diff`, `marked`, `htmldiff-js`, `dompurify` (+ `@types/diff`, `@types/dompurify` as needed). All small, widely used.

## Known follow-ups (out of scope)

Comments (#13), `changes_requested` + resubmit (#19), notifications, pagination/filter, full migration of Wiki/Sources into the tab shell ("route 2").
