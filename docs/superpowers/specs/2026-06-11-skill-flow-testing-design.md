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
├── run-all.sh                        # Entry point (aggregates all results)
├── teardown.sh                       # Cleanup: docker compose down, rm fixtures
├── fixtures/
│   └── .env.test                     # Test env template
├── layer1-agent/                     # Agent understanding tests
│   ├── run.sh                        # Automated: claude -p → tee → evaluate
│   ├── prompts/skill-test.prompt.md  # Standard prompt for LLM agent
│   ├── expected/
│   │   ├── steps.json                # Expected 5 steps + commands
│   │   └── idempotency-cases.json    # Idempotency check scenarios
│   └── evaluate.sh                   # Scoring script (transcript → score)
└── layer2-cli/                       # CLI integration tests
    ├── test-skill-flow.sh            # Main orchestrator (run all steps, aggregate)
    ├── step1-env-check.sh
    ├── step2-install.sh
    ├── step3-login.sh                # OAuth (manual breakpoint) or --api-key fast path
    ├── step4-verify.sh
    ├── step5-bundle.sh
    └── lib/
        ├── assert.sh                 # Assertion helpers
        └── setup.sh                  # docker compose up + npm link + health poll
```

**Execution order:** `lib/setup.sh` first (docker + npm link), then Layer 2 and
Layer 1 can run in any order. Both layers are fully automated.

## Layer 1: Agent Skill Understanding Test

Tests whether a real LLM agent (Claude Code) can correctly interpret the
SKILL.md instructions and produce correct shell commands for each step.

### Automated Execution (`layer1-agent/run.sh`)

Layer 1 runs fully automated via `claude -p` (print/non-interactive mode with
full agent loop including tool calls):

```bash
#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
OUTPUT="$SCRIPT_DIR/output.jsonl"

# Run Claude in non-interactive mode, capture full transcript
claude -p "$(cat "$SCRIPT_DIR/prompts/skill-test.prompt.md")" \
    --output-format json \
    --allowedTools "Bash,Read" \
    2>&1 | tee "$OUTPUT"

# Score the transcript against expected behavior
bash "$SCRIPT_DIR/evaluate.sh" "$OUTPUT"
```

Key points:
- `claude -p` runs the full agent loop (think → tool call → observe → repeat),
  not just a single text response. The agent actually executes `node --version`,
  `npm link`, etc.
- `--allowedTools "Bash,Read"` constrains the agent to only the tools it needs
  for following SKILL.md, preventing it from doing unexpected things.
- `tee` captures the raw output for evaluation while still showing progress
  in the terminal.
- The `claude` CLI must be installed and configured with a valid API key.

### Test Prompt (`prompts/skill-test.prompt.md`)

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

### Evaluation Script (`evaluate.sh`)

`evaluate.sh` accepts a transcript file path as its argument (the JSONL output
from `run.sh`). It parses the JSONL to extract tool calls, runs grep/pattern
checks against the four dimensions, and prints a pass/fail table.

### Expected Steps Specification (`steps.json`)

```json
{
  "steps": [
    {
      "id": 1,
      "name": "Environment Check",
      "commands": ["node --version", "npm --version"],
      "fail_condition": "Node.js version < 18"
    },
    {
      "id": 2,
      "name": "Install CLI",
      "commands": ["cowiki --version", "npm install -g @cowiki/cli"],
      "skip_condition": "cowiki --version succeeds"
    },
    {
      "id": 3,
      "name": "Login",
      "commands": ["cowiki login --server http://localhost:3000"],
      "output_requires": ["API key", ".env"]
    },
    {
      "id": 4,
      "name": "Verify",
      "commands": ["cowiki workspaces", "cowiki list -w"],
      "output_requires": ["workspace"]
    },
    {
      "id": 5,
      "name": "Install Local Skill Bundle",
      "commands": ["ls skills/cowiki-cli/SKILL.md"],
      "output_requires": ["skill bundle"]
    }
  ],
  "min_score": 0.8,
  "scoring": {
    "step_order": 0.4,
    "commands_match": 0.3,
    "idempotency": 0.2,
    "error_handling": 0.1
  }
}
```

`evaluate.sh` scores the transcript against this spec. Weighted dimensions:

| Dimension | Weight | How scored |
|-----------|--------|------------|
| Step order | 40% | Steps appear in sequence 1→2→3→4→5 |
| Commands match | 30% | Agent ran the expected commands (fuzzy match on binary name) |
| Idempotency | 20% | On re-run, agent skipped already-done steps |
| Error handling | 10% | Agent correctly handled simulated failures (e.g., Node < 18) |

Pass threshold: weighted score ≥ 0.8.

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

    # Poll health endpoint with timeout (max 60s)
    local max_attempts=60
    local attempt=0
    while ! curl -s http://localhost:3000/api/health > /dev/null 2>&1; do
        attempt=$((attempt + 1))
        if [[ $attempt -ge $max_attempts ]]; then
            echo "ERROR: Backend failed to come up within ${max_attempts}s"
            exit 1
        fi
        sleep 1
    done

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

### Step 3: Login (Real OAuth with Fast-Path Bypass)

Two modes:

**Fast path** (for daily dev, no human needed):
```bash
export COWIKI_TEST_API_KEY="cw_test_..."
cowiki setup --server http://localhost:3000 --api-key "$COWIKI_TEST_API_KEY"
```
Uses `cowiki setup` with a pre-existing API key. Create a long-lived test key once via the web UI.

**Full OAuth path** (for release validation):
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
- `cowiki search "test"` — assert search is reachable

### Step 5: Local Skill Bundle

Assert all five files exist:
- `cli/skill.md`
- `cli/skills/cowiki-cli/SKILL.md`
- `cli/skills/cowiki-cli/commands.md`
- `cli/skills/cowiki-cli/config.md`
- `cli/skills/cowiki-cli/troubleshooting.md`

Verify that the Skill Files Table in `cli/skill.md` lists URLs matching the
local file paths (so the agent can locate files offline after install).

### Idempotency (Per-Step Re-Run)

Each step script accepts an optional `--rerun` flag. When set, the step runs a
second time and asserts it produces **no side effects**:

| Step | Re-run behavior |
|------|----------------|
| Step 1 | `node --version` still succeeds (idempotent by nature) |
| Step 2 | `cowiki --version` succeeds; `npm install -g` prints "already installed" |
| Step 3 | `.env` already has `COWIKI_API_KEY`; `cowiki login` skips or reconfirms |
| Step 4 | `cowiki list` returns same results (no mutations from verify step) |
| Step 5 | All bundle files still exist; re-download is hash-noop |

The orchestrator `test-skill-flow.sh` invokes each step script twice: first with
no flag (fresh run), then with `--rerun` (idempotency check). Both must pass.

### Teardown (`teardown.sh`)

```bash
#!/usr/bin/env bash
set -euo pipefail

# Stop and remove containers (preserves volumes by default)
docker compose -f "$REPO_ROOT/docker-compose.yml" down

# Clean up test fixtures
rm -f "$REPO_ROOT/tests/skill/fixtures/.env.test.generated"
rm -f "$REPO_ROOT/cli/.env.test"

echo "Teardown complete."
```

### Test Orchestrator (`test-skill-flow.sh`)

Aggregates pass/fail from each step and prints a summary table:

### `run-all.sh` Entry Point

```bash
#!/usr/bin/env bash
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SKILL_DIR="$REPO_ROOT/tests/skill"

echo "=== cowiki Skill Flow Test Suite ==="
echo ""

# Phase 0: Setup
echo "[0] Setting up environment..."
bash "$SKILL_DIR/layer2-cli/lib/setup.sh"

# Phase 1: Layer 2 — CLI integration tests
echo ""
echo "[1] Layer 2: CLI integration tests..."
bash "$SKILL_DIR/layer2-cli/test-skill-flow.sh"
LAYER2_RESULT=$?

# Phase 2: Layer 1 — Agent understanding test (fully automated via claude -p)
echo ""
echo "[2] Layer 1: Agent understanding test..."
bash "$SKILL_DIR/layer1-agent/run.sh"
LAYER1_RESULT=$?

# Phase 3: Teardown
echo ""
bash "$SKILL_DIR/teardown.sh"

# Aggregate
echo ""
echo "=== Final Results ==="
printf "  %-30s %s\n" "Layer 2 (CLI integration)" "$([ $LAYER2_RESULT -eq 0 ] && echo 'PASS' || echo 'FAIL')"
printf "  %-30s %s\n" "Layer 1 (Agent understanding)" "$([ $LAYER1_RESULT -eq 0 ] && echo 'PASS' || echo 'FAIL')"

if [[ $LAYER2_RESULT -ne 0 || $LAYER1_RESULT -ne 0 ]]; then
    exit 1
fi
```

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

### System

- **Node.js 18+** and **npm** — for CLI build and `npm link`
- **Docker** and **docker compose** — for running the backend
- **jq** — for JSON assertions in assertion library
- **bash 4+** — for associative arrays in the orchestrator
- **Claude Code CLI** (`claude`) — for Layer 1 automated agent testing. Must be
  installed (`npm install -g @anthropic-ai/claude-code`) and authenticated with a
  valid API key. Only required if running Layer 1; Layer 2 can run independently.

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
| `tests/skill/run-all.sh` | Entry point (setup → layer2 → layer1 → teardown) |
| `tests/skill/teardown.sh` | Cleanup |
| `tests/skill/fixtures/.env.test` | Test env template |
| `tests/skill/layer1-agent/run.sh` | Automated claude -p runner |
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
