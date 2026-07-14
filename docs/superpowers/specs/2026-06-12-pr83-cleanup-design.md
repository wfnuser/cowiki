# PR #83 Cleanup Design

## Context

PR #83 is a TypeScript CLI rewrite with multi-directory wiki support. The owner's review
identified 4 cleanup items before merge. This spec covers all 4.

## Issue 1: Workspace-scoped search

### Current state

- **CLI** (`cli/src/client.ts:175`): `CowikiClient.search()` calls the global
  `/api/search?q=...&limit=...&branch=...` — no workspace context is passed.
- **Server** (`main.rs:302`): Only has a global `/api/search` route mounted.
  `search.rs` contains a TODO about workspace scoping.
- **Search command** (`commands/search.ts`): Does not require `-w` / `--workspace`.

`cowiki_db::pages::find_similar` already accepts `workspace_slug: Option<&str>` —
passing `Some(ws)` scopes the query; `None` searches across all workspaces.

### Design

Follow the dev branch's existing workspace-scoped route pattern (used by pages, ingest,
submit, reviews, etc.).

**Server — `crates/server/src/routes/search.rs`**: Add a `search_ws` handler that
calls `require_membership` (ViewContent permission) and passes `Some(&ws_slug)` to
`find_similar`.

**Server — `crates/server/src/main.rs`**: Replace the global `/api/search` route with
`/api/workspaces/{ws_slug}/search`, routing to `search_ws`.

**CLI — `cli/src/client.ts`**: Add `ws: string` parameter to `search()`, change URL
to `/api/workspaces/{ws}/search`.

**CLI — `cli/src/commands/search.ts`**: Call `requireWorkspace(globalOpts.workspace)`,
pass the workspace slug to `client.search()`.

## Issue 2: Canonical title rule

### Current state

`pages.rs:88-94` — `require_page_title()` calls `parse_frontmatter()` which falls
back to extracting a `# Heading` from the markdown body. A page without
`frontmatter.title` can pass write validation as long as it has a heading.

Heading/slug fallback is intentionally used for *display* in `get_page_ws` and
`list_pages_ws` (so legacy content renders with a title) — this should stay.

### Design

Add `parse_frontmatter_strict()` that only checks the YAML frontmatter block for a
non-empty `title:` field. No heading/slug fallback. Use it in `require_page_title()`.

Keep `parse_frontmatter()` with its heading fallback for display-only call sites.

## Issue 3: CLI/skill docs

### Current state

- `cli/README.md` references `cargo build`, `~/.config/cowiki/config.toml` (Rust-era docs).
- `cli/skills/cowiki-cli/SKILL.md` references unpublished `npm install -g @cowiki/cli`,
  defaults to `localhost:3000`, and asks users to paste API keys into chat.
- `cli/skills/cowiki-cli/config.md` defaults to `localhost:3000`.

### Design

**`cli/README.md`**:
- Dev install: `cd cli && npm install && npm run build && npm link`
- npm path (when published): `npm install -g @cowiki/cli`
- Config: `~/.cowiki-cli/.env`, env vars `COWIKI_BASE_URL` / `COWIKI_API_KEY`
- Testing: `npm test`

**`cli/skills/cowiki-cli/SKILL.md`**:
- Step 2: Both dev (`npm install && npm run build && npm link`) and npm
  (`npm install -g @cowiki/cli`) install paths.
- Step 3: Replace "paste your API key into chat" flow with asking the user to run
  `cowiki setup`. Remove `--server http://localhost:3000` default. Add note about
  `COWIKI_BASE_URL` / `COWIKI_API_KEY` env vars as an alternative config method.

**`cli/skills/cowiki-cli/config.md`**:
- Document `cowiki setup` as the primary path.
- Don't default to `localhost:3000` without noting it's for local dev only.

## Issue 4: Remove broken API tests

### Current state

`crates/server/tests/pages_api_tests.rs` contains 8 `#[ignore]` integration tests
with 3 bugs:

1. `register_user` parses `json["id"]` instead of `json["user"]["id"]` — doesn't
   match the actual register response shape.
2. All tests write to `"main"` branch, but `require_own_branch()` rejects writes
   to anything other than `user/{id}`.
3. Tests assert `json["dir"]` but the write handler returns `json["path"]`.

### Design

Delete `crates/server/tests/pages_api_tests.rs` entirely.
