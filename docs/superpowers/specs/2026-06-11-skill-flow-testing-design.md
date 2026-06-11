# Skill Flow Testing Design

**Date:** 2026-06-11
**Status:** design complete

## Overview

Design a two-layer test suite to verify the correctness of cowiki's SKILL.md
agent flow before the product website and npm bundle are officially published.
All testing runs locally against a local backend and locally linked CLI.

## Architecture

Two independent layers sharing the same local fixture:

```
tests/skill/
├── run-all.sh                        # Entry point
├── fixtures/
│   └── .env.test                     # Test env template
├── layer1-agent/                     # Agent understanding tests
│   ├── prompts/skill-test.prompt.md  # Standard prompt for LLM agent
│   ├── expected/
│   │   ├── steps.json                # Expected 5 steps + commands
│   │   └── idempotency-cases.json    # Idempotency check scenarios
│   └── evaluate.sh                   # Scoring script
└── layer2-cli/                       # CLI integration tests
    ├── test-skill-flow.sh            # Main orchestrator
    ├── step1-env-check.sh
    ├── step2-install.sh
    ├── step3-login.sh                # Manual OAuth breakpoint
    ├── step4-verify.sh
    ├── step5-bundle.sh
    └── lib/
        ├── assert.sh                 # Assertion helpers
        └── setup.sh                  # docker + npm link
```

**Execution order:** Layer 2 first (synchronous CLI tests), then Layer 1 (agent
evaluation — requires a separate agent session and transcript file).

## Layer 1: Agent Skill Understanding Test

Tests whether a real LLM agent (Claude Code) can correctly interpret the
SKILL.md instructions and produce correct shell commands for each step.

### Test Prompt

The agent receives a single prompt (from `prompts/skill-test.prompt.md`):

```markdown
Read file://cli/skill.md and follow the instructions to set up cowiki.
Use --server http://localhost:3000 for all commands.
```

### Evaluation Dimensions

| Dimension | Method | Pass Criteria |
|---|---|---|
| Step identification | Check agent transcript for 5 steps in order | All 5 steps executed |
| Idempotency | Re-run in already-setup env | Agent skips install, proceeds to login |
| Command correctness | Pattern-match executed commands vs expected | Commands match skill.md code blocks |
| Error handling | Mock node v16 environment | Agent tells user to upgrade Node.js, stops |

### Evaluation Script

`evaluate.sh` accepts a transcript file path as its argument. The transcript is
the Claude Code session log (typically under `.claude/transcripts/` or
`~/.claude/projects/<project>/`). If no path is given, it looks for the most
recent transcript in the current project.

It runs grep/pattern checks against the four dimensions, and prints a pass/fail
table

### Key Design Decision

Test **observable behavior** (commands executed, order, error responses), not
exact text output. Exact wording is brittle; correct actions are durable.

## Layer 2: CLI Link Integration Test

Tests every command in the SKILL.md flow against a real local backend.
No mocking, no bypassing — the full chain runs locally.

### Path Convention

All scripts assume they run from the repo root. Each script sets `REPO_ROOT`
by walking up from its own location:

```bash
REPO_ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
```

### Environment Setup (`lib/setup.sh`)

```bash
setup() {
    # docker-compose.yml is at repo root
    docker compose -f "$REPO_ROOT/docker-compose.yml" up -d
    until curl -s http://localhost:3000/api/health; do sleep 1; done
    cd "$REPO_ROOT/cli" && npm link && cd "$REPO_ROOT"
}
```

### Step 1: Environment Check

- Run `node --version`, assert >= 18
- Run `npm --version`, assert exists
- Exit with error message if not met

### Step 2: Install CLI

- Run `cowiki --version`, assert runs
- Run `cowiki --help`, assert lists expected subcommands (login, setup, key,
  ingest, compile, write, search, read, list, workspaces, submit, review)
- If `cowiki` not found, instruct user to run `npm link`

### Step 3: Login (Real OAuth, Manual Breakpoint)

- Run `cowiki login --server http://localhost:3000`
- Browser opens GitHub OAuth page
- Script prompts user: "Complete GitHub OAuth and paste API key here:"
- User pastes key, script continues
- Assert: `.env` now contains `COWIKI_API_KEY`
- Assert: `cowiki workspaces` no longer returns 401

No bypass. The real GitHub OAuth flow is tested end-to-end, with the human
completing the only step that genuinely requires human interaction. This
matches the exact user experience described in SKILL.md.

### Step 4: Verify

`COWIKI_TEST_WORKSPACE` env var (default: `test`) controls which workspace
slug is used.

- `cowiki workspaces` — assert returns JSON array
- `cowiki list -w "$COWIKI_TEST_WORKSPACE"` — assert returns page list
- `cowiki search --query test` — assert search is reachable

### Step 5: Local Skill Bundle

Assert all five files exist:
- `cli/skill.md`
- `cli/skills/cowiki-cli/SKILL.md`
- `cli/skills/cowiki-cli/commands.md`
- `cli/skills/cowiki-cli/config.md`
- `cli/skills/cowiki-cli/troubleshooting.md`

Verify that the Skill Files Table in `cli/skill.md` lists URLs matching the
local file paths (so the agent can locate files offline after install).

### Assertion Library (`lib/assert.sh`)

```bash
assert_version()      # Compare version strings (e.g., node >= 18)
assert_contains()     # stdout contains substring
assert_file()         # File exists
assert_http_ok()      # curl returns 2xx
assert_json_key()     # jq key is non-null
assert_json_array()   # jq output is a non-empty array
```

## Prerequisites (One-Time Setup)

### GitHub OAuth App

Create a test OAuth App in GitHub Settings → Developer Settings:

```
Homepage URL:  http://localhost:3000
Callback URL:  http://localhost:3000/api/auth/github/callback
```

Add `GITHUB_CLIENT_ID` and `GITHUB_CLIENT_SECRET` to the backend `.env`.

### Backend

The cowiki backend must be runnable locally via `docker compose up`.

### CLI Environment File

`cli/.env` (template at `cli/.env.example`):

```env
COWIKI_BASE_URL=http://localhost:3000
# COWIKI_API_KEY is written by cowiki login
```

## Test Data (Optional)

Creating a seed workspace improves Step 4 verification quality:

```bash
curl -X POST http://localhost:3000/api/workspaces \
  -H "Authorization: Bearer <test_key>" \
  -d '{"name": "test", "slug": "test"}'
```

## Future: CI Integration

When the product is published (npm bundle + production website):

1. Layer 2 Step 3 can be upgraded with Playwright for fully automated OAuth
2. The `npm link` step can be replaced with `npm install -g @cowiki/cli`
3. The hosted `https://cowiki.app/skill.md` URL can replace local `file://`
4. The entire suite can run in CI with a headless browser container

## File Creation Summary

| File | Purpose |
|---|---|
| `tests/skill/run-all.sh` | Entry point |
| `tests/skill/fixtures/.env.test` | Test env template |
| `tests/skill/layer1-agent/prompts/skill-test.prompt.md` | Agent test prompt |
| `tests/skill/layer1-agent/expected/steps.json` | Expected steps + commands |
| `tests/skill/layer1-agent/expected/idempotency-cases.json` | Idempotency scenarios |
| `tests/skill/layer1-agent/evaluate.sh` | Scoring script |
| `tests/skill/layer2-cli/test-skill-flow.sh` | Orchestrator |
| `tests/skill/layer2-cli/step1-env-check.sh` | Step 1 |
| `tests/skill/layer2-cli/step2-install.sh` | Step 2 |
| `tests/skill/layer2-cli/step3-login.sh` | Step 3 (manual OAuth breakpoint) |
| `tests/skill/layer2-cli/step4-verify.sh` | Step 4 |
| `tests/skill/layer2-cli/step5-bundle.sh` | Step 5 |
| `tests/skill/layer2-cli/lib/assert.sh` | Assertion helpers |
| `tests/skill/layer2-cli/lib/setup.sh` | Environment setup |
