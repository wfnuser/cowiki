<p align="center">
  <img src="web/public/cowiki-logo.svg" width="88" alt="CoWiki logo" />
</p>

<h1 align="center">CoWiki: An LLM Wiki, but multiplayer.</h1>

<p align="center">
  <a href="https://github.com/wfnuser/cowiki/actions/workflows/macos-desktop.yml"><img src="https://img.shields.io/github/actions/workflow/status/wfnuser/cowiki/macos-desktop.yml?branch=dev&label=macOS%20build" alt="macOS build" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-EF5A29" alt="Apache 2.0 license" /></a>
  <a href="docs/okf-v0.1.md"><img src="https://img.shields.io/badge/OKF-v0.1-EF5A29" alt="Open Knowledge Format v0.1" /></a>
  <a href="https://github.com/wfnuser/cowiki"><img src="https://img.shields.io/github/stars/wfnuser/cowiki?style=flat&color=EF5A29" alt="GitHub stars" /></a>
  <a href="https://github.com/wfnuser/cowiki/issues"><img src="https://img.shields.io/github/issues/wfnuser/cowiki?color=6B625C" alt="Open issues" /></a>
  <a href="https://github.com/wfnuser/cowiki/graphs/contributors"><img src="https://img.shields.io/github/contributors/wfnuser/cowiki?color=6B625C" alt="Contributors" /></a>
</p>

<p align="center">
  <img src="docs/assets/cowiki-desktop.png" alt="CoWiki reviewing local Markdown changes beside an embedded Claude Code terminal" />
</p>

<p align="center">
  <strong>A local-first, Git-based workspace where teams and their agents compile sources into a portable, reviewable, and shareable LLM Wiki.</strong>
  <br />
  <sub>Bring your own agents. Turn files and URLs into linked OKF knowledge, review every diff, and keep only what you trust.</sub>
</p>

> **macOS-first alpha:** local Spaces work today. Cloud collaboration comes next.

## Features

| Feature | What it means |
| --- | --- |
| **Local Spaces** | A Space is an ordinary folder. No account, server, or external database is required. |
| **Agent-compiled Wiki** | Add URLs, PDFs, documents, spreadsheets, or slides. Agents compile them into structured, linked knowledge. |
| **Bring your own agents** | Use the built-in Codex and Claude terminals, or let Grok, Antigravity, and other file/MCP-capable agents work on the same Space. |
| **Git-based review** | Inspect human and agent edits as diffs, then merge, discard, commit, or checkpoint them. |
| **OKF-native** | CoWiki follows [Google's Open Knowledge Format v0.1](docs/okf-v0.1.md), keeping knowledge readable, linked, and portable. |
| **Collaboration-ready** | The same local Space can later gain cloud publishing, permissions, and team review. |

## Ingest. Compile. Review.

<p align="center">
  <img src="docs/assets/compilation-pipeline.svg" alt="Ingest, compile, lint, and review pipeline" width="100%" />
</p>

Drop source material into a Space. Your agents compile it into linked OKF
knowledge, check its structure, and return a Git diff for human review. Keep
the result, continue editing it, or discard the change as a unit.

## Your Space is a folder

```text
research-space/
├── index.md
├── architecture.md
├── projects/
│   └── cowiki.md
├── .cowiki/
│   └── sources/
└── .git/
```

Markdown and Git are the source of truth. SQLite only stores a rebuildable
local search and backlink index. You can open the same files in another
editor, use normal Git tools, or leave CoWiki without exporting anything.

CoWiki aligns Spaces with [Open Knowledge Format v0.1](docs/okf-v0.1.md) and
preserves frontmatter fields it does not understand.

## Local-first, collaboration-ready

CoWiki is local-first, not local-only. Today the desktop app keeps a complete
offline Space and supports local human–agent review. The next layer will let
you publish that same Space for team permissions, browser access, asynchronous
review, and reusable remote MCP—without changing its portable source format.

## Run from source

Requires macOS, Xcode Command Line Tools, [Node.js 24+](https://nodejs.org/),
and [Rust stable](https://rustup.rs/). Install Codex CLI and/or Claude Code to
use the embedded Agent panel.

```bash
git clone https://github.com/wfnuser/cowiki.git
cd cowiki/web
npm ci
npm run desktop:dev
```

Create a Space with an empty local folder, or import an existing folder of
Markdown files. The desktop app runs as a complete standalone workspace.

Agents launched by the app receive CoWiki's read-only local MCP for retrieval
and edit the Space's Markdown files directly. The
[`cowiki-space` skill](skills/cowiki-space/SKILL.md) defines the same contract
for external Agents; local work never requires a CoWiki account, API key, or
backend.

## Roadmap

- Make the macOS alpha easier to install and trust.
- Improve local review and conflict resolution for many agents.
- Publish local Spaces for team permissions, sync, and browser review.
- Offer compatible remote MCP and expand desktop platform support.

## Contributing

CoWiki is early, and the collaboration model is still an open design problem.
Issues, product criticism, experiments, and code are welcome. See
[CONTRIBUTING.md](CONTRIBUTING.md) to get started.

<img width="161" height="247" alt="9f97463e4dec44b4681d4d3ef853448f" src="https://github.com/user-attachments/assets/fa08defd-55f9-4c43-aa13-d17c78a17228" />


## License

CoWiki is licensed under the [Apache License 2.0](LICENSE).
