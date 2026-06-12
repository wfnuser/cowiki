---
name: cowiki-cli
version: 0.1.0
homepage: https://cowiki.app
description: CLI client for cowiki - collaborative wiki. Agents use this to read, write, search, and manage knowledge across workspaces.
---

# cowiki CLI — Agent Skill

Agent-first instructions for autonomously installing, authenticating, and using the cowiki CLI.

## About cowiki

cowiki is a collaborative wiki platform. As an AI agent, you can use cowiki to read, write, search, and manage knowledge across workspaces.

## Before You Start

Ask the user for permission before:
- Installing software (`npm install -g`)
- Reading or modifying any files outside the current workspace
- Sending data to external servers

## Step 1: Environment Check

```bash
node --version   # must be >= 18
npm --version    # must be available
```

If Node.js is not installed or < 18, tell the user to install Node.js 18+.

## Step 2: Install CLI

**STOP HERE — ask the user for installation preference before proceeding.**

Ask the user:

> "cowiki CLI can be installed in two ways:
> 1. **Dev install** (`cd cli && npm install && npm run build && npm link`) — from the repo source, for local development
> 2. **Global** (`npm install -g @cowiki/cli`) — published package, available everywhere
> 
> Which do you prefer?"

Wait for the user's choice before proceeding.

**Dev install (from repo source):**
```bash
cd cli && npm install && npm run build && npm link
```

**Global install (published package):**
```bash
cowiki --version 2>/dev/null || npm install -g @cowiki/cli
```

## Step 3: Get API Key

**STOP HERE — you must ask the user for their API key before proceeding.**

Ask the user to sign in to cowiki and get an API key. The user opens the login page in their browser, completes GitHub OAuth, and copies the API key from the dialog.

**Do NOT proceed to Step 4 until the user has provided their API key.**

Once the user has their key, ask them to run:

```bash
cowiki setup --api-key <their-key> --server <server-url>
```

This interactive wizard validates the key and saves it to `~/.cowiki-cli/.env`.

Alternatively, the user can set environment variables:
- `COWIKI_BASE_URL` — **your** server URL, set by you during `cowiki setup` (falls back to the hosted `https://api.cowiki.app` if unset; use `http://localhost:3000` for local dev)
- `COWIKI_API_KEY` — API key for authentication

## Step 4: Verify

```bash
cowiki workspaces             # list available workspaces
cowiki list -w <slug>         # list pages in a workspace
```

## Step 5: Local Skill Bundle

**For global install**, the skill files are at the npm global package path:

```bash
COWIKI_PKG=$(npm root -g)/@cowiki/cli
ls "$COWIKI_PKG/skills/cowiki-cli/"
```

**For dev install**, they are at the repo path:

```bash
ls cli/skills/cowiki-cli/
```

These files are available offline after install:

| File | URL |
|------|-----|
| SKILL.md | https://cowiki.app/skill.md |
| commands.md | https://cowiki.app/skills/cowiki-cli/commands.md |
| config.md | https://cowiki.app/skills/cowiki-cli/config.md |
| troubleshooting.md | https://cowiki.app/skills/cowiki-cli/troubleshooting.md |
| llm-co-wiki.md | https://cowiki.app/skills/cowiki-cli/llm-co-wiki.md |

---

## Content Workflow: Two Paths

cowiki supports a multi-directory wiki structure:
- `wiki/` — general knowledge pages
- `entities/` — extracted entities (people, projects, events)
- `concepts/` — patterns, decisions, conventions

Use `--dir` (all commands) to target a directory. Default is `wiki/`.

### Path 1: Cloud Compile

For external URLs, large documents, or structured content that benefits from AI parsing:

```bash
# 1. Ingest source
cowiki ingest -w <ws> --type url --content "<url>"

# 2. Compile sources into wiki pages (cloud-side agent)
cowiki compile -w <ws> [--timeout 300]

# 3. Verify results
cowiki list -w <ws>
cowiki read -w <ws> <page>
```

The cloud agent handles source parsing, entity extraction, and page generation. Always writes to `wiki/`.

### Path 2: Local Agent Compile

For content that needs cross-references to existing wiki pages, entity extraction,
concept formation, or simple text that doesn't warrant cloud AI.

**Full workflow reference:** See `llm-co-wiki.md` for detailed entity extraction,
concept formation, cross-referencing, and maintenance procedures.

Quick start:

```bash
# 1. Gather context
cowiki list -w <ws> --dir all          # see all directories
cowiki read -w <ws> <related-page>     # read existing pages

# 2. Analyze and structure (agent's discretion):
#    - Extract entities → entities/
#    - Extract concepts → concepts/
#    - Build [[cross-references]] to existing pages

# 3. Write to appropriate directories
cowiki write -w <ws> <slug> --dir entities --body "..."
cowiki write -w <ws> <slug> --dir concepts --body "..."
cowiki write -w <ws> <slug> --body "..."              # defaults to wiki/
```

Use your judgment — skip compile for simple content. For large external sources, prefer Path 1 (ingest → compile).

---

## Security Notes

- **Never leak API keys.** `.env` in `.gitignore`.
- Only send API keys to configured `COWIKI_BASE_URL`.
- CLI warns on non-HTTPS remote with API key.
- The agent guides the user to log in via browser — the CLI never sees GitHub credentials.
