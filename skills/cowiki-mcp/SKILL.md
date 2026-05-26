---
name: cowiki-mcp
description: >
  Interactive setup wizard for cowiki MCP server. Guides through API key registration
  and auto-configures MCP client (VS Code / Claude Code).
  Activate when: cowiki mcp, setup cowiki, configure cowiki, mcp setup, 配置 cowiki,
  cowiki 配置, connect cowiki, add cowiki mcp.
argument-hint: "[--vscode | --claude-code]"
---

# cowiki MCP Setup

Interactive installation wizard that configures the cowiki MCP server for your AI tools. The MCP server exposes 10 wiki tools (ingest, compile, read, write, search, list, submit, review) to your AI agent.

## When to Use
- First-time cowiki MCP setup
- Switching between VS Code and Claude Code
- API key rotation or reconfiguration
- User says "setup cowiki", "configure cowiki MCP", "connect cowiki"

## When NOT to Use
- Already configured and working → skip
- Only need docs → read `reference/architecture.md`
- Troubleshooting tool errors → use `@debugger`

## Interactive Hook Protocol

**MANDATORY**: Use `vscode_askQuestions` for ALL user-facing questions (when available).
If `vscode_askQuestions` is NOT available, present numbered markdown options.

### Hook Points

| Step | Trigger | Question | Options |
|------|---------|----------|---------|
| 1. Detect | Start | Environment detection | Auto-detect / VS Code / Claude Code |
| 2. API Key | After detection | How to get API key? | Register on web / I already have one |
| 3. Server URL | After key | MCP server URL | Local (localhost:8080) / Remote / Custom |
| 4. Confirm | Before write | Review config before writing | Confirm / Edit / Cancel |
| 5. Verify | After write | Test connection | Yes / Skip |

## Workflow

### Step 1: Detect Environment

Auto-detect which MCP client to configure:
- Check if `.vscode/` directory exists → VS Code mode
- Check if `~/.claude/` or `~/.claude.json` exists → Claude Code mode
- If both exist, **HOOK**: ask user which to configure

### Step 2: API Key Registration

**HOOK**: Ask user how they want to obtain the API key:
- **Register on web** (recommended): Open browser to cowiki registration page
- **I already have one**: Prompt to paste the key

If "Register on web": guide user through:
1. Navigate to cowiki web UI registration page
2. Complete registration (name + email)
3. Copy the generated API key (format: `cw_` + 32 hex chars)
4. Paste back into the chat

**CRITICAL**: Never echo or log the API key. Treat it like a password.
If user pastes the key in chat, remind them to use the secure input method.

### Step 3: Collect Server URL

**HOOK**: Ask for MCP server endpoint:
- `http://localhost:8080/` (local dev, default)
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

Scripts accept parameters:
- `--api-key <key>` or `-ApiKey <key>`: API key (required)
- `--url <url>` or `-Url <url>`: Server URL (default: `http://localhost:8080/`)
- `--vscode` / `-VSCode`: Configure VS Code
- `--claude-code` / `-ClaudeCode`: Configure Claude Code

### Step 5: Verify Connection

After configuration is written, verify the MCP tools are accessible:

1. **VS Code**: Check the MCP server appears in the Copilot chat tools list
2. **Claude Code**: Run `claude mcp list` to verify cowiki-mcp appears

**HOOK**: Ask user if they want to run a quick smoke test:
- List tools via `cowiki_list`
- Search via `cowiki_search`

## Target Configuration

### VS Code (`.vscode/mcp.json`)

```json
{
  "servers": {
    "cowiki-mcp": {
      "url": "http://localhost:8080/",
      "type": "http",
      "headers": {
        "Authorization": "Bearer <YOUR_API_KEY>"
      }
    }
  }
}
```

### Claude Code (`~/.claude.json` or `~/.claude/mcp.json`)

```json
{
  "mcpServers": {
    "cowiki-mcp": {
      "type": "http",
      "url": "http://localhost:8080/",
      "headers": {
        "Authorization": "Bearer <YOUR_API_KEY>"
      }
    }
  }
}
```

## Available Tools (Post-Setup)

After successful setup, these 10 cowiki tools become available:

| Tool | Description |
|------|-------------|
| `cowiki_ingest` | Ingest source document to personal space |
| `cowiki_compile` | Compile sources → wiki pages via LLM |
| `cowiki_read` | Read a wiki page by slug |
| `cowiki_write` | Create or edit a wiki page |
| `cowiki_list` | List all wiki pages |
| `cowiki_search` | Semantic search across wiki |
| `cowiki_submit` | Submit personal pages for review |
| `cowiki_review_list` | List pending reviews |
| `cowiki_review_get` | Get review details + diffs |
| `cowiki_review_decide` | Approve or reject submission |

## Troubleshooting

| Symptom | Likely Cause | Fix |
|---------|-------------|-----|
| Tools not appearing | MCP server not running | Start `cowiki-mcp` (see cowiki-mcp-server/README.md) |
| 401 Unauthorized | Invalid/missing API key | Re-run setup with new key |
| Connection refused | Wrong URL or server down | Verify `--url` and server status |
| Tool returns error | Backend REST API down | Ensure cowiki-server is running on :3000 |

## Security Notes
- API keys are stored in MCP config files — keep these files secure
- Never commit `.vscode/mcp.json` with real API keys to version control
- Use `.gitignore` to exclude MCP config if it contains secrets
- The setup scripts create config files with restrictive permissions where possible

## Related Docs

All reference material is self-contained within this skill directory:

| File | Content |
|------|------|
| `reference/architecture.md` | MCP server architecture, protocol, tools reference, error codes, config examples |
| `scripts/setup.sh` | Linux/macOS MCP configuration script |
| `scripts/setup.ps1` | Windows PowerShell MCP configuration script |
