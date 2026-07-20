---
name: cowiki-space
description: Use when searching, maintaining, or reviewing a local CoWiki knowledge Space.
---

# Maintain a CoWiki Space

Markdown files are the source of truth. Git records durable history. CoWiki's
SQLite database is a rebuildable index; never edit it directly.

## Read before changing

1. Call `get_space_context` when the local read-only MCP is available.
2. Search before answering or claiming knowledge is absent.
3. Read the relevant pages and cite their relative paths.
4. If MCP is unavailable, inspect the Space with normal file and text-search
   tools. Do not invent remote commands.

A Space uses an arbitrary OKF hierarchy. Never assume fixed `wiki/`,
`entities/`, or `concepts/` directories. Prefer portable Markdown links;
preserve wikilinks when the Space already uses them.

## Write files, not APIs

Edit the Markdown files directly. Preserve unknown frontmatter fields and the
Space's established structure. Treat raw files under `.cowiki/sources/` as
immutable evidence. Keep affected links, `index.md`, and `log.md` coherent.

No local account, API key, or server is required. The retired standalone CLI
and Cloud commands are not part of the local Space workflow.

## Respect the execution mode

**Live mode:** work in the current Draft. Re-read each file immediately before
editing it. If a human or another Agent changed it, incorporate the latest
content; never silently overwrite their work.

**Background mode:** work only in the CoWiki-managed worktree. Do not commit,
checkout, merge, rebase, or push. CoWiki turns the resulting diff into an Agent
Change; the user can continue editing, request another pass, merge it into the
latest Draft, or discard it.

In either mode, do not create Git history or operate CoWiki review state. The
desktop app owns checkpoints, Agent Changes, merge, and discard.
