# cowiki-mcp-server

MCP→REST proxy: exposes cowiki wiki operations as MCP tools, forwarding to a cowiki-server REST API.

## Quick Start

```bash
cd cowiki-mcp-server
cp .env.example .env     # edit as needed
cargo run
```

## Configuration

All configuration is via environment variables or CLI flags (CLI > env > default):

| Variable | CLI Flag | Default | Description |
|----------|----------|---------|-------------|
| `COWIKI_MCP_PORT` | `--port` | `8080` | MCP server listen port |
| `COWIKI_BASE_URL` | `--server-url` | `http://localhost:3000` | cowiki-server REST API base URL |

> **Note:** Like `cli/`, `dotenvy` reads `.env` from the current working directory. Run from `cowiki-mcp-server/` or ensure `.env` is in your CWD.

## MCP Tools

| Tool | Description |
|------|-------------|
| `cowiki_ingest` | Ingest a source document (URL, text, or file) |
| `cowiki_compile` | Compile source documents into wiki pages using LLM |
| `cowiki_read` | Read a wiki page by slug |
| `cowiki_write` | Create or edit a wiki page |
| `cowiki_list` | List all wiki pages in main space |
| `cowiki_search` | Semantic search across wiki pages |
| `cowiki_submit` | Submit pages to review queue |
| `cowiki_review_list` | List pending submissions |
| `cowiki_review_get` | Get review details with diffs |
| `cowiki_review_decide` | Approve or reject a submission |

## Architecture

Pure HTTP proxy — no business logic. Each MCP tool call is forwarded to the cowiki-server REST API. User context is resolved from the MCP session's Authorization header.

## Future Work

- **Workspace tools** — `cowiki_workspace_list`, `cowiki_workspace_create`, `cowiki_workspace_info` (see CLI commands for reference)
- **Admin tools** — API key management, user administration
