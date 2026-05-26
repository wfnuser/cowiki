---
name: cowiki-mcp
description: >
  Interactive setup wizard for cowiki MCP server. Guides through API key registration
  and auto-configures MCP client (VS Code / Claude Code). Also serves as the complete
  reference for all 27 cowiki MCP tools, architecture, and troubleshooting.
  Activate when: cowiki mcp, setup cowiki, configure cowiki, mcp setup, 配置 cowiki,
  cowiki 配置, connect cowiki, add cowiki mcp.
argument-hint: "[--vscode | --claude-code]"
---

# cowiki MCP — Setup & Reference

Self-contained guide for setting up and using the cowiki MCP server. Covers installation, tool reference (27 tools), architecture, error codes, and troubleshooting.

## Architecture

The MCP server is a standalone process — an MCP→REST proxy that forwards tool calls to the cowiki-server REST API:

```
MCP Client ──→ cowiki-mcp (:8080) ──→ cowiki-server REST API (:3000) ──→ core/db
              (rmcp + hyper)              (axum)
```

- **Transport**: Streamable HTTP (MCP 2025-03-26)
- **SDK**: rmcp v1.7
- **Endpoint**: `http://localhost:8080/mcp`
- **Auth**: `Authorization: Bearer <api_key>` (required on all requests, including `initialize`)
- **Config**: `COWIKI_MCP_PORT` (default 8080), `COWIKI_BASE_URL` (default `http://localhost:3000`)

## When to Use
- First-time cowiki MCP setup
- Switching between VS Code and Claude Code
- API key rotation or reconfiguration
- Looking up available tools and their parameters
- Troubleshooting connection or tool errors

## When NOT to Use
- Already configured and working → skip
- Need source code details → see `cowiki-mcp-server/src/server.rs`

---

## Setup Wizard

### Interactive Hook Protocol

**MANDATORY**: Use `vscode_askQuestions` for ALL user-facing questions (when available).
If `vscode_askQuestions` is NOT available, present numbered markdown options.

| Step | Trigger | Question | Options |
|------|---------|----------|---------|
| 1. Detect | Start | Environment detection | Auto-detect / VS Code / Claude Code |
| 2. API Key | After detection | How to get API key? | Register on web / I already have one |
| 3. Server URL | After key | MCP server URL | Local (localhost:8080) / Remote / Custom |
| 4. Confirm | Before write | Review config before writing | Confirm / Edit / Cancel |
| 5. Verify | After write | Test connection | Yes / Skip |

### Step 1: Detect Environment

Auto-detect which MCP client to configure:
- Check if `.vscode/` directory exists → VS Code mode
- Check if `~/.claude/` or `~/.claude.json` exists → Claude Code mode
- If both exist, **HOOK**: ask user which to configure

### Step 2: API Key Registration

**HOOK**: Ask user how they want to obtain the API key.

If "Register on web": guide user through:
1. Navigate to cowiki web UI registration page
2. Complete registration (name + email)
3. Copy the generated API key (format: `cw_` + 32 hex chars)
4. Paste back into the chat

API keys can also be obtained via REST API:
```bash
curl -X POST http://localhost:3000/api/auth/register \
  -H "Content-Type: application/json" \
  -d '{"name":"my-agent","email":"agent@example.com"}'
# → {"api_key": "cw_xxxxxxxxxx", ...}
```

> **CRITICAL**: Never echo or log the API key. Treat it like a password.

### Step 3: Collect Server URL

**HOOK**: Ask for MCP server endpoint:
- `http://localhost:8080` (local dev, default)
- Remote URL (production deployment)
- Custom URL

### Step 4: Run Setup Script

Execute the appropriate setup script based on OS and client:

**VS Code mode**:
- Linux/macOS: `bash skills/cowiki-mcp/scripts/setup.sh --vscode`
- Windows: `powershell -File skills/cowiki-mcp/scripts/setup.ps1 -VSCode`

**Claude Code mode**:
- Linux/macOS: `bash skills/cowiki-mcp/scripts/setup.sh --claude-code`
- Windows: `powershell -File skills/cowiki-mcp/scripts/setup.ps1 -ClaudeCode`

Script parameters:
- `--api-key <key>` / `-ApiKey <key>`: API key (required)
- `--url <url>` / `-Url <url>`: Server URL (default: `http://localhost:8080`)
- `--vscode` / `-VSCode`: Configure VS Code
- `--claude-code` / `-ClaudeCode`: Configure Claude Code

### Step 5: Verify Connection

After configuration is written:
1. **VS Code**: Check the MCP server appears in the Copilot chat tools list
2. **Claude Code**: Run `claude mcp list` to verify cowiki-mcp appears

**HOOK**: Ask user if they want to run a quick smoke test (e.g., `cowiki_list`).

---

## Client Configuration

### VS Code (`.vscode/mcp.json`)

```json
{
  "servers": {
    "cowiki-mcp": {
      "url": "http://localhost:8080/mcp",
      "type": "http",
      "headers": {
        "Authorization": "Bearer <YOUR_API_KEY>"
      }
    }
  }
}
```

### Claude Desktop (`~/.claude.json` or `~/.claude/mcp.json`)

```json
{
  "mcpServers": {
    "cowiki-mcp": {
      "type": "http",
      "url": "http://localhost:8080/mcp",
      "headers": {
        "Authorization": "Bearer <YOUR_API_KEY>"
      }
    }
  }
}
```

### Generic MCP Client

- **Endpoint**: `http://localhost:8080/mcp`
- **Protocol**: MCP 2025-03-26 (Streamable HTTP)
- **Auth**: `Authorization: Bearer <api_key>`

---

## Tool Reference (27 tools)

All tools are HTTP proxies — each MCP tool calls a corresponding REST endpoint on cowiki-server. No business logic is duplicated.

### Personal Space & Knowledge (7 tools)

| Tool | Method | REST Endpoint | Parameters |
|------|--------|---------------|------------|
| `cowiki_ingest` | POST | `/api/ingest` | `source_type`, `content`, `filename?` |
| `cowiki_compile` | POST | `/api/compile` | (none) |
| `cowiki_read` | GET | `/api/pages/{slug}` | `slug` |
| `cowiki_write` | POST | `/api/pages` | `slug`, `body`, `title?`, `summary?` |
| `cowiki_list` | GET | `/api/pages` | (none) |
| `cowiki_search` | GET | `/api/search?q={query}&limit={limit}` | `query`, `limit?` |
| `cowiki_submit` | POST | `/api/submit` | `page_slugs` |

> Write operations auto-resolve to the user's personal branch (`user/{id}`). Reads/searches default to `main`.

### Review (3 tools)

| Tool | Method | REST Endpoint | Parameters |
|------|--------|---------------|------------|
| `cowiki_review_list` | GET | `/api/reviews` | (none) |
| `cowiki_review_get` | GET | `/api/reviews/{id}` | `id` |
| `cowiki_review_decide` | POST | `/api/reviews/{id}` | `id`, `action` |

### Workspace Management (5 tools)

| Tool | Method | REST Endpoint | Parameters |
|------|--------|---------------|------------|
| `cowiki_workspace_list` | GET | `/api/workspaces` | (none) |
| `cowiki_workspace_create` | POST | `/api/workspaces` | `name`, `slug`, `visibility?` |
| `cowiki_workspace_join` | POST | `/api/workspaces/{slug}/join` | `workspace_slug` |
| `cowiki_workspace_rename` | POST | `/api/workspaces/{slug}/rename` | `workspace_slug`, `name` |
| `cowiki_workspace_delete` | DELETE | `/api/workspaces/{slug}` | `workspace_slug` |

### Member Management (4 tools)

| Tool | Method | REST Endpoint | Parameters |
|------|--------|---------------|------------|
| `cowiki_workspace_invite` | POST | `/api/workspaces/{slug}/invite` | `workspace_slug`, `email`, `role?` |
| `cowiki_workspace_members` | GET | `/api/workspaces/{slug}/members` | `workspace_slug` |
| `cowiki_workspace_remove_member` | POST | `/api/workspaces/{slug}/members/remove` | `workspace_slug`, `user_id` |
| `cowiki_workspace_change_role` | POST | `/api/workspaces/{slug}/members/role` | `workspace_slug`, `user_id`, `role` |

### Invitations (3 tools)

| Tool | Method | REST Endpoint | Parameters |
|------|--------|---------------|------------|
| `cowiki_invitation_list` | GET | `/api/invitations/pending` | (none) |
| `cowiki_invitation_accept` | POST | `/api/invitations/{id}/accept` | `invitation_id` |
| `cowiki_invitation_reject` | POST | `/api/invitations/{id}/reject` | `invitation_id` |

### Workspace-Scoped Wiki (5 tools)

| Tool | Method | REST Endpoint | Parameters |
|------|--------|---------------|------------|
| `cowiki_workspace_pages` | GET | `/api/workspaces/{slug}/pages` | `workspace_slug` |
| `cowiki_workspace_read` | GET | `/api/workspaces/{slug}/pages/{page_slug}` | `workspace_slug`, `page_slug` |
| `cowiki_workspace_write` | POST | `/api/workspaces/{slug}/pages` | `workspace_slug`, `slug`, `body`, `title?`, `summary?` |
| `cowiki_workspace_ingest` | POST | `/api/workspaces/{slug}/ingest` | `workspace_slug`, `source_type`, `content`, `filename?` |
| `cowiki_workspace_compile` | POST | `/api/workspaces/{slug}/compile` | `workspace_slug` |

### Typical Workflows

```
Personal:    ingest → compile → read → write → submit
Search:      search → read → list
Review:      review_list → review_get → review_decide
Workspace:   workspace_list → workspace_create → workspace_invite
             → workspace_pages → workspace_write
             → workspace_ingest → workspace_compile
Invitations: invitation_list → invitation_accept / invitation_reject
```

---

## Error Codes

| JSON-RPC Code | Meaning | Common Cause |
|:------------:|---------|--------------|
| `-32700` | Parse error | Malformed request body |
| `-32602` | Invalid params | Wrong/missing parameter types |
| `-32603` | Internal error | Backend API down, network issues, 404 |
| `-32001` | Unauthorized | Missing or invalid Authorization header |
| `-32002` | Not found | Slug doesn't exist |
| `-32003` | Forbidden | Insufficient permissions (e.g., not owner) |

---

## Troubleshooting

| Symptom | Likely Cause | Fix |
|---------|-------------|-----|
| Tools not appearing | MCP server not running | Start `cowiki-mcp`: `cd cowiki-mcp-server && cargo run` |
| 401 Unauthorized | Invalid/missing API key | Re-run setup with new key; ensure `Authorization: Bearer <key>` on ALL requests |
| Connection refused | Wrong URL or server down | Verify `--url` and server status; check `pgrep -f cowiki-mcp` |
| Tool returns error | Backend REST API down | Ensure cowiki-server is running: `curl http://localhost:3000/api/health` |
| `-32603 API 404` | Trailing slash in URL | Ensure `COWIKI_BASE_URL` has no trailing slash (e.g., `http://localhost:3000`) |
| Session expired | MCP session timed out | Re-initialize: send a new `initialize` request |

### Quick Health Check

```bash
# Check MCP server process
pgrep -f cowiki-mcp

# Check backend server
curl http://localhost:3000/api/health
# Expected: ok

# Start MCP server if needed
cd cowiki-mcp-server && cargo run
```

---

## Security Notes
- API keys are stored in MCP config files — keep these files secure
- Never commit `.vscode/mcp.json` with real API keys to version control
- Use `.gitignore` to exclude MCP config if it contains secrets
- The setup scripts create config files with restrictive permissions where possible
- The `cowiki-mcp` binary uses environment variables only (no config file on disk for credentials)
