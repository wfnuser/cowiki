# Antigravity CLI integration

## Goal

Replace the legacy Gemini CLI choice in the CoWiki desktop Agent launcher with
Google Antigravity CLI while preserving CoWiki's per-Space MCP isolation.

## User experience

- The Google agent is named **Antigravity CLI** and launches with `agy`.
- Existing saved `gemini` defaults migrate to `antigravity`; other saved agent
  choices remain unchanged.
- Live and Background sessions receive the same CoWiki maintenance protocol as
  the other supported agents.
- A missing `agy` binary produces the existing unsupported-command error.

## MCP integration

Antigravity discovers workspace MCP servers from `.agents/mcp_config.json` and
does not expose a launch-time MCP configuration flag. Before starting `agy`,
CoWiki writes a session-local MCP configuration into the selected working tree.
The `cowiki` entry starts the current desktop executable with:

```text
--mcp --space <current-space-slug>
```

If `.agents/mcp_config.json` already exists, CoWiki preserves unrelated MCP
servers and replaces only the `cowiki` entry. This makes each Live or Background
working tree point at its own Space without mutating Antigravity's global MCP
configuration. The workspace entry remains available for future sessions;
CoWiki never deletes user-owned configuration during terminal shutdown.

## Launch contract

CoWiki starts:

```text
agy --prompt-interactive "$COWIKI_AGENT_PROMPT"
```

The long prompt remains in an environment variable so the launch command stays
below macOS canonical terminal input limits.

## Compatibility and tests

- Frontend contracts cover the new identifier, label, command, and migration
  from `gemini`.
- Rust tests cover command validation, launch arguments, and MCP JSON merging
  without removing user-owned configuration.
- Codex, Claude Code, Grok, OpenCode, and Hermes behavior is unchanged.

## Non-goals

- Keeping a second legacy Gemini CLI entry.
- Writing Antigravity's global MCP configuration.
- Solving Google account or geographic availability restrictions.
