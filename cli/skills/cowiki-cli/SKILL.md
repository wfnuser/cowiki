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

```bash
# Check if already installed
cowiki --version

# If not found, install globally
npm install -g @cowiki/cli
```

## Step 3: Get API Key

**STOP HERE — you must ask the user for their API key before proceeding.**

Ask the user to sign in to cowiki and paste their API key. The user opens the login page in their browser, completes GitHub OAuth, copies the API key from the dialog, and pastes it into chat.

**Do NOT proceed to Step 4 until the user has provided their API key.**

Once the user pastes their key, run:

```bash
cowiki setup --api-key <pasted_key> --server http://localhost:3000
```

This validates the key and saves it to `~/.cowiki-cli/.env`.

## Step 4: Verify

```bash
cowiki workspaces             # list available workspaces
cowiki list -w <slug>         # list pages in a workspace
```

## Step 5: Local Skill Bundle

These files are available offline after install. See each for details:

| File | URL |
|------|-----|
| SKILL.md | https://cowiki.app/skill.md |
| commands.md | https://cowiki.app/skills/cowiki-cli/commands.md |
| config.md | https://cowiki.app/skills/cowiki-cli/config.md |
| troubleshooting.md | https://cowiki.app/skills/cowiki-cli/troubleshooting.md |

---

## Security Notes

- **Never leak API keys.** `.env` in `.gitignore`.
- Only send API keys to configured `COWIKI_BASE_URL`.
- CLI warns on non-HTTPS remote with API key.
- The agent guides the user to log in via browser — the CLI never sees GitHub credentials.
