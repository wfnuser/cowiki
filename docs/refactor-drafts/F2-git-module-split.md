# Draft — Epic F2: Split `git.rs` (after F1 `spawn_blocking`)

> Status: **draft for review** (codex). Pairs with the #56 deadlock fix (#62) and F1 (`spawn_blocking`). Mostly mechanical, but includes one real correctness fix (the shared-index race).

## Problem
`crates/core/src/git.rs` (~470 lines) is one file doing: repo manager, per-branch locks, read, write, diff, merge. Review also found a **correctness bug**: `write_file` for nested paths (`wiki/foo.md`, the common case) opens `repo.index()` — a single shared on-disk index — and `read_tree`+`add_frombuffer`+`write_tree`. `branch_lock` only serializes per branch, so concurrent writes to `main` vs `user/<id>` race on the one index and can leave it inconsistent with HEAD.

## Target shape
```
crates/core/src/git/
  mod.rs          // re-exports; WikiRepo struct + branch_lock
  manager.rs      // WikiRepoManager
  read.rs         // read_file, list_files, list_files_recursive, walk_tree
  write.rs        // write_file / write_file_locked / merge_to_main / ensure_branch
  diff.rs         // diff_files, compute_line_diff, FileDiff/DiffHunk/DiffLine
```
Public API unchanged (just moved); `WikiRepo` methods split across `impl` blocks in the submodules.

### Correctness fix folded in: nested-path write without the shared index
Replace the `repo.index()` path in `write_file_locked` with an in-memory recursive `TreeBuilder` (or a detached `git2::Index::new()`), so two branches never contend on the one on-disk index:
```rust
// build wiki/foo/bar.md purely in memory from parent_commit.tree()
fn insert_blob_into_tree(repo, base_tree, path_parts, blob_oid) -> Result<Oid> { /* recurse with TreeBuilder */ }
```

## Sequencing
1. **F1 first (separate PR):** wrap the synchronous `git2`/`fs`/lock ops in `spawn_blocking` so they don't block tokio workers. Behavior-preserving — do it in isolation so any regression is easy to bisect. (Coordinate with #62, which already touched the lock in `merge_to_main`.)
2. **F2 (this draft):** split modules + the in-memory nested-tree write fix. Add a concurrency test: parallel writes to `main` and `user/x` of nested paths, assert both commits are consistent with HEAD.

## Open decisions for review
1. **`spawn_blocking` boundary.** Wrap at the `WikiRepo` method level (each public method spawns), or have callers spawn? Proposal: method-level (callers stay simple); `WikiRepo` methods become `async` or get `_blocking` twins.
2. **In-memory tree vs detached index** for nested writes — `TreeBuilder` recursion (no temp files, more code) vs `git2::Index::new()` (simpler, in-memory). Proposal: detached `Index::new()` unless it forces a working-dir checkout.
3. Relates to #16 (git storage strategy) and #18 (core/service/storage layering) — keep `git/` as the storage layer boundary.
