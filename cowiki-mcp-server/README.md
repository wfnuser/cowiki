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

### Personal Space & Review (10 tools)

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

### Workspace Management (5 tools)

| Tool | Description |
|------|-------------|
| `cowiki_workspace_list` | List all workspaces you belong to |
| `cowiki_workspace_create` | Create a new workspace |
| `cowiki_workspace_join` | Join a public workspace by slug |
| `cowiki_workspace_rename` | Rename a workspace (owner only) |
| `cowiki_workspace_delete` | Delete a workspace (owner only) |

### Member Management (4 tools)

| Tool | Description |
|------|-------------|
| `cowiki_workspace_invite` | Invite a user by email (owner only) |
| `cowiki_workspace_members` | List members of a workspace |
| `cowiki_workspace_remove_member` | Remove a member (owner only) |
| `cowiki_workspace_change_role` | Change a member's role (owner only) |

### Invitations (3 tools)

| Tool | Description |
|------|-------------|
| `cowiki_invitation_list` | List your pending invitations |
| `cowiki_invitation_accept` | Accept a pending invitation |
| `cowiki_invitation_reject` | Reject a pending invitation |

### Workspace-Scoped Wiki Operations (5 tools)

| Tool | Description |
|------|-------------|
| `cowiki_workspace_pages` | List all wiki pages in a workspace |
| `cowiki_workspace_read` | Read a wiki page in a workspace |
| `cowiki_workspace_write` | Create or edit a wiki page in a workspace |
| `cowiki_workspace_ingest` | Ingest a source document into a workspace |
| `cowiki_workspace_compile` | Compile source documents in a workspace using LLM |

## Architecture

Pure HTTP proxy — no business logic. Each MCP tool call is forwarded to the cowiki-server REST API. User context is resolved from the MCP session's Authorization header.

## Future Work

- **Admin tools** — API key management, user administration
- **CLI workspace commands** — Add workspace subcommands to `cli/` matching the MCP tools above
