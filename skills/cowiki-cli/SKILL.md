---
name: cowiki-cli
description: >
  Full-featured guide for the cowiki CLI — from installation and configuration
  to daily wiki operations (ingest, compile, search, read, write, list, submit, review).
  Activate when: cowiki cli, cowiki 命令行, cowiki 配置, cowiki 安装, cowiki command,
  install cowiki, setup cowiki cli, 怎么用 cowiki, cowiki 命令,
  or when user wants to operate the wiki but hasn't specified a tool.
argument-hint: "[install | <command> | troubleshoot]"
---

# cowiki CLI

Command-line client for cowiki — a collaborative wiki where humans and AI agents co-maintain a shared knowledge base. The CLI is a standalone Rust binary that talks to a cowiki server over HTTP.

## When to Use

- User wants to install or configure the cowiki CLI
- User asks about any cowiki command: `ingest`, `compile`, `search`, `read`, `write`, `list`, `submit`, `review`
- User wants to operate the wiki but hasn't specified which tool (CLI, MCP, web)
- CLI returns an error and user needs troubleshooting help
- User says "cowiki 怎么用", "cowiki 命令", "cowiki 安装"

## When NOT to Use

- User wants MCP server setup → use `/cowiki-mcp`
- User wants web UI operations → direct to browser
- User wants server-side configuration → read `docs/config.md`

## Interactive Hook Protocol

**MANDATORY**: Use `vscode_askQuestions` for user-facing questions during **installation/configuration only**.
Command reference and troubleshooting sections are read-only — AI agent reads them and acts directly.

### Hook Points

| Step | Trigger | Question | Options |
|------|---------|----------|---------|
| 1. Detect | Start | Setup type | First-time install / Reconfigure / Just use |
| 2. Source | After detection | How to install? | Build from source / Prebuilt binary / Already installed |
| 3. API Key | After source | How to get API key? | Register on web / I already have one |
| 4. Server URL | After key | Server endpoint | Local (localhost:3000) / Remote / Custom |
| 5. Shell | After URL | Setup shell completions? | bash / zsh / fish / Skip |

## Quick Start

### Build from Source

```bash
cd cli
cargo build --release
./target/release/cowiki --help
```

The CLI is a standalone crate — excluded from the root workspace. Build it independently.

### Configure

```bash
# Create config interactively
cowiki config set server-url http://localhost:3000
cowiki config set api-key <your-key>

# Or use environment variables
export COWIKI_BASE_URL=http://localhost:3000
export COWIKI_API_KEY=<your-key>
```

Config file: `~/.config/cowiki/config.toml`

```toml
server_url = "http://localhost:3000"
api_key = "your-api-key"
```

### Shell Completions

```bash
# bash
source <(cowiki completions bash)

# zsh
source <(cowiki completions zsh)

# fish
cowiki completions fish | source
```

## Commands

| Command | Description | Detail |
|---------|-------------|--------|
| `cowiki ingest` | Add a source (URL, text, or file) | [reference/commands.md#ingest](reference/commands.md#ingest) |
| `cowiki compile` | Compile sources into wiki pages via LLM | [reference/commands.md#compile](reference/commands.md#compile) |
| `cowiki write` | Create or edit a page | [reference/commands.md#write](reference/commands.md#write) |
| `cowiki search` | Semantic search across the wiki | [reference/commands.md#search](reference/commands.md#search) |
| `cowiki read` | Read a page (with pager) | [reference/commands.md#read](reference/commands.md#read) |
| `cowiki list` | List pages on a branch | [reference/commands.md#list](reference/commands.md#list) |
| `cowiki submit` | Submit pages for review | [reference/commands.md#submit](reference/commands.md#submit) |
| `cowiki review` | Review submissions (approve/reject) | [reference/commands.md#review](reference/commands.md#review) |

### Core Workflow: Ingest → Compile → Submit

```bash
# 1. Ingest a URL source
cowiki ingest --type url --content https://example.com/article

# 2. Compile ingested sources into wiki pages
cowiki compile

# 3. Submit all compiled pages for review
cowiki submit --all
```

### Search & Read

```bash
# Semantic search
cowiki search "docker networking"

# Read with pager (less)
cowiki read docker-networking

# Read without pager (direct stdout)
cowiki read docker-networking --no-pager
```

### Write Modes

```bash
# Inline write
cowiki write my-page --title "My Page" --body "Hello world"

# Open in $EDITOR (vim, nano, etc.)
cowiki write my-page

# Pipe content from stdin
echo "## Hello" | cowiki write my-page
```

### Review Workflow

```bash
# List pending reviews
cowiki review list

# Show review with colored diff
cowiki review show <id>

# Approve or reject
cowiki review approve <id>
cowiki review reject <id>
```

## Global Options

| Flag | Description |
|------|-------------|
| `--server <URL>` | Override server URL |
| `--json` | Machine-readable JSON output |

## Architecture

- **Pure HTTP client** — zero dependency on `cowiki_core` or `cowiki_db`
- **Stateless** except auth — only `~/.config/cowiki/config.toml` persisted
- **Async** — `tokio` runtime, `reqwest` HTTP client
- **Dual output** — human-friendly colored tables by default, `--json` for scripting
- **Security** — warns if API key is sent over non-HTTPS remote connection; config file permissions set to `0600`

## Configuration Reference

See [reference/config.md](reference/config.md) for details on config file, environment variables, and priority order.

## Troubleshooting

See [reference/troubleshooting.md](reference/troubleshooting.md) for common error patterns and fixes.

## Installation Scripts

| Script | Platform |
|--------|----------|
| `scripts/setup.sh` | Linux / macOS |
| `scripts/setup.ps1` | Windows |

Scripts accept:
- `--api-key <key>`: API key (required)
- `--url <url>`: Server URL (default: `http://localhost:3000`)
- `--shell <shell>`: Setup completions for bash/zsh/fish (optional)

## Security Notes

- API keys are stored in `~/.config/cowiki/config.toml` with `0600` permissions
- Never commit config.toml with real API keys
- The CLI warns if API key would be sent over non-HTTPS remote connections
- Use environment variables (`COWIKI_API_KEY`) in CI/CD or shared environments

## Related Docs

| File | Content |
|------|---------|
| `reference/commands.md` | Full command reference with all flags and examples |
| `reference/config.md` | Configuration file format, env vars, priority |
| `reference/troubleshooting.md` | Common errors and fixes |
| `scripts/setup.sh` | Linux/macOS interactive setup script |
| `scripts/setup.ps1` | Windows interactive setup script |
