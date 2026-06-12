# CLI Skill Decoupling, Dual-Path Workflow & Multi-Directory Wiki

**Date:** 2026-06-12
**Status:** design complete

## Overview

Three changes in one spec:

1. **Decouple `guide.md` from setup** — `guide.md` removed; SKILL.md is the single source of truth for agent setup
2. **Dual-path content workflow in SKILL.md** — cloud compile (ingest → compile) vs local agent compile (read → analyze → write), with `cowiki write` repurposed for local agent use
3. **Multi-directory wiki with extracted lib layer** — add `entities/` and `concepts/` directories alongside `wiki/`; extract `WikiFs` abstraction for reusable directory operations

## Part A: CLI Skill Decoupling

### Current State

`cli/guide.md` (123 lines) duplicates the full setup flow (env check → CLI install → API key → verify → skill install) that already lives in `cli/skills/cowiki-cli/SKILL.md`. Two files say the same thing.

### Design

- **Delete `cli/guide.md`.** The single entry point is the skill files:
  - Remote: `https://cowiki.app/skill.md`
  - Local (post-install): `cli/skills/cowiki-cli/SKILL.md`
- **SKILL.md** is the single source of truth — it contains the complete bootstrap flow (Steps 1-5)
- Agent reads SKILL.md, follows its instructions autonomously

### File Changes

| File | Action |
|------|--------|
| `cli/guide.md` | Delete |
| `cli/skills/cowiki-cli/SKILL.md` | Rewrite — add dual-path workflow (see Part B) |

## Part B: Dual-Path Content Workflow

### Current State

`commands.md` describes `cowiki write` as "Write a wiki page" with no guidance on when to use ingest/compile vs write. `SKILL.md` has no content workflow section.

### Design

Two paths for agents to add content:

#### Path 1: Cloud Compile

For external URLs, large documents, or structured content that benefits from AI parsing:

```
1. cowiki ingest -w <ws> --type url --content "<url>"
2. cowiki compile -w <ws> [--timeout 300]
3. Verify: cowiki list -w <ws> / cowiki read -w <ws> <page>
```

The cloud agent handles source parsing, entity extraction, and page generation. Always writes to `wiki/`.

#### Path 2: Local Agent Compile

For content that needs cross-references to existing wiki pages, or simple text that doesn't warrant cloud AI:

```
1. cowiki read -w <ws> <related-page>    # gather context
2. cowiki list -w <ws> --dir all         # understand the wiki structure
3. Agent analyzes and structures:
   - Extract entities (people, projects, events) → write to entities/
   - Extract concepts (patterns, decisions, conventions) → write to concepts/
   - Build [[cross-references]] to existing pages
4. cowiki write -w <ws> <slug> --path entities/ --body "..."
   cowiki write -w <ws> <slug> --path concepts/ --body "..."
```

Agent discretion: for trivial content, agent may skip compile and write directly.

#### `cowiki write` semantic clarification

- `write` creates a new page (if slug doesn't exist) or edits an existing page
- It is the primary tool for local agent output
- Use `--path` to target which directory (see Part C)
- For cloud AI processing of large sources, prefer Path 1 (ingest → compile)

### SKILL.md New Section

After Step 5 (Local Skill Bundle), add:

```
## Content Workflow: Two Paths

### Path 1: Cloud Compile
For external URLs, large documents, structured content:
1. cowiki ingest -w <ws> --type url --content "<url>"
2. cowiki compile -w <ws>
3. Verify results

The cloud agent handles parsing and page generation.

### Path 2: Local Agent Compile
For content connecting to existing wiki knowledge:

1. Gather context: cowiki read / cowiki list
2. Analyze: extract entities and concepts
3. Build cross-references with [[page-name]] syntax
4. Write: cowiki write -w <ws> <slug> --path <entities|concepts> --body "..."

Use your judgment — skip compile for simple content.
```

### commands.md Changes

| Command | Updated Description |
|---------|---------------------|
| `cowiki ingest` | Add note: "Use with `cowiki compile` for the cloud compile workflow. See SKILL.md." |
| `cowiki compile` | Add note: "Triggers cloud-side agent. For local compile, use `cowiki write`." |
| `cowiki write` | Rephrase: "Create or edit a wiki page. Primary tool for local agent output. Use --path to target entities/, concepts/, or wiki/. For large external sources, prefer `cowiki ingest` → `cowiki compile`." |

### File Changes

| File | Action |
|------|--------|
| `cli/skills/cowiki-cli/SKILL.md` | Add Content Workflow section after Step 5 |
| `cli/skills/cowiki-cli/commands.md` | Update write/ingest/compile descriptions |

## Part C: Multi-Directory Wiki + WikiFs Abstraction

### Current State

All page routes in `server/src/routes/pages.rs` hardcode `wiki/` prefix. `compile.rs` hardcodes `wiki/` in page paths. `core/src/git.rs` inits only `wiki/` and `sources/` directories. Adding a new content directory requires copy-pasting or rewriting every route.

### Design

#### C1. Git layer: WikiRepo (unchanged)

`WikiRepo` in `core/src/git.rs` remains as-is — it operates on arbitrary file paths. No knowledge of directory semantics.

#### C2. New abstraction: WikiFs

New file `crates/core/src/wiki_fs.rs` wraps `WikiRepo` with directory-aware operations:

```rust
/// Known content directories (whitelist, not enum — easy to extend)
const CONTENT_DIRS: &[&str] = &["wiki", "entities", "concepts"];

pub fn validate_dir(dir: &str) -> Result<&str> {
    // Strip trailing slash, check against whitelist
    let dir = dir.trim_end_matches('/');
    if CONTENT_DIRS.contains(&dir) {
        Ok(dir)
    } else {
        Err(format!("unknown content dir: {dir}. valid: {CONTENT_DIRS:?}"))
    }
}

pub fn all_dirs() -> &'static [&'static str] {
    CONTENT_DIRS
}

/// Write a page under a content directory
pub fn write_page(
    repo: &WikiRepo,
    branch: &str,
    dir: &str,        // "wiki", "entities", "concepts"
    slug: &str,
    content: &[u8],
    author: &str,
) -> Result<()> {
    let dir = validate_dir(dir)?;
    let path = format!("{dir}/{slug}.md");
    repo.write_file(branch, &path, content, &format!("edit: {slug}"), author)
}

/// Read a page from a content directory
pub fn read_page(repo: &WikiRepo, branch: &str, dir: &str, slug: &str) -> Result<Option<Vec<u8>>> {
    let dir = validate_dir(dir)?;
    repo.read_file(branch, &format!("{dir}/{slug}.md"))
}

/// List pages in a content directory
pub fn list_pages(repo: &WikiRepo, branch: &str, dir: &str) -> Result<Vec<String>> {
    let dir = validate_dir(dir)?;
    repo.list_files(branch, dir)
}

/// List pages recursively (for tree view)
pub fn list_pages_recursive(repo: &WikiRepo, branch: &str, dir: &str) -> Result<Vec<String>> {
    let dir = validate_dir(dir)?;
    repo.list_files_recursive(branch, dir)
}

/// List across all content directories
pub fn list_all_dirs(repo: &WikiRepo, branch: &str) -> Result<Vec<String>> {
    let mut all = Vec::new();
    for dir in CONTENT_DIRS {
        if let Ok(files) = repo.list_files_recursive(branch, dir) {
            all.extend(files);
        }
    }
    Ok(all)
}
```

**Design principle:** String + whitelist, not enum. Adding a 4th directory = adding one string to `CONTENT_DIRS`. No enum variant to add, no match arms to update.

#### C3. Repo init

In `core/src/git.rs` `open_or_init()`, add:

```rust
fs::create_dir_all(path.join("entities")).ok();
fs::create_dir_all(path.join("concepts")).ok();
fs::write(path.join("entities/.gitkeep"), "").ok();
fs::write(path.join("concepts/.gitkeep"), "").ok();
// git add + commit for the new dirs
```

For existing repos, directories are created lazily on first write (via `write_file` which calls `fs::create_dir_all` already).

#### C4. Server routes

**`WritePage` request — add `path` field:**

```rust
#[derive(Deserialize)]
pub struct WritePage {
    pub slug: String,
    pub body: String,
    pub branch: String,
    pub path: Option<String>,  // default: "wiki"
}
```

`write_page_ws` becomes:

```rust
pub async fn write_page_ws(...) -> Result<...> {
    let dir = input.path.as_deref().unwrap_or("wiki");
    let dir = cowiki_core::wiki_fs::validate_dir(dir)?;
    cowiki_core::wiki_fs::write_page(&repo, &branch, dir, &input.slug, input.body.as_bytes(), &branch)?;
    Ok(Json(json!({"ok": true, "slug": input.slug, "path": format!("{dir}/{}", input.slug)})))
}
```

**`PageQueryParams` — add `dir` query param:**

```rust
#[derive(Deserialize)]
pub struct PageQueryParams {
    pub branch: Option<String>,
    pub dir: Option<String>,  // "wiki" (default), "entities", "concepts", "all"
}
```

`list_pages_ws` becomes:

```rust
pub async fn list_pages_ws(
    Query(params): Query<PageQueryParams>,
) -> Result<...> {
    let dir = params.dir.as_deref().unwrap_or("wiki");
    let files = if dir == "all" {
        cowiki_core::wiki_fs::list_all_dirs(&repo, &branch)?
    } else {
        cowiki_core::wiki_fs::list_pages_recursive(&repo, &branch, dir)?
    };
    // ... rest of tree building logic (unchanged)
}
```

**`get_page_ws` — add `dir` query param:**

```rust
// Rename ListParams → PageQueryParams (used by both list and get)
#[derive(Deserialize)]
pub struct PageQueryParams {
    pub branch: Option<String>,
    pub dir: Option<String>,
}

pub async fn get_page_ws(
    Path((ws_slug, raw_slug)): Path<(String, String)>,
    Query(params): Query<PageQueryParams>,
) -> ... {
    let dir = params.dir.as_deref().unwrap_or("wiki");
    cowiki_core::wiki_fs::read_page(&repo, &branch, dir, &slug)?
}
```

`list_pages_ws` also uses `PageQueryParams` instead of `ListParams`.

#### C5. Bulk operations: compile, diff, submit, review

These remain scoped to `wiki/` for now — cloud compile is Part B Path 1 territory. The `WikiFs` layer exists so they can be extended later, but that's out of scope for this spec.

#### C6. CLI changes

**`cowiki write` — add `--path`:**

```bash
cowiki write -w <ws> my-page --path entities --body "..."
cowiki write -w <ws> my-page --path concepts --body "..."
cowiki write -w <ws> my-page --body "..."              # defaults to wiki
```

`WritePageRequest` type adds `path?: string`. `client.ts` `writePage` passes it through. `commands/write.ts` adds `.option('--path <path>', 'Target directory: wiki, entities, concepts')`.

**`cowiki list` — add `--dir`:**

```bash
cowiki list -w <ws> --dir entities
cowiki list -w <ws> --dir all
```

`commands/list.ts` adds `.option('--dir <dir>', 'Directory: wiki, entities, concepts, all')`.

**`cowiki read` — add `--dir`:**

```bash
cowiki read -w <ws> my-entity --dir entities
```

### File Changes

| Crate | File | Change |
|-------|------|--------|
| `core` | `src/wiki_fs.rs` | **New** — content directory validation + operations |
| `core` | `src/lib.rs` | Add `pub mod wiki_fs` |
| `core` | `src/git.rs` | Init `entities/`, `concepts/` dirs on repo creation |
| `server` | `routes/pages.rs` | `WritePage` + `path`, `ListParams` + `dir`, `GetPageParams` + `dir`, use `wiki_fs` functions |
| `cli` | `commands/write.ts` | Add `--path` option |
| `cli` | `commands/list.ts` | Add `--dir` option |
| `cli` | `commands/read.ts` | Add `--dir` option |
| `cli` | `types.ts` | Add `path?: string` to `WritePageRequest` |
| `cli` | `client.ts` | No changes needed (JSON passthrough) |
| `cli` | `skills/cowiki-cli/SKILL.md` | Add Content Workflow section |
| `cli` | `skills/cowiki-cli/commands.md` | Update descriptions, add --path/--dir |

## Data Flow: Local Agent Compile

```
┌──────────────────────────────────────────────────┐
│ Local Agent                                      │
│                                                   │
│  1. cowiki list -w mywiki --dir all              │
│     → sees: wiki/page-a, wiki/page-b,            │
│             entities/project-x, concepts/pattern-y │
│                                                   │
│  2. cowiki read -w mywiki page-a                 │
│     → reads body                                  │
│                                                   │
│  3. Agent analyzes:                              │
│     - Extract entity "Project X"                  │
│     - Extract concept "Pattern Y"                │
│     - Cross-ref: [[page-a]], [[page-b]]          │
│                                                   │
│  4. cowiki write -w mywiki project-x \           │
│       --path entities --body "..."               │
│     cowiki write -w mywiki pattern-y \           │
│       --path concepts --body "..."               │
│                                                   │
│  Result: entities/project-x.md created,           │
│          concepts/pattern-y.md created            │
│          Both link to existing wiki pages         │
└──────────────────────┬───────────────────────────┘
                       │ POST /api/workspaces/mywiki/pages
                       │ { slug: "project-x", path: "entities", body: "..." }
                       ▼
┌──────────────────────────────────────────────────┐
│ Server                                           │
│  wiki_fs::validate_dir("entities") → Ok          │
│  wiki_fs::write_page(repo, branch, "entities",   │
│    "project-x", body, author)                    │
│    → git: entities/project-x.md                  │
└──────────────────────────────────────────────────┘
```

## Backward Compatibility

- `WritePage.path` omitted → defaults to `"wiki"` → existing clients unchanged
- `ListParams.dir` omitted → defaults to `"wiki"` → existing `cowiki list` unchanged
- `get_page_ws` `dir` omitted → defaults to `"wiki"` → existing reads unchanged
- Old repos without `entities/` or `concepts/` dirs: created lazily on first write (via `write_file`'s existing `fs::create_dir_all`)
- npm package `cli/skills/cowiki-cli/` — files updated, users re-run `npm install -g @cowiki/cli` and re-copy skill files

## Testing

| Layer | What | Tool |
|-------|------|------|
| Unit | `wiki_fs::validate_dir` — valid/invalid dirs | `cargo test` |
| Integration | Write to `entities/`, read from `entities/`, list `--dir entities` | `cargo test` (against temp repo) |
| Integration | Write to `concepts/`, read from `concepts/` | same |
| Integration | `--dir all` returns pages from all directories | same |
| CLI Unit | `write --path entities` generates correct request body | `vitest` |
| CLI Unit | `list --dir entities` generates correct query params | `vitest` |
| Skill Flow | Agent reads SKILL.md, understands dual-path workflow | manual (Layer 1) |

## Not in Scope

- Cloud compile output to `entities/` or `concepts/` — stays in `wiki/` for now
- Bulk operations (submit/review/diff) across non-wiki directories — wiki/ only
- Web UI changes for directory browsing — separate spec
- Database schema changes — `pages` table stores slug + title + summary + embedding; directory context is purely git-side
