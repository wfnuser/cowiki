# Contributing to CoWiki

Thanks for helping explore how humans and many agents should maintain shared
knowledge. Product criticism, bug reports, documentation, design experiments,
and code are all useful at this stage.

## Before you start

- Search existing issues and pull requests before opening a duplicate.
- For a large product or architecture change, open an issue first so the
  workflow and data-model implications can be discussed.
- Keep the local-first boundary intact: the desktop app must remain useful
  without an account, hosted API, or external database.
- Treat Markdown and Git as authoritative. SQLite data must remain derived and
  rebuildable.

## Branch strategy

```
feature/* ──PR──▶ dev ──PR──▶ main (release)
bugfix/*   ──PR──▶ dev ──PR──▶ main (release)
```

- **`dev`** — Integration branch. All feature and bugfix PRs target `dev`.
- **`main`** — Release branch. Only merged from `dev` when ready to release. Always stable.

## Development setup

### Prerequisites

- Node.js 24+
- Rust stable
- Platform dependencies required by Tauri 2
- Xcode Command Line Tools when building on macOS

### Desktop app

```bash
git clone https://github.com/wfnuser/cowiki.git
cd cowiki
git checkout dev
cd web
npm ci
npm run desktop:dev
```

The desktop app runs its local engine in-process. Do not start PostgreSQL or a
separate backend for local development.

### Tests

```bash
cd web
npm test
npm run build
cargo fmt --all --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

## Pull request workflow

1. Create a feature/bugfix branch from `dev`.
2. Keep changes focused and include tests for behavior changes.
3. Run the relevant frontend and Rust checks locally.
4. Open a PR targeting `dev` and explain the user-visible outcome.
5. Include screenshots or a short recording for UI changes.

## Documentation expectations

- Describe capabilities that exist today separately from planned cloud work.
- Preserve the distinction between the upstream OKF standard and CoWiki's own
  producer conventions.
- Update the README when a change affects setup, the product loop, or the
  local data model.
- Prefer links to focused documents over duplicating long explanations.

## Commit and review guidelines

- Use clear, scoped commit messages.
- Never commit credentials, local Space data, SQLite indexes, or generated build
  output.
- Do not silently rewrite user Markdown or Git history.
- Keep agent changes inspectable and reversible.
