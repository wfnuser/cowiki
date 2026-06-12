# cowiki CLI

Command-line client for [cowiki](https://github.com/wfnuser/cowiki) — a collaborative wiki where humans and AI agents co-maintain a shared knowledge base.

## Quick Start

### Dev Install (from source)

```bash
cd cli
npm install
npm run build
npm link
cowiki --help
```

### npm Install (when published)

```bash
npm install -g @cowiki/cli
cowiki --help
```

### Setup

```bash
# Interactive setup wizard
cowiki setup

# Or non-interactive
cowiki setup --api-key cw_xxx --server https://cowiki.example.com
```

Configuration is stored in `~/.cowiki-cli/.env`. Alternatively, use env vars:

- `COWIKI_BASE_URL` — server base URL
- `COWIKI_API_KEY` — API key for authentication

### Testing

```bash
npm test
```

## For Agents

Read and install cowiki-cli skill for me: [file://cli/skill.md](https://github.com/wfnuser/cowiki/blob/8a2c4d7586e7ab2a1560c5d32cd38d48949247ae/cli/skills/cowiki-cli/SKILL.md), and setup cowiki.

## Commands

| Command | Description |
|---------|-------------|
| `cowiki ingest` | Add a source (URL, text, or file) |
| `cowiki compile` | Compile sources into wiki pages with LLM |
| `cowiki write` | Create or edit a page (use `--dir entities|concepts` for multi-dir wiki) |
| `cowiki search` | Semantic search across the wiki |
| `cowiki read` | Read a page with pager (use `--dir entities|concepts` for multi-dir wiki) |
| `cowiki list` | List pages on a branch (use `--dir entities|concepts|all` for multi-dir wiki) |
| `cowiki workspaces` | List available workspaces |
| `cowiki submit` | Submit pages for review |
| `cowiki review` | Review submissions (approve/reject) |

## Workspaces

cowiki supports two types of workspaces. Use the `--workspace`/`-w` flag to target a specific one.

### Personal Workspace

Every user has a private workspace (created automatically on sign-up). Operations use your personal branch (`user/<your-id>`) by default.

```bash
# Read a page from your personal workspace
cowiki read my-notes

# List all pages in your personal workspace
cowiki list

# Write a new page to your personal workspace
cowiki write getting-started --title "Getting Started" --body "# Welcome"
```

### Shared (Team) Workspace

Team workspaces are shared spaces with members and roles. Use `-w <slug>` to operate on them.

```bash
# List pages in a team workspace
cowiki list -w engineering-wiki

# Read a page from a team workspace
cowiki read architecture -w engineering-wiki

# Write to a team workspace (uses your personal branch for drafts)
cowiki write design-doc -w engineering-wiki --title "Design Doc" --body "# Overview"

# Ingest a source into a team workspace
cowiki ingest --type url --content https://example.com/article -w engineering-wiki

# Compile sources in a team workspace
cowiki compile -w engineering-wiki
```

### Branch Resolution

| Scenario | Default Branch | Override |
|----------|---------------|----------|
| Personal workspace (no `-w`) | `main` | `--branch <name>` |
| Shared workspace (`-w <slug>`) | `user/<your-id>` | `--branch <name>` |

### Finding Your Workspace Slug

```bash
# List all workspaces you have access to
cowiki workspaces

# Shows: NAME, SLUG, ROLE (owner/writer/reader), VISIBILITY (private/public)
```

Your personal workspace slug matches your user ID. Team workspace slugs are the URL-friendly names shown in the web UI sidebar (e.g., `engineering-wiki`).

### Multi-Directory Wiki

cowiki supports three content directories beyond the default `wiki/`:

| Directory | Purpose | Example |
|-----------|---------|---------|
| `wiki/` | General knowledge pages (default) | `cowiki write architecture --body "..."` |
| `entities/` | Extracted entities (people, projects, events) | `cowiki write alice --dir entities --body "..."` |
| `concepts/` | Patterns, decisions, conventions | `cowiki write error-handling --dir concepts --body "..."` |

```bash
# Write to different directories
cowiki write my-project -w mywiki --dir entities --body "# My Project"
cowiki write error-pattern -w mywiki --dir concepts --body "# Error Pattern"

# List specific directories
cowiki list -w mywiki --dir entities          # entities only
cowiki list -w mywiki --dir concepts          # concepts only
cowiki list -w mywiki --dir all               # all directories merged

# Read from specific directories
cowiki read my-project -w mywiki --dir entities
cowiki read error-pattern -w mywiki --dir concepts
```

## Usage Examples

### Ingest → Compile → Submit (the core workflow)

```bash
# Personal workspace
cowiki ingest --type url --content https://example.com/article
cowiki compile

# Team workspace
cowiki ingest --type url --content https://example.com/article -w team-wiki
cowiki compile -w team-wiki

# Submit all compiled pages
cowiki submit --all
```

### Search & Read

```bash
# Semantic search
cowiki search "docker networking"

# Read with default pager (less)
cowiki read docker-networking

# Read without pager
cowiki read docker-networking --no-pager
```

### Write & Edit

```bash
# Inline write
cowiki write my-page --title "My Page" --body "Hello world"

# Open in $EDITOR (vim, nano, etc.)
cowiki write my-page

# Pipe content from stdin
echo "## Hello" | cowiki write my-page
```

### Review

```bash
# List pending reviews
cowiki review list

# Show review with colored diff
cowiki review show <id>

# Approve or reject
cowiki review approve <id>
cowiki review reject <id>
```

## Output Formats

```bash
# Default: human-friendly tables with colors
cowiki search "topic"

# Machine-readable JSON for scripting
cowiki search "topic" --json | jq '.[].slug'
```

## Configuration

Credentials and defaults are stored in `~/.cowiki-cli/.env`:

```env
COWIKI_BASE_URL=https://api.cowiki.app   # or http://localhost:3000 for local dev
COWIKI_API_KEY=your-api-key
```

Set up interactively with `cowiki setup`, or create the file manually.

Override with environment variables:

| Variable | Field |
|----------|-------|
| `COWIKI_SERVER` | (deprecated, use `COWIKI_BASE_URL`) |
| `COWIKI_BASE_URL` | Server base URL |
| `COWIKI_API_KEY` | API key |

## Shell Completions

```bash
# bash
source <(cowiki completions bash)

# zsh
source <(cowiki completions zsh)

# fish
cowiki completions fish | source
```

## Architecture

- **Pure HTTP client** — zero dependency on `cowiki_core` or `cowiki_db`
- **Workspace-aware** — `--workspace`/`-w` routes to per-workspace API endpoints
- **Stateless except auth** — only `~/.cowiki-cli/.env` persisted
- **TypeScript** — Node.js runtime, native `fetch` for HTTP
- **Dual output** — human-friendly tables by default, `--json` for scripting

## Testing

```bash
npm test
```

Unit tests cover argument parsing, URL construction, config loading, output formatting, and error handling. No server needed.

## Future Plans

- [ ] Terminal UI (TUI) for interactive browsing and editing
- [ ] `workspace create` / `workspace invite` management commands
- [ ] npm package publication (`@cowiki/cli`)