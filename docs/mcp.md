# Local MCP contract

CoWiki's current MCP server is embedded in the CoWiki desktop app. It gives an
Agent read-only retrieval over one local Space while Markdown remains the
source of truth.

There is no account, API key, HTTP server, or separate MCP process to configure.
When Codex or Claude Code is started from the Agent panel, CoWiki launches its
own executable in stdio MCP mode and scopes it to the selected Space.

```text
Agent ──stdio──> CoWiki desktop executable ──> local SQLite index
  │                                                │
  └──────────────── edits Markdown files directly ┘
```

SQLite accelerates retrieval and can be rebuilt from the files. MCP does not
own writes, Git history, or review state.

## Read-only tools

| Tool | Purpose |
| --- | --- |
| `get_space_context` | Return the selected Space root and OKF context. |
| `search_pages` | Search indexed titles and Markdown bodies. |
| `get_page` | Read a page by its Space-relative path. |
| `list_backlinks` | Find pages that link to a document. |

The embedded server deliberately has no write, ingest, commit, branch, merge,
or review tools. Agents edit ordinary Markdown with their filesystem tools.
The desktop app owns Live versus Background mode, Agent Changes, checkpoints,
merge, and discard.

## Agent workflow

The authoritative workflow is distributed as the
[`cowiki-space` skill](../skills/cowiki-space/SKILL.md):

- search and read before making knowledge claims;
- preserve the Space's arbitrary OKF hierarchy and unknown frontmatter;
- edit Markdown directly and never edit the derived SQLite database;
- treat `.cowiki/sources/` as immutable raw evidence;
- re-read before writes in Live mode;
- leave Git and review operations to CoWiki in Background mode.

Codex and Claude Code receive the embedded MCP configuration automatically.
Agents without MCP access can still work directly in the selected folder and
use normal file search, following the same skill contract.

## Manual development launch

The app normally supplies these arguments. For protocol testing, run the built
desktop executable over stdio:

```bash
web/src-tauri/target/debug/cowiki-desktop --mcp --space <local-space-slug>
```

For an installed app, the binary is
`CoWiki.app/Contents/MacOS/cowiki-desktop`. The executable resolves the Space
through CoWiki's local metadata directory. This interface is an implementation
detail for local Agent integration, not a general-purpose CLI.

## Cloud MCP

A future remote MCP may expose authenticated shared-Space capabilities, but it
will be a different contract. It must not replace local file editing or make a
local Space depend on a backend.

The top-level `cowiki-mcp-server/` crate is a legacy REST-proxy experiment. It
is not the current desktop MCP and should not be used for local Spaces.
