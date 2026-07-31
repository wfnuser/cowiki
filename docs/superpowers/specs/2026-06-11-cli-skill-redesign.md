# cowiki CLI Rewrite & Agent-First SKILL.md Redesign

> **Historical document — superseded.** The standalone CLI and its API-key
> workflow were retired by issue #74. Current guidance lives in the
> [`cowiki-space` skill](../../../skills/cowiki-space/SKILL.md).

**Date:** 2026-06-11  
**Issue:** [#74](https://github.com/wfnuser/cowiki/issues/74)  
**Status:** design complete

## Overview

Rewrite the cowiki CLI from Rust to TypeScript and redesign the SKILL.md so that AI agents can autonomously install, register, and configure the CLI by simply reading a hosted skill file. Inspired by Avoko (`https://avoko.ai/participant/skill.md`) and Moltbook (`https://www.moltbook.com/skill.md`).

### User Flow

```
1. User pastes into their agent:
   "Read https://cowiki.app/skill.md and follow the instructions to join cowiki"

2. Agent autonomously:
   - Checks Node.js 18+
   - npm install -g cowiki-cli
   - cowiki login (opens browser, user completes GitHub OAuth)
     → Web frontend displays the API key
     → User pastes the key back into the terminal
     → CLI writes it to .env
   - cowiki list (verifies connectivity)
   - Downloads local skill bundle for offline use

3. Done — agent is ready for wiki operations. Note: the human must
   complete the browser OAuth step; the agent cannot do this itself.
```

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Language | TypeScript + Commander.js | Matches frontend stack (Next.js/TS), mature CLI ecosystem |
| Distribution | npm (`cowiki-cli`) | Standard Node.js toolchain, `npm link` for local dev |
| Configuration | `.env` only | Simpler than config.toml + .env + env vars; CLI flag > env var > .env |
| Auth for `login` | Browser OAuth → user pastes key | Only the initial login needs human OAuth; subsequent key management uses the stored API key |
| Registration | Web UI (user does it) | No claim flow needed; different agents use different keys |
| Skill hosting | `https://cowiki.app/skill.md` | Public URL agent can fetch directly |

## SKILL.md Agent Execution Flow

```
agent reads https://cowiki.app/skill.md
  │
  ├─ Step 1: Environment Check
  │    check: node --version >= 18 && npm --version
  │    fail → tell user to install Node.js 18+
  │
  ├─ Step 2: Install CLI
  │    check: cowiki --version
  │    fail → npm install -g cowiki-cli
  │
  ├─ Step 3: Login (get API key via OAuth)
  │    cowiki login
  │    opens browser → user GitHub OAuth → web displays API key
  │    user pastes key into terminal → CLI writes to .env
  │
  ├─ Step 4: Verify
  │    cowiki list → confirm connectivity
  │
  └─ Step 5: Install Local Skill Bundle
       downloads SKILL.md + sub-skill files to local skills directory
       enables offline agent operation
```

Each step has an idempotent "Done already?" check so re-running is safe.

### SKILL.md Content Sections

The hosted `skill.md` follows this structure (modeled on Avoko's format):

- **YAML frontmatter** — name, version, homepage, description
- **About cowiki** — what the platform is, what the agent's role is
- **Before You Start** — ask owner for consent
- **Step 1–5** — each with check command, action commands, and verification
- **Security Notes** — never leak API keys, only send to cowiki.app domains
- **Skill Files Table** — URLs of all files to download for local bundle

## CLI Command Structure

```
cowiki
├── setup                      [NEW] Interactive .env configuration
├── login                      [NEW] GitHub OAuth login, stores API key in .env
│
├── key                        [NEW] API key management (for subsequent use, not initial setup)
│   ├── generate --name        Generate an additional API key (e.g., for another agent)
│   ├── list                   List all keys
│   └── revoke <id>            Revoke a key
│
├── ingest                     [MIGRATED] Add source document (url/text/file)
├── compile                    [MIGRATED] Compile sources into wiki pages
├── write                      [MIGRATED] Create or edit a page
├── search                     [MIGRATED] Semantic search
├── read                       [MIGRATED] Read a page (with pager)
├── list                       [MIGRATED] List pages in a workspace
├── workspaces                 [MIGRATED] List available workspaces
├── submit                     [MIGRATED] Submit pages for review
└── review                     [MIGRATED] Review operations
    ├── list                   List pending reviews
    ├── show <id>              Show review with diff
    ├── approve <id>           Approve a review
    └── reject <id>            Reject a review
```

### Global Flags (preserved)

| Flag | Description |
|------|-------------|
| `--server <URL>` | Override server URL |
| `-w, --workspace <slug>` | Target workspace for scoped operations (required for workspace-scoped commands; see below) |
| `--json` | Machine-readable JSON output |

### Command-Specific Flags

| Flag | Applies To | Description |
|------|-----------|-------------|
| `--no-pager` | `read` | Print directly to stdout instead of using a pager |
| `--yes` / `-y` | `submit` | Skip confirmation prompt |
| `--timeout <s>` | `compile` | Timeout in seconds (default: 120) |
| `--limit <n>` | `search` | Max results to return (default: 10) |

### Migrated Command Flags

All existing Rust CLI flags are preserved except `--branch` (removed). Key flags:

| Flag | Command | Description |
|------|---------|-------------|
| `--type <url\|text\|file>` | `ingest` | Source type (default: `url`) |
| `--content <value>` | `ingest` | URL, text, or file path |
| `--all` | `submit` | Submit all pages on the branch |
| `--title <text>` | `write` | Page title |
| `--body <text>` | `write` | Page body (inline; omit for editor or stdin) |

### Workspace Behavior

`--workspace` / `-w` is **explicit only** — the CLI never auto-detects or infers a workspace. The expected workflow:

1. `cowiki workspaces` — list available workspaces with slugs and roles
2. User/agent picks a workspace slug
3. Pass `-w <slug>` on subsequent commands

Omitting `-w` on a workspace-scoped command produces an error with a hint to run `cowiki workspaces` first.

Commands **not** scoped to a workspace: `login`, `setup`, `key`, `search`, `workspaces`.

The `--branch` flag from the Rust CLI is **removed**: the branch is always `user/<id>` (derived from auth), which is the only branch a user can write to.

### Authentication Logic

| Command | Auth Method |
|---------|------------|
| `login` | Browser-based GitHub OAuth → web displays API key → user pastes into CLI → saved to `.env` |
| `key generate` | Existing API key (from `.env`) |
| `key list` | Existing API key |
| `key revoke` | Existing API key |
| All others | `COWIKI_API_KEY` from `.env` (Bearer header) |

### Configuration

**Only `.env` file** — no `config.toml`, no `~/.config/cowiki/`.

Priority: CLI flag (`--server`) > environment variable > `.env` file

```env
COWIKI_BASE_URL=http://localhost:3000
COWIKI_API_KEY=cw_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
```

`cowiki setup` writes this interactively. `cowiki login` writes the key automatically after the user pastes it. `cowiki key generate` prints the new key to stdout (the user must store it; it does not overwrite `.env`, since `.env` holds the primary key from login).

This is a **clean break** from the Rust CLI's three-method config (`.env`, shell env vars, `~/.config/cowiki/config.toml`). The TS CLI uses `.env` only. No migration of old config files — users re-run `cowiki setup` or create `.env` manually. The Rust CLI directory is removed entirely in Phase 5.

## Project Structure

```
cli/                              # New JS CLI (replaces Rust cli/)
├── package.json
├── tsconfig.json
├── tsup.config.ts                # Build config (ESM bundle)
├── vitest.config.ts              # Test runner config
├── .env.example                  # Template for local setup
├── src/
│   ├── index.ts                  # Entry point, registers all commands
│   ├── client.ts                 # HTTP client (fetch wrapper, auth header)
│   ├── config.ts                 # .env read/write
│   ├── types.ts                  # TypeScript type definitions
│   ├── output.ts                 # Output formatting (table / JSON)
│   ├── shared.ts                 # Shared helpers (requireWorkspace, etc.)
│   ├── error.ts                  # Structured error classes
│   ├── commands/
│   │   ├── setup.ts              # Interactive .env configuration
│   │   ├── login.ts              # GitHub OAuth flow
│   │   ├── key.ts                # key generate / list / revoke
│   │   ├── ingest.ts
│   │   ├── compile.ts
│   │   ├── write.ts
│   │   ├── search.ts
│   │   ├── read.ts
│   │   ├── list.ts
│   │   ├── workspaces.ts
│   │   ├── submit.ts
│   │   └── review.ts
│   └── utils/
│       ├── urlencode.ts          # RFC 3986 URL encoding
│       └── editor.ts             # External editor integration
├── skills/cowiki-cli/            # Local skill bundle (bundled in npm package)
│   ├── SKILL.md                  # Main skill file for offline use
│   ├── commands.md               # Full command reference
│   ├── config.md                 # Configuration guide
│   └── troubleshooting.md        # Common issues
└── tests/
    ├── client.test.ts
    ├── config.test.ts
    ├── error.test.ts
    ├── urlencode.test.ts
    ├── commands/
    │   ├── parsing.test.ts       # Command argument parsing
    │   ├── output.test.ts        # Table/JSON output formatting
    │   └── error-handling.test.ts # Error scenarios
    └── skill/                    # Skill flow tests (see testing design spec)
        ├── run-all.sh
        ├── layer1-agent/         # Agent understanding tests
        └── layer2-cli/           # CLI integration tests
```

## Dependencies

| Package | Purpose |
|---------|---------|
| `commander` | CLI framework (subcommands, flags, help text) |
| `chalk` | Terminal colors |
| `dotenv` | `.env` file parsing |
| `open` | Open browser for OAuth login |
| `ora` | Terminal spinners for async operations |
| `cli-table3` | Table rendering for list/search output |
| `@inquirer/prompts` | Interactive prompts (setup, write in editor mode) |

### Not Used

- No progress bars — `ora` spinners for indeterminate waits, npm shows install progress
- No backend runtime dependencies — pure HTTP client

## Build & Dev Tooling

| Tool | Purpose |
|------|---------|
| `tsup` | Bundle TS → JS (ESM output, handles shebang, tree-shaking) |
| `tsx` | Dev runner (`tsx src/index.ts` for rapid iteration) |
| `vitest` | Test runner (fast, native ESM, TS source support) |
| `typescript` | Type checking (`tsc --noEmit` in CI) |

**Target:** Node.js 18+ (matches the minimum version in SKILL.md). **Module format:** ESM (`"type": "module"` in `package.json`). **Bin entry:** `"bin": { "cowiki": "./dist/index.js" }` — tsup produces a single-file output with a Node shebang.

## Testing Strategy

| Layer | Scope | Tool | CI |
|-------|-------|------|----|
| Unit | Individual commands with mocked HTTP client | `vitest` + `msw` (or inline fetch mock) | Yes |
| Integration | End-to-end command execution against live server | `vitest` + `execa` | Optional (requires server) |
| Skill Flow | Agent SKILL.md understanding + real OAuth login chain | bash orchestrator (+ Claude Code for Layer 1) | No (manual OAuth) |
| Type | No type errors across codebase | `tsc --noEmit` | Yes |

**Unit tests** (7 files, 56 tests, all passing): argument parsing (each subcommand
accepts its flags), output formatting (table mode / JSON mode), config read/write,
HTTP client error handling (network failures, HTTP 4xx/5xx, auth failures), URL
encoding, structured error classes.

**Integration tests** cover: login flow (mocked OAuth callback), key generate → list
→ revoke lifecycle, ingest → compile → read roundtrip, search returns results,
review approve/reject.

**Skill flow tests** ([design spec](./2026-06-11-skill-flow-testing-design.md))
cover two layers: (1) LLM agent's ability to read SKILL.md and produce correct
shell commands, and (2) CLI command chain against a real local backend with
genuine GitHub OAuth (manual breakpoint). These run via `tests/skill/run-all.sh`
and require a local docker backend.

Tests that require a running server are marked with a convention (e.g., file suffix
`.integration.test.ts` or `describe.skip`) and run separately in CI when a server
is available.

## Output Formats

- **Default:** Human-friendly colored tables and text (stdout)
- **`--json`:** Machine-readable JSON for scripting and agent consumption

## API Endpoints Used

### Auth & Keys

| Endpoint | Method | Purpose | Status |
|----------|--------|---------|--------|
| `/api/auth/github` | GET | Redirect to GitHub OAuth | Existing |
| `/api/auth/github/callback` | GET | GitHub OAuth callback → redirects to frontend with `api_key` | Existing |
| `/api/auth/me` | GET | Get current user info | Existing |
| `/api/keys` | POST | Create a new API key (body: `{name}`) | Existing |
| `/api/keys` | GET | List API keys for current user | Existing |
| `/api/keys/{id}` | DELETE | Revoke an API key | Existing |

### Workspace-Scoped Operations

All page/ingest/compile/submit/review routes are scoped under `/api/workspaces/{ws_slug}/`.

| Endpoint | Method | Purpose | Status |
|----------|--------|---------|--------|
| `/api/workspaces/{ws_slug}/ingest` | POST | Add source document | Existing |
| `/api/workspaces/{ws_slug}/compile` | POST | Compile sources | Existing |
| `/api/workspaces/{ws_slug}/pages` | GET/POST | List / Write pages | Existing |
| `/api/workspaces/{ws_slug}/pages/{*slug}` | GET | Read a page | Existing |
| `/api/workspaces/{ws_slug}/submit` | POST | Submit for review | Existing |
| `/api/workspaces/{ws_slug}/reviews` | GET | List reviews | Existing |
| `/api/workspaces/{ws_slug}/reviews/{id}` | GET/POST | Show / act on review | Existing |
| `/api/workspaces` | GET | List workspaces | Existing |
| `/api/search` | GET | Semantic search (not workspace-scoped) | Existing |

### Frontend Change Required

The GitHub OAuth callback (`/api/auth/github/callback`) currently redirects to the frontend URL with `api_key` as a query parameter. For the CLI login flow, the frontend must display this key prominently after a successful first-time OAuth (e.g., a "Your API Key" card with a copy button). The user copies it and pastes it into the CLI at the `cowiki login` prompt.

## Migration Checklist

### Phase 1: Scaffold
- [x] Initialize `cli/` as TypeScript npm package
- [x] Configure tsconfig, build scripts, bin entry
- [x] Implement `src/config.ts` (`.env` only)
- [x] Implement `src/client.ts` (fetch-based HTTP client)
- [x] Implement `src/types.ts` (all type definitions)
- [x] Implement `src/output.ts` (table renderer + JSON mode)

### Phase 2: New Commands
- [x] `cowiki login` (GitHub OAuth)
- [x] `cowiki setup` (interactive .env config)
- [x] `cowiki key generate --name`
- [x] `cowiki key list`
- [x] `cowiki key revoke <id>`

### Phase 3: Migrated Commands
- [x] `cowiki ingest`
- [x] `cowiki compile`
- [x] `cowiki write`
- [x] `cowiki search`
- [x] `cowiki read`
- [x] `cowiki list`
- [x] `cowiki workspaces`
- [x] `cowiki submit`
- [x] `cowiki review list/show/approve/reject`

### Phase 4: SKILL.md
- [x] Write hosted `skill.md` (for `https://cowiki.app/skill.md`)
- [x] Write local skill bundle (bundled in npm package at `cli/skills/cowiki-cli/`)
- [x] Write SKILL.md sub-files (commands.md, config.md, troubleshooting.md)
- [x] Align local skill bundle with remote URLs: ensure `cli/skills/cowiki-cli/` structure matches the Skill Files Table so agent can locate files offline

### Phase 5: Polish
- [x] Command-level tests: argument parsing, output formatting (table/JSON), and error handling (missing args, auth failures, network errors) — 7 test files, 56 tests passing
- [ ] Skill flow tests: implement `tests/skill/` test suite per [skill flow testing design](./2026-06-11-skill-flow-testing-design.md)
- [ ] Shell completions (generated by Commander)
- [x] Remove old Rust CLI directory
- [ ] Update CI to build/test JS CLI

## Error Handling

- API errors: print status code + message, exit 1
- Network errors: print connection error, suggest checking server URL
- Missing API key: suggest `cowiki login` or `cowiki setup`
- Missing workspace: suggest `cowiki workspaces` to list available workspaces
- Invalid `.env`: print parse error with line number
- Key creation failure: print error, suggest verifying credentials with `cowiki key list`

## Security

- API keys stored in `.env` only, never committed
- `.env` already in `.gitignore`
- Warn if API key sent over non-HTTPS remote connection
- `cowiki login` uses local OAuth flow, never sees user password
- CLI never sends API key to domains other than configured server
