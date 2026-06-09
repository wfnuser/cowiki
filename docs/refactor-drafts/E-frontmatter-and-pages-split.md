# Draft — Epic E: Frontmatter model + split `pages.rs`

> Status: **draft for review** (codex). Natural follow-up of #53. **Must add tests** (title fallback / canonical title) or it regresses. Has a real product decision (title fallback).

## Problem
- `crates/server/src/routes/pages.rs` mixes HTTP handlers, frontmatter parsing, title validation, and wiki-tree building.
- Frontmatter is **hand-assembled** in two places with `format!("---\ntitle: \"{}\"...")` (e.g. `compile.rs:112`) and on the frontend, so a missing/empty title can produce a page with no title. Review also found `compiler.rs parse_compiled_page` strips a symbolic-only title to `""` → slug `wiki/.md`, so all such pages collide/overwrite.
- `list_pages_from_repo` reads each file in a loop and `build_level` re-scans items per directory per level (O(N·depth)).

## Target shape
```
crates/server/src/routes/
  pages.rs        // HTTP layer only (handlers)
crates/core/src/  (or a wiki module)
  frontmatter.rs  // parse + build (one source of truth)
  wiki_tree.rs    // build the page tree from a flat file list
```

### `frontmatter.rs` — parse + build together (kills "no-title page")
```rust
pub struct Frontmatter { pub title: String, pub summary: String, pub kind: String, pub sources: Vec<String> }

pub fn parse(markdown: &str) -> (Frontmatter, String /* body */);

/// Single canonical builder used by compile + any write path.
pub fn build_page_markdown(fm: &Frontmatter, body: &str) -> String;

/// Canonical slug from a title, with a deterministic fallback when empty.
pub fn slug_for_title(title: &str) -> String;
```
Frontend gets the mirror: `web/src/lib/markdown.ts` `makePageMarkdown({ title, summary, kind })`, so both sides build frontmatter identically.

## Open decisions for review (the important ones)
1. **Title fallback.** When a compiled/written page has an empty or symbol-only title:
   - **(a, proposed)** fall back to a deterministic slug `page-<hash8(content)>` and set the title to e.g. `"Untitled (page-ab12cd34)"`;
   - (b) reject the page (compile returns a structured per-page error — pairs with #15/Epic A2);
   - (c) derive a title from the first heading/sentence of the body.
   Proposal: **(a)** for safety (never collide/overwrite) + surface a warning; revisit with (c) later.
2. **Where do the modules live** — `crates/core` (reusable by server + future CLI) or a server-local `wiki/` module? Proposal: `crates/core` (it's domain logic, and #18 wants core/service/storage layering).
3. **Canonical title rules** — is a CJK/emoji title valid (review said yes)? Confirm the normalization (don't strip non-ASCII; only fall back when the result is truly empty).

## Tests to add (required)
- `parse` round-trips `build_page_markdown`.
- `slug_for_title`: normal, CJK, symbol-only → fallback, empty → fallback (no two distinct inputs collide).
- `wiki_tree` builds the expected nesting from a flat list.

## Notes
- Touches `compile.rs` (conflicts with #64/#66) and `compiler.rs`.
