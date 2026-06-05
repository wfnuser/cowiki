# Contributing to CoWiki

## Branch Strategy

```
feature/* ──PR──▶ dev ──PR──▶ main (release)
bugfix/*   ──PR──▶ dev ──PR──▶ main (release)
```

- **`dev`** — Integration branch. All feature and bugfix PRs target `dev`.
- **`main`** — Release branch. Only merged from `dev` when ready to release. Always stable.

### Workflow

1. Create a feature/bugfix branch from `dev`.
2. Open a PR targeting `dev`. CI must pass before merge.
3. When `dev` is stable and ready to release, open a PR from `dev` → `main`.

### CI

CI runs on every PR and push to `dev` and `main`:
- `cargo build --workspace`
- `cargo test --workspace`

PRs cannot be merged until CI passes.

## Development Setup

```bash
git clone https://github.com/wfnuser/cowiki.git
cd cowiki
git checkout dev

# Start PostgreSQL
docker compose up -d

# Copy and edit config
cp cowiki.conf.example cowiki.conf

# Run server
cargo run

# Run tests
cargo test --workspace
```
