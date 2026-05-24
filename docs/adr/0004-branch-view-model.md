# Branch View Model — unresolved design problem

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

Not decided yet. Current implementation uses approach A (overlay) as a temporary solution. The search indexing problem needs to be solved before adding PostgreSQL FTS.

## Impact

- Search should index the user's "effective view" (overlay of main + their drafts)
- Avoid surfacing duplicate versions of the same page
- Need a rebase/sync mechanism for stale drafts
