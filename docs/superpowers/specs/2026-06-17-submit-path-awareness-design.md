# Submit Path Awareness — Design Spec

> **Historical document — superseded.** This submit flow belonged to the
> retired CLI/backend review model. Local review now uses Draft and Agent
> Changes; see the [`cowiki-space` skill](../../../skills/cowiki-space/SKILL.md).

**Date:** 2026-06-17
**PR:** [#92](https://github.com/wfnuser/cowiki/pull/92)
**Status:** approved

## Problem

The submit flow uses bare page slugs (`people/alice`) throughout the stack, with `wiki/` hardcoded at three call sites. Now that the sidebar uses a 4-section directory tree (`wiki/`, `entities/`, `concepts/`), a bare slug is ambiguous — the server cannot know which content directory it belongs to.

Three hardcoded `wiki/{slug}.md` sites:

| Site | File |
|------|------|
| diff against main | `crates/core/src/git.rs:639` |
| embedding + dedup loop | `crates/server/src/routes/submit.rs:74` |
| CLI flatten | `cli/src/commands/submit.ts:32` |

## Design

Replace bare slugs with **repo paths** in the submit contract. A repo path is a directory-prefixed slug without `.md` extension: `entities/people/alice`, `wiki/team-home`, `concepts/patterns/error-handling`.

### Principle

`slug` = short display identifier (unchanged). `path` = full repo location (new). Two fields, two responsibilities.

---

## 1. Data Model

### 1.1 `PageListItem` (server → JSON)

Add `path` field alongside existing `slug`:

```rust
pub struct PageListItem {
    pub slug: String,       // "people/alice" — unchanged, for display/routing
    pub path: String,       // NEW — "entities/people/alice"
    pub title: String,
    pub summary: String,
    pub branch: String,
    pub kind: String,       // "page" | "folder"
    pub children: Vec<PageListItem>,
}
```

### 1.2 `SubmitRequest`

```rust
pub struct SubmitRequest {
    pub branch: String,
    pub paths: Vec<String>,  // was: page_slugs
    #[serde(default)]
    pub skip_review: bool,
}
```

```typescript
// cli/src/types.ts
interface SubmitRequest {
  branch: string;
  paths: string[];  // was: page_slugs
}
```

### 1.3 `PageMeta` (CLI)

```typescript
interface PageMeta {
  slug: string;
  path: string;       // NEW
  title: string;
  summary: string;
  branch: string;
  kind?: string;      // "page" | "folder"
  children?: PageMeta[];
}
```

### 1.4 `submissions` table

Column `page_slugs` → `paths`. Stores JSON string array of repo paths.

### 1.5 Frontend `Submission` type

```typescript
interface Submission {
  // page_slugs: string[];  // removed
  paths: string[];          // added
}
```

---

## 2. Server Changes

### 2.1 `list_pages_from_dir` — construct `path`

```rust
// Leaf pages
result.push(PageListItem {
    slug: item.slug.clone(),
    path: format!("{}/{}", dir, item.slug),  // "entities/people/alice"
    ...
});

// Folder nodes
result.push(PageListItem {
    slug: format!("{subdir}/_index"),
    path: format!("{dir}/{subdir}/_index"),   // "entities/people/_index"
    ...
});
```

### 2.2 `list_pages_all_dirs`

No special treatment needed — delegates to `list_pages_from_dir` which now builds correct paths per directory. Top-level `dir_node` path = `format!("{dir}/_index")`.

### 2.3 `diff_files` — accept paths

```rust
// Before
pub fn diff_files(&self, branch: &str, slugs: &[String]) -> ... {
    for slug in slugs {
        let path = format!("wiki/{slug}.md");

// After
pub fn diff_files(&self, branch: &str, paths: &[String]) -> ... {
    for p in paths {
        let file_path = format!("{p}.md");
```

### 2.4 Submit embedding loop

```rust
// Before
for slug in &input.page_slugs {
    let path = format!("wiki/{slug}.md");

// After
for p in &input.paths {
    let file_path = format!("{p}.md");
    // Skip synthetic _index folders
    if p.ends_with("/_index") { continue; }
```

### 2.5 `submissions` DB

- Rename column: `page_slugs` → `paths`
- Migration: SQLite `ALTER TABLE RENAME COLUMN` (3.25.0+)
- Update `create` function signature: `paths: &[String]` instead of `page_slugs: &[String]`

### 2.6 MCP server

```rust
pub struct SubmitParams { pub paths: Vec<String>, }  // was: page_slugs
```

---

## 3. CLI Changes

### 3.1 `submit.ts` — flatten paths, not slugs

```typescript
function flattenPaths(pages: PageMeta[]): string[] {
  const paths: string[] = [];
  for (const p of pages) {
    if (p.kind === 'folder' && p.children && p.children.length > 0) {
      paths.push(...flattenPaths(p.children));
    } else if (p.kind !== 'folder') {
      paths.push(p.path);
    }
  }
  return paths;
}
```

### 3.2 `--dir` parameter

```bash
cowiki submit -w myws people/alice                    # → wiki/people/alice
cowiki submit -w myws people/alice --dir entities     # → entities/people/alice
cowiki submit -w myws entities/people/alice           # CLI detects known dir prefix, uses as-is
cowiki submit -w myws --all --dir entities --yes      # submit all in entities/
cowiki submit -w myws --all --dir all --yes           # submit all across all dirs
```

Default `--dir` = `wiki`. When the slug already starts with a known content directory (`wiki/`, `entities/`, `concepts/`), it is passed through as-is without prepending.

### 3.3 SKILL docs

Update `cli/skills/cowiki-cli/commands.md` with `--dir` examples.

---

## 4. Frontend Changes

### 4.1 `ActiveView` — store `path`

`ActiveView` currently tracks `{ kind, slug, content }`. Add `path` so page actions can derive the content directory from the node itself:

```typescript
type ActiveView = {
  kind: 'page' | ...;
  slug: string;
  path?: string;   // NEW — "entities/people/alice"
  content: PageFull | null;
} | null;
```

### 4.2 Remove `pageDirMap` / `getPageDir`

Currently `pageDirMap` (`Record<string, string>`) maps slug → dir, built by walking the page tree. `getPageDir(slug)` falls back to `'wiki'` when no entry exists.

**Problem:** `pageDirMap` is keyed by slug, which is ambiguous across directories. A draft-only page in `entities/` won't be in the map if it was built from main pages only — it falls back to `wiki/`, causing read/save/delete to target the wrong directory.

**Fix:** With `PageListItem.path` now available, derive the directory directly from the selected node's `path` (first segment before `/`). Remove `pageDirMap` state, `buildPageDirMap()`, and `getPageDir()`.

Three call sites change:

| Site | Before | After |
|------|--------|-------|
| `selectPage` (L307) | `const dir = getPageDir(slug)` | `const dir = path.split('/')[0]` |
| path ops callback (L586) | `const dir = getPageDir(activeView.slug)` | `const dir = activeView.path?.split('/')[0] \|\| 'wiki'` |
| `handleSavePage` (L604) | `const dir = getPageDir(slug)` | `const dir = activeView.path?.split('/')[0] \|\| 'wiki'` |

`selectPage` signature changes from `(ws, slug)` to `(ws, slug, path)` — the sidebar passes the node's `path` when selecting.

### 4.3 `api.ts`

```typescript
export async function submit(branch: string, paths: string[], ...) {
  body: JSON.stringify({ branch, paths, ... }),
```

### 4.4 `ReviewDetail.tsx` / `ReviewList.tsx`

Replace `submission.page_slugs` with `submission.paths`. Strip leading content-dir prefix for display (e.g., `entities/people/alice` → show as `entities/people/alice`).

### 4.5 `MainLayout.tsx` — tree merge key

Switch merge key from `slug` to `path`:

```typescript
const mainByPath = new Map(main.map((p) => [p.path, p]));
// ...
const existing = mainByPath.get(dp.path);
```

`mergePageTrees` uses `path` for identity comparison throughout. Sorting still uses `title`.

---

## 5. Files Affected

| File | Change |
|------|--------|
| `crates/server/src/routes/submit.rs` | `SubmitRequest.paths`, loop uses paths |
| `crates/server/src/routes/pages.rs` | `PageListItem.path`, construction in build_level |
| `crates/core/src/git.rs` | `diff_files` signature: `slugs` → `paths` |
| `crates/db/src/submissions.rs` | column rename + function signatures |
| `cowiki-mcp-server/src/server.rs` | `SubmitParams.paths` |
| `cli/src/types.ts` | `SubmitRequest.paths`, `PageMeta.path` |
| `cli/src/commands/submit.ts` | `flattenPaths`, `--dir` flag |
| `cli/src/client.ts` | `SubmitRequest` type import |
| `cli/skills/cowiki-cli/commands.md` | `--dir` docs |
| `web/src/api.ts` | `submit()` signature |
| `web/src/pages/MainLayout.tsx` | merge key → `path`, remove `pageDirMap`/`getPageDir`, `ActiveView` +`path`, `selectPage` +`path` param |
| `web/src/components/review/ReviewDetail.tsx` | display `paths` |
| `web/src/components/review/ReviewList.tsx` | display `paths` |

---

## 6. Non-Goals

- Backward compatibility for `page_slugs`. Per wfnuser: "no legacy `page_slugs` compatibility needed."
- Changing URL routing or slug semantics. `slug` remains the display/routing identifier.
- Changing `diff_ref_against_main` — it operates on refs, not individual slugs, so it is unaffected.
