# Derived indexes are rebuildable from Git

## Status

Accepted.

## Context

cowiki has one canonical storage layer and several derived storage layers:

| Layer | Role | Source of truth |
| --- | --- | --- |
| Git repository | `sources/*`, `wiki/*.md`, `.cowiki/state.json`, branch history | Yes |
| PostgreSQL + pgvector | page metadata, search rows, embeddings | No |
| Future graph store | entities, facts, page/entity links | No |

This raises a reset and recovery question: when a user resets a page, branch, or workspace to an earlier Git state, should cowiki also maintain coordinated snapshots for PostgreSQL and graph data?

## Decision

Do not build cross-layer snapshot infrastructure.

Git is the source of truth. PostgreSQL search rows, vector embeddings, and future graph data are derived indexes. When Git content is reset or otherwise rewritten, derived rows should be deleted or marked stale and rebuilt from the Git tree.

The default rebuild scope is the workspace/branch being reset. Page-level rebuilds are allowed as an optimization only when the affected derived data is known to be local to that page.

## Consequences

Reset and repair flows should follow this shape:

1. Move Git to the target state.
2. Delete or mark stale the derived rows for the affected workspace/branch.
3. Re-read wiki files from Git.
4. Recompute page metadata and embeddings.
5. Rebuild graph facts/entities when that layer exists.

This keeps recovery simple and avoids a second source of truth. A stale or corrupt search/graph index is repaired by rebuilding, not by restoring an index snapshot.

The trade-off is rebuild latency. Embeddings require API calls, and future graph extraction may be slower. That cost is acceptable for reset/repair operations because they are expected to be infrequent and can run as background work.

## Search Scope

Search indexes must preserve workspace and branch scope. A physical database index may cover many rows, but query results must be filtered to the active workspace and branch. Per ADR 0004, the view a user searches is their own `user/{id}` branch tree (kept current by rebasing onto `main`) — there is no overlay of multiple branches to merge at query time.

This follows the same architectural principle as Git-backed systems such as GitLab: repository content is canonical, and search indexes are eventually consistent derived data that can be rebuilt from repository state.

## Open Questions

- Whether graph extraction should cache intermediate results per Git object or commit to reduce rebuild cost.
- Whether reset UI should expose page-level rebuilds, workspace-level rebuilds, or only workspace-level rebuilds at first.
- Index lifecycle when many `user/{id}` branches are indexed alongside `main` (ADR 0004 settled the *view* — search reads the user's branch tree — but per-branch index storage/GC strategy is open).

## Related

- Issue #48: research conclusion for delete-and-rebuild instead of snapshots.
- Issue #26: workspace-scoped page metadata and vector search.
- Issue #44: branch-aware search and cross-branch merge strategy.
- Issue #15: compile system optimization.
