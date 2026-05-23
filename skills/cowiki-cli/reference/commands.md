# cowiki CLI Command Reference

Full reference for all cowiki CLI commands with flags, examples, and output formats.

---

## Ingest

Add a source document to the wiki for later compilation.

```bash
cowiki ingest --type <type> --content <content> [--branch <branch>]
```

### Flags

| Flag | Required | Default | Description |
|------|----------|---------|-------------|
| `--type` | Yes | `url` | Source type: `url`, `text`, or `file` |
| `--content` | No | — | Content: URL string, inline text, or file path |
| `--branch` | No | auto | Target branch (defaults to `user/<id>` if authenticated, else `main`) |

### Examples

```bash
# Ingest a URL
cowiki ingest --type url --content https://example.com/article

# Ingest inline text
cowiki ingest --type text --content "Important note about deployment"

# Ingest a local file
cowiki ingest --type file --content ./notes.md

# Ingest to a specific branch
cowiki ingest --type url --content https://example.com --branch my-research
```

### AI Agent Notes
- After ingesting, sources must be **compiled** (`cowiki compile`) before they become wiki pages
- The `--content` flag is optional; if omitted, reads from stdin
- File paths are resolved relative to the current working directory

---

## Compile

Compile ingested source documents into structured wiki pages using LLM.

```bash
cowiki compile [--branch <branch>] [--timeout <seconds>]
```

### Flags

| Flag | Required | Default | Description |
|------|----------|---------|-------------|
| `--branch` | No | auto | Branch to compile sources from |
| `--timeout` | No | `120` | Timeout in seconds (compilation may involve LLM calls) |

### Examples

```bash
# Compile on default branch
cowiki compile

# Compile with longer timeout for large sources
cowiki compile --timeout 300

# Compile a specific branch
cowiki compile --branch my-research
```

### AI Agent Notes
- Compilation is potentially slow — default timeout is 120 seconds
- Only **ingested sources** are compiled; already-compiled sources are skipped (dedup)
- Compilation is idempotent: running it again won't duplicate pages

---

## Write

Create a new wiki page or edit an existing one.

```bash
cowiki write <slug> [--title <title>] [--body <body>] [--branch <branch>] [--summary <summary>]
```

### Flags

| Flag | Required | Default | Description |
|------|----------|---------|-------------|
| `slug` | Yes | — | Page slug (URL-safe identifier) |
| `--title` | No | — | Page title (used in editor template) |
| `--body` | No | — | Page body content (inline text) |
| `--branch` | No | auto | Target branch |
| `--summary` | No | — | Change summary for version tracking |

### Input Modes

1. **Inline mode**: Provide `--title` and `--body` directly
2. **Editor mode**: Run without `--body` to open `$EDITOR`
3. **Stdin mode**: Pipe content when no `--body` flag

### Examples

```bash
# Inline write
cowiki write my-page --title "My Page" --body "# Hello\n\nWorld"

# Open editor
cowiki write my-page --title "My Page"

# Pipe from stdin
echo "# Pipeline Test" | cowiki write test-page

# Write with change summary
cowiki write my-page --body "Updated content" --summary "Fix typos"
```

### AI Agent Notes
- When writing multi-line body content, prefer inline mode with `\n` for short content
- For long content, pipe via stdin or use a temporary file
- The `--slug` is the URL identifier — use lowercase, hyphens for spaces

---

## Search

Semantic search across wiki pages.

```bash
cowiki search <query> [--limit <n>] [--branch <branch>]
```

### Flags

| Flag | Required | Default | Description |
|------|----------|---------|-------------|
| `query` | Yes | — | Search query text |
| `--limit` | No | `10` | Max results to return |
| `--branch` | No | auto | Branch to search |

### Examples

```bash
# Basic search
cowiki search "docker networking"

# Limit results
cowiki search "rust async" --limit 5

# JSON output for scripting
cowiki search "api design" --json | jq '.[].slug'
```

### AI Agent Notes
- Semantic search — natural language queries work best
- Results include relevance scores; higher limits may return less relevant pages
- Use `--json` output when piping to other tools

---

## Read

Read a wiki page by slug.

```bash
cowiki read <slug> [--branch <branch>] [--no-pager]
```

### Flags

| Flag | Required | Default | Description |
|------|----------|---------|-------------|
| `slug` | Yes | — | Page slug to read |
| `--branch` | No | auto | Branch to read from |
| `--no-pager` | No | — | Print directly to stdout (skip pager) |

### Examples

```bash
# Read with pager (less)
cowiki read docker-networking

# Read without pager
cowiki read docker-networking --no-pager

# JSON output
cowiki read docker-networking --json
```

### AI Agent Notes
- Default pager is `less` — use `--no-pager` for programmatic consumption
- The page content is returned as markdown
- Use `--json` to get structured output with metadata

---

## List

List wiki pages on a branch.

```bash
cowiki list [--branch <branch>]
```

### Flags

| Flag | Required | Default | Description |
|------|----------|---------|-------------|
| `--branch` | No | auto | Branch to list pages from |

### Examples

```bash
# List pages on default branch
cowiki list

# List on specific branch
cowiki list --branch feature/docs

# JSON output
cowiki list --json
```

### AI Agent Notes
- Lists pages with slug, title, and summary
- Default branch is `user/<id>` if authenticated, else `main`

---

## Submit

Submit pages from personal space for review.

```bash
cowiki submit [<slugs>...] [--all] [--branch <branch>] [--yes]
```

### Flags

| Flag | Required | Default | Description |
|------|----------|---------|-------------|
| `slugs` | No* | — | Page slugs to submit (one or more) |
| `--all` | No | — | Submit all pages on the branch |
| `--branch` | No | auto | Branch to submit from |
| `--yes` | No | — | Skip confirmation prompt |

\* `slugs` is required unless `--all` is used.

### Examples

```bash
# Submit specific pages
cowiki submit my-page another-page

# Submit all pages (with confirmation)
cowiki submit --all

# Submit all pages, skip confirmation
cowiki submit --all --yes
```

### AI Agent Notes
- Submitted pages enter the **review queue** — see `cowiki review`
- Use `--yes` for non-interactive/CI workflows
- Submitting a page does not remove it from the personal branch

---

## Review

Manage the review queue: list, inspect, approve, or reject submissions.

```bash
cowiki review <subcommand> [flags]
```

### Subcommands

| Subcommand | Description |
|------------|-------------|
| `list` | List review submissions |
| `show <id>` | Show submission details with colored diff |
| `approve <id>` | Approve a submission |
| `reject <id>` | Reject a submission |

### Review List

```bash
cowiki review list [--status <status>]
```

| Flag | Required | Default | Description |
|------|----------|---------|-------------|
| `--status` | No | — | Filter: `pending`, `approved`, `rejected` |

### Review Show

```bash
cowiki review show <id>
```

Shows the submission detail with a colored diff of changes.

### Review Approve / Reject

```bash
cowiki review approve <id>
cowiki review reject <id>
```

### Examples

```bash
# List pending reviews
cowiki review list

# List only approved
cowiki review list --status approved

# Inspect a submission
cowiki review show abc123

# Approve
cowiki review approve abc123

# Reject
cowiki review reject abc123
```

### AI Agent Notes
- Review actions are **irreversible** — confirm before approving/rejecting
- The `show` command displays a colored diff for visual review
- Review statuses: `pending` (yellow), `approved` (green), `rejected` (red)

---

## Global Flags

These flags work with any command:

| Flag | Description |
|------|-------------|
| `--server <URL>` | Override the server URL for this invocation |
| `--json` | Output in machine-readable JSON format |

### Examples

```bash
# Use a different server
cowiki search "topic" --server https://cowiki.example.com

# JSON output for all commands
cowiki list --json
cowiki read my-page --json
cowiki search "query" --json
```
