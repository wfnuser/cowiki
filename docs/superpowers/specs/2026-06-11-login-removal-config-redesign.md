# Remove `cowiki login`, Redesign Config & Test Flow

**Date:** 2026-06-11
**Status:** design complete
**Depends on:** [CLI & SKILL.md Redesign](./2026-06-11-cli-skill-redesign.md), [Skill Flow Testing Design](./2026-06-11-skill-flow-testing-design.md)

## Overview

Three changes to simplify the auth flow and make testing practical:

1. **Remove `cowiki login` CLI command** — the CLI no longer opens a browser or handles OAuth
2. **SKILL.md guides agent to prompt user for browser login** — agent asks user to visit the login page, complete GitHub OAuth, and paste the API key
3. **`cowiki setup --api-key <key>` becomes the key storage mechanism** — default path `~/.cowiki-cli/.env`, overridable with `--env-path`
4. **Test flow redesigned** — interactive Claude tests where the human completes OAuth in browser

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Default config path | `~/.cowiki-cli/.env` | User-level key, not project-bound; survives `npm link` / re-installs |
| Key entry mechanism | Agent asks user to paste | Simpler than `cowiki login`; user already has the key from the web dialog |
| `cowiki login` | Removed entirely | Redundant — `cowiki setup --api-key` handles the same flow |
| Test auth path | Interactive Claude + manual OAuth | No way to automate GitHub OAuth without user credentials; accept manual step |
| Browser storage clearing | Not needed | User operates the browser entirely on their own; the test doesn't touch the browser |

## New Auth Flow

```
Agent reads SKILL.md
  │
  ├─ Step 1-2: Check environment, install CLI (unchanged)
  │
  ├─ Step 3: Get API Key (NEW)
  │    Agent says: "Please sign in to cowiki to get your API key.
  │                 Open this link: http://localhost:5173/login
  │                 Click 'Sign in with GitHub' and authorize cowiki.
  │                 After login, you'll see your API key. Copy it and paste it here."
  │
  │    User opens browser → GitHub OAuth → ApiKeyModal displays key
  │    User copies key → pastes into chat
  │
  │    Agent runs: cowiki setup --api-key cw_xxx --server http://localhost:3000
  │    → Validates against server, writes to ~/.cowiki-cli/.env
  │
  ├─ Step 4-5: Verify, install skill bundle (unchanged)
```

**Before (old flow):**
```
cowiki login --server URL
  → CLI opens browser
  → User does OAuth
  → ApiKeyModal shows key
  → User copies key, pastes into CLI prompt (in terminal)
  → CLI writes to cwd/.env
```

**After (new flow):**
```
Agent asks user to log in
  → User opens browser themselves
  → User does OAuth
  → ApiKeyModal shows key
  → User copies key, pastes into agent chat
  → Agent runs cowiki setup --api-key <key>
  → CLI writes to ~/.cowiki-cli/.env
```

## Config Layer Changes

### `config.ts`

```typescript
import os from 'node:os';
import path from 'node:path';

const DEFAULT_ENV_DIR = path.join(os.homedir(), '.cowiki-cli');
const DEFAULT_ENV_PATH = path.join(DEFAULT_ENV_DIR, '.env');

// loadConfig() — reads from ~/.cowiki-cli/.env
export function loadConfig(serverOverride?: string): CliConfig {
  dotenv.config({ path: DEFAULT_ENV_PATH, override: false });
  const baseUrl = serverOverride || process.env.COWIKI_BASE_URL || 'http://localhost:3000';
  const frontendUrl = process.env.COWIKI_FRONTEND_URL || 'http://localhost:5173';
  const apiKey = process.env.COWIKI_API_KEY || undefined;
  return { baseUrl, frontendUrl, apiKey };
}

// writeEnvFile() — default path ~/.cowiki-cli/.env, auto-creates directory
export function writeEnvFile(
  updates: Record<string, string>,
  envPath: string = DEFAULT_ENV_PATH,
): void {
  fs.mkdirSync(path.dirname(envPath), { recursive: true });
  // ... existing merge logic
}
```

**Key changes:**
- `dotenv.config()` explicitly targets `~/.cowiki-cli/.env` (no longer reads `cwd/.env`)
- `writeEnvFile()` accepts optional `envPath` parameter, defaults to `~/.cowiki-cli/.env`
- Auto-creates `~/.cowiki-cli/` directory on first write

## CLI Command Changes

### Removed

- `cli/src/commands/login.ts` — entire file
- `cli/src/index.ts` — `registerLoginCommand` import and call
- `package.json` — `open` dependency

### Modified: `cowiki setup`

New `--env-path` flag:

```typescript
.option('--env-path <path>', 'Path to .env file', DEFAULT_ENV_PATH)
```

`--api-key` mode writes to `opts.envPath` instead of hardcoded `cwd/.env`:

```typescript
if (opts.apiKey) {
  const envPath = path.resolve(opts.envPath.replace(/^~/, os.homedir()));
  // validate key, then:
  writeEnvFile({ COWIKI_BASE_URL: serverUrl, COWIKI_API_KEY: opts.apiKey }, envPath);
  printInfo(`Saved to ${envPath}`);
}
```

Interactive mode also uses `envPath` and removes the old hint to run `cowiki login`:

```typescript
// Old (removed):
printInfo('No API key set. Run "cowiki login" to authenticate with GitHub OAuth.');

// New:
printInfo('No API key set. Visit the cowiki website to get an API key, then run "cowiki setup --api-key <key>".');
```

### Global `--server` flag interaction

`cowiki setup --api-key <key> --env-path <path>` respects the global `--server` flag (via `globalOpts.server`). The server URL is written to `.env` alongside the API key, so subsequent commands work without `--server`.

## SKILL.md Changes

### Step 3 (rewritten)

```markdown
## Step 3: Get API Key

Ask the user to sign in to cowiki and paste their API key:

> "Please sign in to cowiki to get your API key.
> Open this link: <frontend URL>/login
> Click 'Sign in with GitHub' and authorize cowiki.
> After login, you'll see your API key in a dialog. Copy it and paste it here."

Wait for the user to paste their key, then run:

```bash
cowiki setup --api-key <pasted_key> --server http://localhost:3000
```

This validates the key against the server and saves it to `~/.cowiki-cli/.env`.

The key looks like: `cw_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx`
```

### Other steps

Steps 1, 2, 4, 5, Security Notes, and Skill Files Table are unchanged.

## Test Flow Changes

### Old Test Model

Two separate layers:
- **Layer 1:** `claude -p` non-interactive agent understanding test (fully automated)
- **Layer 2:** CLI integration tests with manual OAuth breakpoint at Step 3

### New Test Model

Single interactive test combining both concerns:

```
run-all.sh
  │
  ├─ Phase 0: Environment Setup
  │    lib/setup.sh — docker compose up + health check + npm link
  │
  ├─ Phase 1: Interactive Agent Test
  │    1. Launch Claude in interactive mode
  │    2. Claude reads SKILL.md, executes Steps 1-2 (env check, install CLI)
  │    3. Claude prompts user: "Please open this link and paste your API key"
  │       → User opens browser, completes GitHub OAuth, copies key
  │       → User pastes key into Claude chat
  │       → Claude runs: cowiki setup --api-key <key> --server http://localhost:3000
  │    4. Claude executes Steps 4-5 (verify connectivity, check bundle)
  │    5. Test script captures Claude transcript for scoring
  │
  ├─ Phase 2: Scoring
  │    evaluate.sh analyzes transcript against expected behavior:
  │    - 5 steps executed in order (40%)
  │    - Commands match expected set (30%)
  │    - Re-run idempotency (20%)
  │    - Error handling (10%)
  │    Pass threshold: ≥ 0.8
  │
  └─ Phase 3: Teardown
       docker compose down, clean fixtures
```

### Key Difference

| Aspect | Old | New |
|--------|-----|-----|
| Agent test mode | `claude -p` (non-interactive) | Interactive Claude session |
| OAuth handling | CLI `cowiki login` | Browser, user-operated |
| Key entry | Paste into CLI prompt | Paste into agent chat |
| Scoring | Automated against JSONL | Transcript analysis post-run |
| Manual step | Step 3 login script breakpoint | Agent asks user to paste key |

## Files Changed

| File | Action |
|------|--------|
| `cli/src/commands/login.ts` | **Delete** |
| `cli/src/index.ts` | Remove login import + registration |
| `cli/src/config.ts` | Default path → `~/.cowiki-cli/.env`, `writeEnvFile()` accepts `envPath` |
| `cli/src/commands/setup.ts` | Add `--env-path`, update hints, remove login references |
| `cli/package.json` | Remove `open` dependency |
| `cli/skill.md` | Rewrite Step 3 |
| `docs/superpowers/specs/2026-06-11-cli-skill-redesign.md` | Update auth flow section |
| `docs/superpowers/specs/2026-06-11-skill-flow-testing-design.md` | Update test architecture |
| `tests/skill/layer1-agent/run.sh` | Switch from `claude -p` to interactive |
| `tests/skill/layer2-cli/step3-login.sh` | Remove or repurpose |
| `tests/skill/run-all.sh` | Update test orchestration |

## Error Handling

- **Invalid API key at setup:** validation against `/api/auth/me` fails → print error, exit 1, no `.env` written
- **Missing `~/.cowiki-cli/` directory:** auto-created by `writeEnvFile()` with `fs.mkdirSync({ recursive: true })`
- **Permission denied on `~/.cowiki-cli/`:** `fs.mkdirSync` throws → caught as `CliError` with message about directory permissions
- **Agent receives malformed key:** `cowiki setup --api-key` validation rejects it before writing
- **Key looks right but server down:** validation timeout → print connection error, suggest checking server

## Security

- API key stored in `~/.cowiki-cli/.env` with `0o600` permissions (unchanged from existing `writeEnvFile`)
- Key never appears in shell history (agent passes it as `--api-key` argument within a single command execution)
- User's OAuth flow is entirely in their browser; the agent and CLI never see GitHub credentials
- `.env` at `~/.cowiki-cli/.env` is outside any project directory, reducing risk of accidental commit
