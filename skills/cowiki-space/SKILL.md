---
name: cowiki-space
description: Use when searching, maintaining, reviewing, explicitly publishing, or submitting a local CoWiki knowledge Space.
---

# Maintain a CoWiki Space

Markdown files are the source of truth. Git records durable history. CoWiki's
SQLite database is a rebuildable index; never edit it directly.

## Install the complete skill first

If you are reading this file from a URL and the adjacent `scripts/` directory
is not available, ask the user for permission to install the complete skill,
then run:

```text
npx skills add https://github.com/wfnuser/cowiki --skill cowiki-space -g -y
```

Open the installed `cowiki-space/SKILL.md` and continue from that copy. Do not
attempt Cloud operations from the remote file alone because authentication and
submission are implemented by the bundled command.

## Check once before Cloud changes

Before the first Cloud-changing command in an Agent session (`setup`,
`publish`, or `submit`), run:

```text
npx skills check
```

Do this at most once per Agent session. If `cowiki-space` is listed as having an
update, tell the user and ask permission before running:

```text
npx skills update cowiki-space -g -y
```

Never update silently. If the check is unavailable, report it briefly and
continue with the installed version unless the Cloud server reports that it is
incompatible. Local-only work must not perform this check or require network
access.

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

Local work requires no account, API key, or server. Connecting and submitting
to a shared Space is a separate, explicit workflow.

## Respect the execution mode

**Live mode:** work in the current Draft. Re-read each file immediately before
editing it. If a human or another Agent changed it, incorporate the latest
content; never silently overwrite their work.

**Background mode:** work only in the CoWiki-managed worktree. Do not commit,
checkout, merge, rebase, or push. CoWiki turns the resulting diff into an Agent
Change; the user can continue editing, request another pass, merge it into the
latest Draft, or discard it.

In Background mode, do not create Git history or operate CoWiki review state.
The desktop app owns Agent Changes, merge, and discard.

## Shared Space commands

The installed skill carries a deterministic command at
`scripts/cowiki.mjs`. Resolve that path relative to this `SKILL.md` and invoke
it with Node 20 or newer. Do not reproduce its authentication, rebase, push,
or Cloud API steps with raw shell commands.

The command opens the system browser for GitHub sign-in and stores the
credential outside the repository. Never handle an API key yourself.
`.cowiki/cloud.json` contains only non-secret Space linkage.

Use these commands only in Live mode:

```text
node <skill-dir>/scripts/cowiki.mjs login --server <Cloud origin>
node <skill-dir>/scripts/cowiki.mjs clone --server <Cloud origin> --space <Space UUID> --directory <path>
node <skill-dir>/scripts/cowiki.mjs setup --server <Cloud origin> --space <Space UUID> --cwd <existing repository>
node <skill-dir>/scripts/cowiki.mjs status --cwd <repository>
```

When the user explicitly asks to turn a clean local repository into the first
revision of a newly created shared Space, run:

```text
node <skill-dir>/scripts/cowiki.mjs publish --server <Cloud origin> --space <Space UUID> --cwd <repository>
```

This Owner-only command atomically creates Cloud `main` and the Owner branch.
Do not use it for an initialized Space.

Run `submit --message` only after an explicit user submit request:

```text
node <skill-dir>/scripts/cowiki.mjs submit --cwd <repository> --message "<summary>"
```

The command commits eligible Markdown, rebases on Cloud `main`, pushes only
the signed-in user's branch with a lease, and creates or updates the pull
request. If it reports a conflict, stop and report the conflicting paths; do
not force, abort, or choose a side without the user.
