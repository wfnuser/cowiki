# cowiki CLI

Command-line client for [cowiki](https://github.com/wfnuser/cowiki) — a collaborative wiki where humans and AI agents co-maintain a shared knowledge base.

## Quick Start

```bash
cargo build --release
./target/release/cowiki --help

# edit env with your API key and server URL
cp .env.example .env

# check configuration
cargo run --release help 
```

## Commands

| Command | Description |
|---------|-------------|
| `cowiki login` | Register/login and save credentials |
| `cowiki ingest` | Add a source (URL, text, or file) |
| `cowiki compile` | Compile sources into wiki pages with LLM |
| `cowiki write` | Create or edit a page |
| `cowiki search` | Semantic search across the wiki |
| `cowiki read` | Read a page (with pager) |
| `cowiki list` | List pages on a branch |
| `cowiki submit` | Submit pages for review |
| `cowiki review` | Review submissions (approve/reject) |

## Usage Examples

### Ingest → Compile → Submit (the core workflow)

```bash
# Ingest a URL
cowiki ingest --type url --content https://example.com/article

# Compile ingested sources into wiki pages
cowiki compile

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

Credentials and defaults are stored in `~/.config/cowiki/config.toml`:

```toml
server_url = "http://localhost:3000"
api_key = "your-api-key"
default_branch = "main"
```

Override with environment variables:

| Variable | Field |
|----------|-------|
| `COWIKI_SERVER` | `server_url` |
| `COWIKI_API_KEY` | `api_key` |
| `COWIKI_BRANCH` | `default_branch` |

## Shell Completions

```bash
# bash
source <(cowiki completions bash)

# zsh
source <(cowiki completions zsh)

# fish
cowiki completions fish | source
```

## Build from Source

```bash
cd cli
cargo build --release
```

The CLI is a standalone crate — it's excluded from the root workspace.
Build it independently; `cargo build` at the repo root won't include it.

## Architecture

- **Pure HTTP client** — zero dependency on `cowiki_core` or `cowiki_db`
- **Stateless except auth** — only `~/.config/cowiki/config.toml` persisted
- **Async** — `tokio` runtime, `reqwest` HTTP client
- **Dual output** — human-friendly tables by default, `--json` for scripting

## Future Plans

- [ ] Terminal UI (TUI) for interactive browsing and editing