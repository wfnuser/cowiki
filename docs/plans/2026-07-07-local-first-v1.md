# Local-First V1 — direction and scope

> Status: proposal, 2026-07-07. Written after the client/backend split (#116). Supersedes the "hosted backend + thin desktop shell" reading of the Tauri work: the desktop client's next milestone is *local-first*, not a prettier remote client.

## Thesis

cowiki should be **usable alone, offline, on one machine** before it is a team product. The collaboration loop (submit → review → merge) stays the heart of the product, but it must sit on top of a local workspace that is fast and self-contained — a user's first five minutes must not require Postgres, a server, or an account.

Positioning stays as decided: **cowiki is the collaboration/memory workbench**. Agent execution is not our job — compile/ingest jobs dispatch through [Steward](https://github.com/wfnuser/Steward), the shared local runtime daemon (also used by [TagAny](https://github.com/wfnuser/TagAny)). The in-repo agent-harness experiments (`feat/agent-compile-pipeline`) are reference material, not the path forward.

## What changes

### 1. Local workspace, SQLite, no Postgres requirement

- Local mode stores metadata/index in **SQLite**; the wiki content itself remains a plain **local git repo** (as today).
- Postgres + pgvector remains the hosted/team backend. This means a storage abstraction over the metadata layer with two implementations — acceptable cost; the schemas are already small after the split.
- Search in local mode: FTS first (SQLite FTS5). Vector search is not a V1 blocker; add a local embedding index later if FTS proves insufficient.

### 2. Desktop client becomes offline-capable

- The Tauri shell (#116) currently only does OAuth loopback login against a hosted backend. Next: open a **local workspace directory** without any server — read, browse, edit, commit.
- Editor: build on the CodeMirror 6 editor (#96) toward comfortable live-preview markdown: wikilink autocomplete + hover preview, backlinks panel, quick file switcher. Target is "clean and fast", not feature-parity with any existing note app — our differentiation is the review/provenance loop, not editor bells.

### 3. Compile/ingest via Steward

- cowiki (backend or desktop client in local mode) calls Steward's loopback HTTP API to run compile jobs on the user's own agents (claude/codex/…), using the user's own subscriptions/keys.
- cowiki supplies the prompt and workspace; Steward owns process, session, isolation. SSE progress is relayed into the existing compile-drawer UI.
- Sequencing: land after TagAny has exercised the Steward API, unless local compile becomes urgent first.

### 4. Sync = async, review-gated; no CRDT

"联机" for V1 means: local git workspace ↔ shared remote via push/pull, with conflicts resolved through the existing submit/review/merge model. Real-time co-editing (CRDT) is explicitly **not planned** — the personal-space-then-submit philosophy makes character-level live merge a non-goal. Write this down so it doesn't get re-litigated implicitly.

### 5. Memory provider for TagAny (V1.5+)

- Expose retrieval ("relevant reviewed pages for this task text") and **memory-candidate submission** (TagAny proposes; cowiki review decides) endpoints.
- Anticipate volume: agent-generated candidates will exceed human review bandwidth. Plan a triage tier (auto-filter low-value candidates before they reach a human queue) — this is our own "review is the new bottleneck" thesis applied to ourselves.

## Explicit non-goals now

- CRDT / realtime cursors.
- Editor parity race with dedicated note apps.
- Owning agent orchestration UI (that's TagAny).
- Splitting the storage/memory infra into its own project — revisit only when a second consumer needs it directly.

## Open questions

1. Does the desktop local mode talk to a slimmed local backend process, or embed the logic in-process (Tauri Rust side)? Leaning: embed; keep the hosted backend a separate deployment.
2. Migration story between local SQLite workspace and hosted Postgres workspace (adopt-a-local-workspace-into-a-team flow).
3. Where the Steward client lives (shared crate vs per-repo thin client).
