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

`cli/guide.md` (122 lines) duplicates the full setup flow (env check → CLI install → API key → verify → skill install) that already lives in `cli/skills/cowiki-cli/SKILL.md`. Two files say the same thing.

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
4. cowiki write -w <ws> <slug> --path entities --body "..."
   cowiki write -w <ws> <slug> --path concepts --body "..."
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
4. Write: cowiki write -w <ws> <slug> --path entities --body "..."
   (or --path concepts, --path wiki)

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

New file `crates/core/src/wiki_fs.rs` wraps `WikiRepo` with directory-aware operations. All functions operate on a **full relative path** (e.g., `wiki/food/apple`), where the first path segment must be one of the known top-level content directories (`wiki`, `entities`, `concepts`). Nesting under the top-level dir is arbitrary and transparent.

```rust
/// Known top-level content directories (whitelist — easy to extend)
const CONTENT_DIRS: &[&str] = &["wiki", "entities", "concepts"];

/// Validate a content path. Paths are absolute from content root —
/// must start with a known top-level directory, no `.` or `..` components.
///
/// Accepts: "wiki", "wiki/food", "wiki/food/apple".
/// Rejects: "unknown", "wiki/../../etc", "./wiki/foo", "wiki/./bar".
fn validate_path(path: &str) -> Result<String> {
    let path = path.trim_end_matches('/').trim_start_matches('/');

    // Reject `.` and `..` segments anywhere in the path
    for seg in path.split('/') {
        if seg == "." || seg == ".." {
            return Err(format!("invalid path component: {seg}"));
        }
    }

    if path.is_empty() {
        return Err("empty path".into());
    }

    // First segment must be a known content directory
    let top = path.split('/').next().unwrap_or(&path);
    if CONTENT_DIRS.contains(&top) {
        Ok(path.to_string())
    } else {
        Err(format!("unknown content dir: {top}. valid: {CONTENT_DIRS:?}"))
    }
}

pub fn all_dirs() -> &'static [&'static str] {
    CONTENT_DIRS
}

/// Write a page at a relative path (e.g. "wiki/food/apple" → "wiki/food/apple.md")
pub fn write_page(
    repo: &WikiRepo,
    branch: &str,
    rel_path: &str,    // "wiki/food/apple"
    content: &[u8],
    author: &str,
) -> Result<()> {
    let path = validate_path(rel_path)?;
    let file_path = format!("{path}.md");
    let slug = rel_path.rsplit('/').next().unwrap_or(rel_path);
    repo.write_file(branch, &file_path, content, &format!("edit: {slug}"), author)
}

/// Read a page from a relative path (e.g. "wiki/food/apple" → reads "wiki/food/apple.md")
pub fn read_page(repo: &WikiRepo, branch: &str, rel_path: &str) -> Result<Option<Vec<u8>>> {
    let path = validate_path(rel_path)?;
    repo.read_file(branch, &format!("{path}.md"))
}

/// List pages under a directory prefix (e.g. "wiki/food" → pages under wiki/food/)
pub fn list_pages(repo: &WikiRepo, branch: &str, dir: &str) -> Result<Vec<String>> {
    let dir = validate_path(dir)?;
    repo.list_files(branch, dir)
}

/// List pages recursively under a directory prefix
pub fn list_pages_recursive(repo: &WikiRepo, branch: &str, dir: &str) -> Result<Vec<String>> {
    let dir = validate_path(dir)?;
    repo.list_files_recursive(branch, dir)
}

/// List across all top-level content directories
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

**Design principle:** String + whitelist, not enum. Adding a 4th top-level directory = adding one string to `CONTENT_DIRS`. Path validation only checks the first segment; nesting underneath is free-form (e.g. `wiki/a/b/c/d/page`).

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

**`WritePage` request — `slug` → `path`:**

`slug` was a bare filename. Now `path` is the full relative path without `.md` (e.g. `"wiki/food/apple"`). For backward compat, `slug` is still accepted and defaults path to `"wiki/{slug}"`.

```rust
#[derive(Deserialize)]
pub struct WritePage {
    pub slug: Option<String>,    // backward compat — resolves to "wiki/{slug}"
    pub path: Option<String>,    // "wiki/food/apple", "entities/projects/x", etc.
    pub body: String,
    pub branch: String,
}
```

`write_page_ws` becomes:

```rust
pub async fn write_page_ws(...) -> Result<...> {
    let rel_path = match (input.path.as_deref(), input.slug.as_deref()) {
        (Some(p), _) => p.to_string(),
        (None, Some(s)) => format!("wiki/{s}"),
        (None, None) => return Err(AppError::BadRequest("path or slug required".into())),
    };
    cowiki_core::wiki_fs::write_page(&repo, &branch, &rel_path, input.body.as_bytes(), &branch)?;
    Ok(Json(json!({"ok": true, "path": format!("{rel_path}.md")})))
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
        // Build a merged tree with dir names as top-level folder nodes
        let mut tree = Vec::new();
        for d in cowiki_core::wiki_fs::all_dirs() {
            if let Ok(dir_files) = cowiki_core::wiki_fs::list_pages_recursive(&repo, &branch, d) {
                let dir_node = PageListItem {
                    slug: format!("{d}/_index"),
                    title: d.to_string(),
                    summary: String::new(),
                    branch: branch.clone(),
                    kind: "folder".into(),
                    children: list_pages_from_dir(&repo, &branch, d, &dir_files),
                };
                tree.push(dir_node);
            }
        }
        return Ok(Json(tree));
    } else {
        let files = cowiki_core::wiki_fs::list_pages_recursive(&repo, &branch, dir)?;
        return list_pages_from_dir(&repo, &branch, dir, &files);
    };
}
```

`list_pages_from_repo` 被重命名并参数化为 `list_pages_from_dir(repo, branch, dir, files)` — `dir` 替换了硬编码的 `"wiki"`，用于文件列表范围界定（第 209 行）和前缀剥离（第 224 行 `strip_prefix("wiki/")` → `strip_prefix(&format!("{dir}/"))`）。

**`--dir all` 输出形状：** 合并树。每个内容目录成为一个顶级文件夹节点（`kind: "folder"`），其子节点是该目录内的标准树。这与现有的 `list_pages_ws` 输出模式一致 —— 调用者无论 `dir` 值为何都能获得一棵树。

**`get_page_ws` — support nested paths via `{*slug}`:**

The catch-all `{*slug}` captures the full path (e.g. `wiki/food/apple`). When the slug contains `/`, it's treated as a full relative path. When it's a bare name, `dir` param provides the context (defaults to `"wiki"`).

```rust
#[derive(Deserialize)]
pub struct PageQueryParams {
    pub branch: Option<String>,
    pub dir: Option<String>,     // default: "wiki"
}

pub async fn get_page_ws(
    Path((ws_slug, raw_slug)): Path<(String, String)>,
    Query(params): Query<PageQueryParams>,
) -> ... {
    let rel_path = if raw_slug.contains('/') {
        raw_slug.trim_start_matches('/').to_string()   // "wiki/food/apple"
    } else {
        let dir = params.dir.as_deref().unwrap_or("wiki");
        format!("{dir}/{raw_slug}")                    // "wiki/my-page"
    };
    cowiki_core::wiki_fs::read_page(&repo, &branch, &rel_path)?
}
```

`list_pages_ws` also uses `PageQueryParams` instead of `ListParams`.

#### C5. Bulk operations: compile, diff, submit, review

These remain scoped to `wiki/` for now — cloud compile is Part B Path 1 territory. Each uses `WikiRepo` directly (bypassing `WikiFs`) with hardcoded `wiki/` paths. The `WikiFs` layer exists so they can be extended to other directories later, but that's out of scope for this spec.

#### C6. CLI changes

**`cowiki write` — add `--path`:**

```bash
# --path: directory prefix (combined with slug to form full path)
cowiki write -w <ws> my-page --body "..."                       # → wiki/my-page.md (default)
cowiki write -w <ws> my-page --path entities --body "..."       # → entities/my-page.md
cowiki write -w <ws> wiki/food/apple --body "..."               # → wiki/food/apple.md (slug as full path)
cowiki write -w <ws> entities/projects/x --body "..."           # → entities/projects/x.md
cowiki write -w <ws> concepts/patterns/y --body "..."           # → concepts/patterns/y.md
```

When `--path` is given, the full path is `"{path}/{slug}"`. When omitted, slug is used directly: if slug contains `/` it's the full path; otherwise defaults to `"wiki/{slug}"`. All paths must be absolute from content root — `..` and `.` are rejected by `validate_path`.

`commands/write.ts` logic:
```typescript
const relPath = opts.path
  ? `${opts.path}/${slug}`
  : (slug.includes('/') ? slug : `wiki/${slug}`);
```

`WritePageRequest` adds `path?: string`. `WriteResponse` returns `path` instead of `slug`.

**`cowiki list` — add `--dir`:**

```bash
cowiki list -w <ws>                           # defaults to --dir wiki
cowiki list -w <ws> --dir entities            # list entities/ recursively
cowiki list -w <ws> --dir wiki/food           # list wiki/food/ subtree
cowiki list -w <ws> --dir all                 # merged tree across all content dirs
```

`commands/list.ts` adds `.option('--dir <dir>', 'Directory prefix: wiki, entities, concepts, wiki/food, all')`.

**`cowiki read` — add `--dir`:**

```bash
cowiki read -w <ws> my-entity --dir entities              # → reads entities/my-entity.md
cowiki read -w <ws> wiki/food/apple                        # → reads wiki/food/apple.md (slug contains /)
```

### File Changes

| Crate | File | Change |
|-------|------|--------|
| `core` | `src/wiki_fs.rs` | **New** — `validate_path`, `write_page`, `read_page`, `list_pages`, `list_pages_recursive`, `list_all_dirs` |
| `core` | `src/lib.rs` | Add `pub mod wiki_fs` |
| `core` | `src/git.rs` | Init `entities/`, `concepts/` dirs on repo creation |
| `server` | `routes/pages.rs` | `WritePage` — `slug`→`path` (full relative path); `PageQueryParams` + `dir`; `get_page_ws` supports `{*slug}` with `/` for nested paths; `list_pages_from_repo` → `list_pages_from_dir`; `--dir all` merged-tree |
| `cli` | `commands/write.ts` | Add `--path` (supports nested: `wiki/food/apple`) |
| `cli` | `commands/list.ts` | Add `--dir` (supports prefix: `wiki/food`) |
| `cli` | `commands/read.ts` | Add `--dir` (backward compat); slug with `/` auto-resolves |
| `cli` | `types.ts` | `WritePageRequest`: `slug?` → optional, add `path?`; `WriteResponse`: `slug` → `path` |
| `cli` | `client.ts` | Add `dir?: string` param to `listPages()` and `getPage()` |
| `cli` | `skills/cowiki-cli/SKILL.md` | Add Content Workflow section with nested path examples |
| `cli` | `skills/cowiki-cli/commands.md` | Update descriptions, add --path/--dir with nested examples |

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
                       │ { path: "entities/project-x", body: "...", branch: "..." }
                       ▼
┌──────────────────────────────────────────────────┐
│ Server                                           │
│  wiki_fs::validate_path("entities/project-x") → Ok     │
│  wiki_fs::write_page(repo, branch,                      │
│    "entities/project-x", body, author)                  │
│    → git: entities/project-x.md                         │
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
| Unit | `wiki_fs::validate_path` — valid/invalid dirs and path traversal | `cargo test` |
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
