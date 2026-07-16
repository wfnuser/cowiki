# RALPLAN-DR: cowiki CLI Tool — Consensus Planning

> **Historical document — superseded.** The standalone CLI described here was
> retired by issue #74. Local Agents use read-only MCP for retrieval and edit
> Space Markdown directly; see [`cowiki-space`](../../skills/cowiki-space/SKILL.md).

**Date:** 2026-05-22  
**Mode:** SHORT  
**Plan saved to:** `docs/plans/2026-05-22-cowiki-cli-ralplan.md`

---

## 1. Principles (3-5 guiding design axioms)

1. **API-as-contract only** — The CLI talks exclusively to the cowiki HTTP API. It must not depend on `cowiki_core` or `cowiki_db` crates. The server is the single source of truth; the CLI is a thin, stateless terminal frontend.

2. **Terminal-first UX** — Output is human-readable by default (colored tables, formatted diffs, progress spinners). Machine-readable output (`--json`) is a secondary concern for scripting/piping.

3. **Ergonomic defaults, discoverable flags** — Common workflows work with minimal typing (`cowiki search "topic"`). Branch selection, output format, and other options are exposed via `--flag` or subcommand args, with `--help` always useful.

4. **Stateless except for auth** — The CLI stores only `~/.config/cowiki/credentials` (API key + server URL). No local cache, no offline mode. Every command is a single HTTP round-trip (or a small, bounded sequence).

5. **Command parity with Web UI** — Every user-facing operation in the Web UI must have a CLI equivalent. Admin-only or internal endpoints (e.g., `/api/usage`) are out of scope for MVP.

---

## 2. Decision Drivers (top 3)

| # | Driver | Weight | Rationale |
|---|--------|--------|-----------|
| 1 | **API contract stability** | Highest | The CLI lives or dies on the server's HTTP API. Route shapes, auth mechanism, and error formats must be treated as the canonical spec. Any drift breaks the CLI silently. |
| 2 | **Developer UX speed** | High | The primary audience is human developers who want to ingest/compile/submit/review from the terminal. Latency, keystroke count, and output clarity are the measure of success. |
| 3 | **Maintainability & low coupling** | Medium | The CLI must not pull in the full server dependency tree. A standalone crate with ~5 direct dependencies keeps compile times low and allows independent releases. |

---

## 3. Viable Options Matrix

### Decision A: CLI Crate Structure & Command Organization

#### Option A1: Flat subcommands via clap derive (`cowiki <verb> <args>`)

```
cli/
├── Cargo.toml
└── src/
    ├── main.rs          # clap derive enum + dispatch
    ├── commands/
    │   ├── mod.rs
    │   ├── ingest.rs
    │   ├── compile.rs
    │   ├── submit.rs
    │   ├── review.rs
    │   ├── search.rs
    │   ├── read.rs
    │   ├── write.rs
    │   ├── list.rs
    ├── client.rs        # HTTP client (reqwest)
    ├── auth.rs          # Credential store
    └── output.rs        # Formatters (table, colored, json)
```

**Commands:**
```
cowiki ingest <file|url>       # POST /api/ingest
cowiki ingest <file|url>       # POST /api/ingest
cowiki compile                 # POST /api/compile
cowiki submit <slug...>       # POST /api/submit
cowiki review [list|show|approve|reject]  # GET/POST /api/reviews
cowiki search <query>          # GET /api/search
cowiki read <slug>             # GET /api/pages/{slug}
cowiki write <slug>            # POST /api/pages (reads body from stdin or $EDITOR)
cowiki list                    # GET /api/pages
```

**Pros:** Flat, intuitive, one word per action. Matches git-style UX that developers expect.  
**Cons:** `review` becomes a sub-subcommand tree (`review list`, `review show <id>`, `review approve <id>`, `review reject <id>`), slightly inconsistent with flat verbs.

#### Option A2: Noun-first subcommands (`cowiki <resource> <action>`)

```
cowiki pages list
cowiki pages read <slug>
cowiki pages write <slug>
cowiki sources ingest <file|url>
cowiki compile run
cowiki submit create <slug...>
cowiki review list
cowiki review show <id>
cowiki review approve <id>
cowiki review reject <id>
cowiki search <query>
```

**Pros:** RESTful mental model, naturally extendable (e.g., `cowiki pages delete` later).  
**Cons:** More typing (`cowiki pages read` vs `cowiki read`). Verbose for the 80% case (read/write/search). Feels bureaucratic compared to `git`-style flat commands.

#### → Recommendation: **Option A1 (Flat subcommands)**

Git's success proves developers prefer `git log` over `git commits list`. The flat model is faster to type and matches the "tool" feel. `review` as the only nested subcommand is acceptable because review is inherently multi-step.

---

### Decision B: HTTP Client Layer

#### Option B1: Direct `reqwest` with hand-written request/response types

- Define `struct IngestRequest`, `struct SearchResult`, etc. in the CLI crate.
- Manually serialize/deserialize with `serde_json`.
- No code generation, no shared types with server.

**Pros:** Zero toolchain complexity. Full control over error handling. Compiles fast.  
**Cons:** Must manually keep request/response types in sync with server. Duplication of type definitions (server already defines them).

#### Option B2: Extract shared API types to a `cowiki_api_types` crate

- Create `crates/api-types/` with only the serde structs (no logic, no db, no core deps).
- Both `server` and `cli` depend on it.
- Single source of truth for `IngestRequest`, `SearchResult`, `PageResponse`, etc.

**Pros:** Type-safe contract. Server and CLI can't drift. Single `cargo check` catches mismatches.  
**Cons:** Adds a workspace member. Requires refactoring server routes to re-export from api-types. Moderate up-front cost.

#### Option B3: OpenAPI/Swagger code generation

- Generate Rust client from an OpenAPI spec (or derive spec from axum routes via `utoipa`).

**Pros:** Industry standard. Auto-generated docs.  
**Cons:** Massive toolchain overhead for 20 endpoints. Generated code is usually ugly. Overkill for this scale.

#### → Recommendation: **Option B1 (Direct reqwest) for MVP, with migration path to B2**

For MVP with 9 commands, hand-written types are the pragmatic choice. The API surface is small (~15 request/response types), and the duplication is manageable. If the API grows beyond ~30 endpoints, extract to `api-types`. The key risk (drift) is mitigated by integration tests that hit the real server.

---

### Decision C: Async Runtime Strategy

#### Option C1: `#[tokio::main]` — full async main

```rust
#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Search { query, branch } => commands::search::run(&client, query, branch).await,
        // ...
    }
}
```

**Pros:** Natural fit for `reqwest`. Every command is async anyway (HTTP). Single runtime, simple mental model.  
**Cons:** `tokio` is a heavy dependency (~50 crates in tree). Compile time impact.

#### Option C2: `tokio` with `flavor = "current_thread"` — lightweight runtime

```rust
fn main() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async { /* ... */ });
}
```

**Pros:** Lighter footprint than multi-threaded runtime. Sufficient for CLI (one request at a time).  
**Cons:** Still pulls in tokio. Slightly more boilerplate.

#### Option C3: `ureq` (blocking HTTP) — no async at all

- Use `ureq` (synchronous HTTP client) instead of `reqwest`.
- No tokio dependency.

**Pros:** Fastest compile times. Simplest mental model.  
**Cons:** Cannot do parallel requests (e.g., search + list simultaneously). `ureq` is less feature-rich (no HTTP/2, fewer TLS options). Future-proofing: if the CLI ever needs concurrent requests, a rewrite is needed.

#### → Recommendation: **Option C1 (`#[tokio::main]` with multi-thread)**

`reqwest` is the de facto Rust HTTP client. `tokio` is already in the workspace dependency tree (server uses it). The compile-time cost is amortized because `target/` is shared across workspace members. Multi-thread runtime adds no meaningful overhead for a CLI, and it keeps the door open for concurrent operations (e.g., parallel ingest of multiple URLs in a future release).

---

### Decision D: Output Formatting

#### Option D1: `colored` + `tabled` — simple, focused crates

- `colored` for ANSI color in terminal output.
- `tabled` for table formatting (list, search results, review list).
- Manual formatting for special views (diff display in review).

**Pros:** Lightweight. Each crate does one thing well.  
**Cons:** Two separate APIs to learn. Table customization is limited compared to `comfy-table`.

#### Option D2: `clap` + `comfy-table` + `indicatif` — rich terminal UI

- `comfy-table` for styled tables with borders, padding, alignment.
- `indicatif` for progress spinners (compile is a long-running operation).
- `bat`-style syntax highlighting for page body output (via `syntect`).

**Pros:** Polished, "premium" terminal experience. Spinners give feedback during long operations.  
**Cons:** More dependencies. `syntect` adds ~2MB to binary and slower compiles.

#### Option D3: `--json` only, pipe to `jq`

**Pros:** Zero formatting code. Users compose with `jq`, `fx`, etc.  
**Cons:** Terrible DX for the primary audience (human developers). Defeats the "terminal-first" principle.

#### → Recommendation: **Option D2 (Rich terminal UI)**

`comfy-table` + `indicatif` + `colored` hits the sweet spot. `indicatif` is critical because `cowiki compile` can take 10-60 seconds (LLM calls) and users need feedback. Skip `syntect` for MVP — just print markdown body as plain text with `---` separators.

---

## 4. Recommended Approach

| Decision | Choice |
|----------|--------|
| CLI Structure | **A1** — Flat subcommands (`cowiki ingest`, `cowiki search`, `cowiki review {action}`) |
| HTTP Client | **B1** — Direct `reqwest` with hand-written types (migrate to B2 if the API grows) |
| Async Runtime | **C1** — `#[tokio::main]` with multi-thread runtime |
| Output Formatting | **D2** — `comfy-table` + `indicatif` + `colored` |

**Rationale:** This combination optimizes for the #1 and #2 decision drivers (API contract stability, developer UX speed) while keeping the implementation simple enough for MVP delivery. The only "premium" spend is `indicatif` for spinner feedback — worth it because `compile` and `submit` are long-running.

---

## 5. Implementation Phases

### Phase 1: Skeleton + Config (cli/ scaffolding, .env, config.toml)

**Goal:** Config loads from .env / config.toml. `cowiki --help` shows all commands.

**Files to create:**
- `cli/Cargo.toml` — dependencies: `clap`, `reqwest`, `serde`, `serde_json`, `tokio`, `dirs`, `colored`, `comfy-table`, `indicatif`
- `cli/src/main.rs` — clap derive enum, top-level dispatch
- `cli/src/auth.rs` — `Credentials { server_url, api_key }`, load/save to `~/.config/cowiki/credentials.json`, `COWIKI_API_KEY` env var override
- `cli/src/client.rs` — `CowikiClient` struct wrapping `reqwest::Client`, base URL from credentials, `Authorization: Bearer` header injection
- `cli/src/output.rs` — `print_table()`, `print_json()`, spinner helpers
- `cli/src/types.rs` — All request/response structs (duplicated from server routes for now)

**Acceptance criteria:**
- [ ] `cargo build -p cowiki-cli` succeeds
- [ ] `cowiki --help` shows all commands with descriptions
- [ ] `COWIKI_API_KEY=xxx cowiki search "test"` works via .env
- [ ] `cowiki --help` lists all 9 commands with descriptions

**Workspace change:**
```toml
# Cargo.toml
members = ["crates/server", "crates/core", "crates/db", "cli"]
```

---

### Phase 2: Read-Only Commands (search, read, list)

**Goal:** Users can browse wiki content from the terminal.

**Files to create/modify:**
- `cli/src/commands/search.rs` — `GET /api/search?q=&branch=&limit=`
- `cli/src/commands/read.rs` — `GET /api/pages/{slug}?branch=`
- `cli/src/commands/list.rs` — `GET /api/pages?branch=`

**Acceptance criteria:**
- [ ] `cowiki search "quantum computing"` returns a formatted table of results with slug, title, similarity score
- [ ] `cowiki search "quantum computing" --json` outputs raw JSON
- [ ] `cowiki search "quantum computing" --branch user/xxx` scopes to personal branch
- [ ] `cowiki read "quantum-computing"` prints the full page body to stdout
- [ ] `cowiki read "quantum-computing" --branch user/xxx` reads from personal branch
- [ ] `cowiki list` prints a table of all pages on main branch
- [ ] `cowiki list --branch user/xxx` lists personal branch pages
- [ ] All commands handle 404, 500, and network errors with human-readable messages

---

### Phase 3: Write Commands (ingest, write)

**Goal:** Users can create content from the terminal.

**Files to create:**
- `cli/src/commands/ingest.rs` — `POST /api/ingest` with source_type detection
- `cli/src/commands/write.rs` — `POST /api/pages` with body from stdin or $EDITOR

**Acceptance criteria:**
- [ ] `cowiki ingest --type text "My content here"` ingests text and returns filename + hash
- [ ] `cowiki ingest --type url "https://example.com/article"` fetches and ingests URL
- [ ] `cowiki ingest --type file ./notes.md` reads local file and ingests
- [ ] `cowiki ingest` with no args: auto-detect (if arg is URL → url, if file exists → file, else → text)
- [ ] `cowiki write "my-page"` opens $EDITOR with template, saves on exit
- [ ] `echo "# Hello" | cowiki write "my-page" --stdin` writes from pipe
- [ ] Both commands respect `--branch` flag

---

### Phase 4: Workflow Commands (compile, submit)

**Goal:** Full ingestion → compilation → submission pipeline from terminal.

**Files to create:**
- `cli/src/commands/compile.rs` — `POST /api/compile` with spinner during LLM compilation
- `cli/src/commands/submit.rs` — `POST /api/submit` with duplicate warning display

**Acceptance criteria:**
- [ ] `cowiki compile` shows a spinner while compilation runs
- [ ] `cowiki compile` prints compiled page slugs + titles on success, skipped count
- [ ] `cowiki compile --branch user/xxx` compiles from personal branch
- [ ] `cowiki submit "page-1" "page-2"` submits pages and prints submission ID + summary
- [ ] `cowiki submit` warns about duplicate pages with similarity scores
- [ ] `cowiki submit --yes` skips confirmation prompt

---

### Phase 5: Review Commands

**Goal:** Review and approve/reject submissions from the terminal.

**Files to create:**
- `cli/src/commands/review.rs` — `review list`, `review show <id>`, `review approve <id>`, `review reject <id>`

**Acceptance criteria:**
- [ ] `cowiki review list` shows pending submissions in a table
- [ ] `cowiki review show <id>` displays submission details + file diffs
- [ ] `cowiki review approve <id>` approves and merges to main
- [ ] `cowiki review reject <id>` rejects the submission
- [ ] Diff output is colored (green for additions, red for deletions)

---

### Phase 6: Polish + Integration Testing

**Goal:** End-to-end verification and DX polish.

**Tasks:**
- [ ] Write integration test script (`test/cli-integration.sh`) that:
  1. Starts the server (Docker Compose)
  2. Creates a test user via direct API call
  3. Runs ingest → compile → submit → review approve pipeline
  4. Verifies search returns the new page
- [ ] Add `cowiki --version` (read from `Cargo.toml` via `env!("CARGO_PKG_VERSION")`)
- [ ] Add shell completion generation (`cowiki completions bash|zsh|fish` via clap)
- [ ] Add `cowiki status` (quick health check: `GET /api/health` + user info from `GET /api/auth/me`)
- [ ] Manually test all error paths (invalid API key, server down, bad branch name)

---

## 6. Complete API Contract Reference

| CLI Command | HTTP Method | Endpoint | Auth | Request Body / Params | Response |
|-------------|-------------|----------|------|-----------------------|----------|
| `cowiki ingest` | POST | `/api/ingest` | No | `{source_type, content, filename?, branch}` | `{filename, content_hash}` |
| `cowiki compile` | POST | `/api/compile` | No | `{branch}` | `{pages: [{slug, title, summary}], skipped}` |
| `cowiki submit` | POST | `/api/submit` | No | `{branch, page_slugs}` | `{submission_id, summary, duplicates}` |
| `cowiki search` | GET | `/api/search` | No | `?q=&branch=&limit=` | `[{slug, title, summary, similarity}]` |
| `cowiki read` | GET | `/api/pages/{slug}` | No | `?branch=` | `{slug, title, summary, body, branch}` |
| `cowiki list` | GET | `/api/pages` | No | `?branch=` | `[PageMeta]` |
| `cowiki write` | POST | `/api/pages` | No | `{slug, body, branch}` | `{ok, slug}` |
| `cowiki review list` | GET | `/api/reviews` | No | — | `[Submission]` |
| `cowiki review show` | GET | `/api/reviews/{id}` | No | — | `{submission, diffs}` |
| `cowiki review approve` | POST | `/api/reviews/{id}` | No | `{action: "approve"}` | `{ok}` |
| `cowiki review reject` | POST | `/api/reviews/{id}` | No | `{action: "reject"}` | `{ok}` |

**Auth note:** Current server routes do NOT require auth on most endpoints (ingest, pages, compile, submit, search, review). Only workspaces and `/api/auth/me` require `Authorization: Bearer <key>`. The CLI should still send the key when available (no harm), but must not fail if the user hasn't logged in yet.

---

## 7. Risk Areas & Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| **Server API changes silently** — route shapes or response fields change, CLI breaks | Medium | High | Pin CLI version to server version in CI. Add `cowiki --check-compat` that calls `/api/health` and verifies expected response shape. Integration test in CI. |
| **Auth model mismatch** — Server adds required auth to currently-open endpoints | Medium | Medium | CLI always sends `Authorization` header when credentials exist. Graceful fallback if 401 received: print message suggesting to check API key. |
| **Large page bodies** — `cowiki read` on a 50KB wiki page floods the terminal | High | Low | Pipe to `$PAGER` by default (respect `$PAGER`/`less`). `--no-pager` flag to disable. |
| **Compile timeout** — LLM compilation takes >60s, CLI appears hung | Medium | Medium | `indicatif` spinner with elapsed time. Configurable timeout via `--timeout` flag (default 120s). `reqwest::ClientBuilder::timeout()`. |
| **Binary size bloat** — tokio + reqwest + clap produce a 15MB+ binary | Low | Low | Acceptable for a developer tool. Use `opt-level = "s"` and LTO in release profile. |
| **Branch name complexity** — Users must type `user/723666c1-...` manually | High | Medium | Add `cowiki whoami` to show current user's branch. `--branch` flag accepts `@me` shorthand resolved to `user/<id>` via `/api/auth/me`. |

---

## 8. Dependency Budget

```toml
[dependencies]
clap = { version = "4", features = ["derive"] }
reqwest = { version = "0.12", features = ["json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
dirs = "5"
colored = "2"
comfy-table = "7"
indicatif = "0.17"
```

Total: **8 direct dependencies**. All are well-maintained, widely-used crates.

---

## 9. ADR (Architecture Decision Record)

**Decision:** Build cowiki CLI as a standalone Rust binary using clap + reqwest, communicating exclusively via HTTP with the cowiki server.

**Drivers:**
1. API contract stability (the server is the single source of truth)
2. Developer UX speed (terminal-first, minimal keystrokes)
3. Maintainability (no coupling to core/db crates)

**Alternatives considered:**
- **Embed core lib directly:** Rejected — would couple CLI to server version, defeat independent releases, and pull in heavy dependencies (sqlx, pgvector, git2).
- **Python/Go CLI:** Rejected — Rust workspace consistency, shared type definitions in future, and `clap` + `reqwest` are best-in-class for CLI + HTTP.
- **Generated OpenAPI client:** Rejected — overkill for 12 endpoints, adds codegen toolchain complexity.

**Why chosen:** Flat clap subcommands + reqwest + tokio is the Rust ecosystem's standard path for CLI tools. It optimizes for speed of development and runtime performance while keeping the door open for future extraction of shared API types.

**Consequences:**
- Must manually keep request/response types in sync with server (mitigated by integration tests).
- Compile time includes tokio + reqwest (~30s from clean on modern hardware; acceptable).
- No offline capability (acceptable; CLI is explicitly a thin client).

**Follow-ups:**
- If API grows beyond ~30 endpoints, extract `crates/api-types/` shared crate.
- If auth becomes mandatory on all endpoints post-MVP, require API key configuration.
- Shell completion generation via clap's built-in support.

---

## RALPLAN-DR Confirmation

**Plan ready for review.** This is a SHORT-mode consensus plan. The 6-phase breakdown has concrete file paths and acceptance criteria. The 4 architecture decisions are settled with clear rationale.

**Does this plan capture the intent?**
- `proceed` — Hand off to @executor for implementation
- `adjust <area>` — Return to interview on a specific decision
- `restart` — Discard and re-plan from scratch
