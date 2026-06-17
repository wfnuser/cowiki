<p align="center">
  <h1 align="center">CoWiki</h1>
  <p align="center"><strong>A team wiki that builds itself.</strong></p>
  <p align="center">
    Open-source LLM Wiki for teams — agents contribute, humans review, all version-controlled.
  </p>
</p>

<p align="center">
  <a href="https://github.com/wfnuser/cowiki/blob/main/LICENSE"><img src="https://img.shields.io/badge/License-Apache_2.0-blue.svg" alt="License"></a>
  <a href="https://github.com/wfnuser/cowiki"><img src="https://img.shields.io/github/stars/wfnuser/cowiki?style=social" alt="Stars"></a>
</p>

---

## The Problem

Karpathy's [LLM Wiki](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f) pattern is powerful — instead of re-discovering knowledge at query time, compile it once into a persistent wiki. But every implementation today is single-player. Your team's agents each learn independently, and nothing compounds.

CoWiki makes LLM Wiki multiplayer:

- **Your space, your pace** — Ingest sources and compile wiki pages in your personal space. No one sees your drafts until you're ready.
- **Submit for review** — When you're happy, submit to the shared wiki. The system checks for duplicates and generates a summary.
- **Review like a PR** — Reviewers see an LLM-generated summary, full diff, and duplicate warnings. Approve or reject with one click.
- **Agents welcome** — AI agents connect via MCP, contribute knowledge alongside humans, and follow the same review process.
- **Git under the hood** — Every change is version-controlled. Full history, rollback, blame — but you never touch Git directly.

## How It Works

```
  You / Your Agent                              Your Team

  1. Ingest                                  5. Pages appear in
     URL, text, file                            the shared wiki
         |                                          |
  2. Compile                                 6. Anyone can browse
     AI turns sources                           and search
     into wiki pages
         |
  3. Edit drafts
     in personal space
         |
  4. Submit ──────────> Review ──────────> Shared Wiki
                        (diff + summary       (version-controlled,
                         + dedup check)        searchable)
```

## Quick Start

```bash
# for ubuntu
sudo apt install fd-find ripgrep

npm install -g @earendil-works/pi-coding-agent
pi install npm:pi-mcp-adapter

npm install -g @anthropic-ai/claude-code

# Clone
git clone https://github.com/wfnuser/cowiki.git
cd cowiki

# Start PostgreSQL using default volume for data persistence 
# (in /var/lib/docker/volumes/pgdata/cowiki_pgdata)
docker compose up -d

# Or you can specify your own Postgres config, below will store data in /path/to/data on host
# PGDATA_TYPE=bind PGDATA_SOURCE=/path/to/data docker compose up -d

# start from copy
cp cowiki.conf.example cowiki.conf
# edit cowiki.conf to set cowiki

# Start the server
cargo run

# Start the MCP server (in another terminal)
cd cowiki-mcp-server && cargo run  # or: cargo mcp

# Start the frontend (in another terminal)
cd web && npm install && npm run dev
```

Open [http://localhost:5173](http://localhost:5173)

## Features

### Personal Space
Ingest sources (URLs, text, files) and compile them into structured wiki pages using LLM. Edit freely — only you can see your drafts.

### Shared Wiki
The team's knowledge base. Pages enter through review only. Semantic search powered by pgvector finds what you need.

### Review Workflow
Every submission shows an LLM-generated summary, full file diff, and duplicate warnings. Approve, reject, or request changes.

### Semantic Search
Search by meaning, not just keywords. Powered by OpenAI embeddings and PostgreSQL pgvector.

### Version Control
Git tracks every change under the hood. Full history, per-page diffs, and the ability to see who contributed what and when.

## Architecture

```
┌─────────────────────────────────┐
│  React + TypeScript + Tailwind  │
│  shadcn/ui · Milkdown editor   │
└──────────────┬──────────────────┘
               │
┌──────────────▼──────────────────┐
│       Rust Backend (axum)       │
│                                 │
│  Pages · Ingest · Compile       │
│  Submit · Review · Search       │
├────────────────┬────────────────┤
│  Git (files)   │  PostgreSQL    │
│  branches,     │  pgvector,     │
│  version ctrl  │  metadata      │
└────────────────┴────────────────┘
```

## Tech Stack

| Layer | Choice |
|-------|--------|
| Backend | Rust, axum |
| Frontend | React, TypeScript, Vite, Tailwind, shadcn/ui |
| Database | PostgreSQL + pgvector |
| Version Control | Git (libgit2) |
| LLM | OpenAI API |
| Agent Protocol | MCP (coming soon) |

## Roadmap

- [ ] MCP Server — agents connect directly
- [ ] Multi-user auth and API keys
- [ ] Markdown editor (Milkdown) in the browser
- [ ] Wikilink auto-resolution
- [ ] Incremental compilation (skip unchanged sources)
- [ ] Knowledge graph visualization
- [ ] Desktop app (Tauri)
- [ ] CRDT for real-time co-editing

## Contributing

We welcome contributions! See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## License

[Apache License 2.0](LICENSE)
