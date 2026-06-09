# Draft — Epic C: Decompose `MainLayout`

> Status: **draft for review** (codex). Maintainability refactor only — **no visual/interaction change**.

## Problem
`web/src/pages/MainLayout.tsx` (~1176 lines, ~35 `useState`) owns workspace/page/source/review/member state, all data fetching, modals, and manual `ActiveView` routing that can drift from the URL. It's the host of most frontend stale-closure, swallowed-error, and perf nits found in review.

## Target shape
Extract data + mutations into hooks; `MainLayout` becomes a thin shell that composes them and renders. **No markup/style changes** in this Epic.

```
web/src/
  hooks/
    useAsyncResource.ts   // shared fetch/loading/error/cancel primitive (do FIRST)
    useWorkspaces.ts      // list + active workspace + create/rename/delete/join
    usePages.ts           // per-workspace page tree + selection + write/folder
    useSources.ts         // per-workspace sources + ingest
    useReviews.ts         // pending count + list (wraps listReviews)
    useWorkspaceDialogs.ts// modal open/close state (create/rename/ingest/...)
  pages/MainLayout.tsx     // compose hooks + render; ActiveView routing stays here
```

### `useAsyncResource` (the shared primitive — replaces ~24 ad-hoc fetch blocks)
```ts
type AsyncResource<T> = { data: T | null; loading: boolean; error: string | null; reload: () => void };

function useAsyncResource<T>(
  fetcher: () => Promise<T>,
  deps: React.DependencyList,
): AsyncResource<T> { /* setX(null) -> fetch -> cancelled flag -> catch setError, once */ }
```
Every `useXxx` hook builds on this, so loading/error/cancel semantics are uniform (and consume Epic A2's `{code}` error envelope once that lands).

## Sequencing (each step ships green, no behavior change)
1. `useAsyncResource` + migrate the simplest reads (reviews, sources) to it.
2. `useWorkspaces` (the spine) — move workspace list + active-workspace state.
3. `usePages` / `useSources` / `useReviews`.
4. `useWorkspaceDialogs` (mechanical state move).
5. Fix the review-surfaced nits as they pass through: `loadWorkspaces` stale-closure deps, non-memoized search IIFE (`useMemo([searchQuery, spacePages, workspaces])`), user-branch-first selection 404 waterfall.

## Open decisions for review
1. **Routing ownership.** Keep manual `ActiveView` in `MainLayout`, or move to real nested routes (`react-router`)? The review flagged `ActiveView` can drift from the URL. Proposal: keep `ActiveView` this Epic (no behavior change), file a follow-up for real routes.
2. **Hook granularity.** Is 6 hooks the right cut, or merge `usePages`+`useSources` into one `useWorkspaceContent`? (They share the active-workspace + branch.)
3. **Where does `activeWorkspace` live** — in `useWorkspaces` (and others receive it as an arg), or a small context? Proposal: `useWorkspaces` returns it; pass down explicitly (no context yet).

## Notes
- Pairs with Epic B (`request<T>()`): `useAsyncResource` and `request<T>()` are complementary — `request<T>` is the transport, `useAsyncResource` the React state wrapper.
- Will conflict heavily with anything else editing `MainLayout`; should land as its own focused stack after the in-flight PRs settle.
