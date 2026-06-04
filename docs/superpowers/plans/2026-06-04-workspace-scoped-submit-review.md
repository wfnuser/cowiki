# Workspace-Scoped Submit/Review + Legacy Global Repo Removal — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the `submit` and `review` flows a workspace dimension so every git operation goes through the per-workspace repo, then remove the legacy global "default repo" (`data/repo`) and all its routes/handlers/fields entirely.

**Architecture:** cowiki currently runs two parallel generations of the wiki-storage design. The new one (`WikiRepoManager`) keeps one git repo per workspace under `data/{workspace_slug}/repo`. The old one keeps a single global repo at `data/repo`, exposed via `AppState.wiki_repo`. The `ingest`/`compile`/`pages` flows already have workspace-scoped twins; only `submit` and `review` still operate exclusively on the global repo, because the `submissions` table has no workspace column. This plan adds `submissions.workspace_slug`, rewrites `submit`/`review` to resolve the repo via `repo_manager.get(ws_slug)`, migrates the (thin) frontend callers, then deletes every trace of the global repo.

**Tech Stack:** Rust (axum, sqlx/PostgreSQL, git2/libgit2), React + TypeScript (Vite), Docker Compose (Postgres pgvector).

---

## Pre-flight: establish a clean baseline

The worktree is already created at `.worktrees/workspace-repo-migration` on branch `refactor/workspace-scoped-repos` (off `origin/main`).

- [ ] **Step 0.1: Start a Postgres for tests**

Run from repo root:
```bash
docker compose up -d postgres
```
Expected: `postgres` container running, port 5432 mapped.

- [ ] **Step 0.2: Confirm the workspace compiles before any change**

Run:
```bash
cargo build
```
Expected: builds successfully (warnings OK). If it fails, STOP and report — the baseline is broken, not your change.

- [ ] **Step 0.3: Confirm DB tests run against the test database**

Run:
```bash
TEST_DATABASE_URL=postgres://cowiki:cowiki@localhost:5432/cowiki cargo test -p cowiki-db
```
Expected: existing tests pass (DB-integration tests use `TEST_DATABASE_URL`; if unset they silently skip — so confirm the var is exported).

---

## Task 1: Add `workspace_slug` to the `submissions` table (DB layer, TDD)

This is the schema change that makes everything else possible. It is the one layer with a real automated-test harness, so it gets full TDD.

**Files:**
- Create: `crates/db/src/migrations/008_submission_workspace.sql`
- Modify: `crates/db/src/lib.rs` (register migration 008 in `run_migrations`)
- Modify: `crates/db/src/submissions.rs` (struct field, `create` signature, new `list_pending_for_workspace`)
- Modify (test harness only): `crates/db/src/workspaces.rs` `test_pool()` (register migration 008 so tests see the new column)

### Background facts (verified)
- `run_migrations` in `crates/db/src/lib.rs:14` applies migrations 001–007 by `include_str!` + `sqlx::raw_sql`. Migration 008 must be appended there.
- The test harness `test_pool()` in `crates/db/src/workspaces.rs:437` ALSO hardcodes the migration list (001–007). It must get 008 too, or DB tests run against a schema without the new column.
- `Submission` struct + `create`/`list_pending`/`find_by_id`/`update_status` live in `crates/db/src/submissions.rs`.
- The `pages` table already has a `workspace_slug TEXT NOT NULL DEFAULT ''` column (migration `005_fts.sql:20`) — we mirror that exact shape for consistency.

- [ ] **Step 1.1: Write the migration**

Create `crates/db/src/migrations/008_submission_workspace.sql`:
```sql
-- Scope submissions to a workspace so review can resolve the correct per-workspace git repo.
-- Existing rows referenced the now-removed global repo; they default to '' and become
-- inaccessible through the new per-workspace review listing (acceptable: internal, early-stage).
ALTER TABLE submissions ADD COLUMN IF NOT EXISTS workspace_slug TEXT NOT NULL DEFAULT '';

CREATE INDEX IF NOT EXISTS idx_submissions_workspace_pending
    ON submissions(workspace_slug, status, created_at DESC);
```

- [ ] **Step 1.2: Register migration 008 in the app migration runner**

In `crates/db/src/lib.rs`, after the `sql7` block (ends at line ~28), add before `Ok(())`:
```rust
    let sql8 = include_str!("migrations/008_submission_workspace.sql");
    sqlx::raw_sql(sql8).execute(pool).await.map_err(|e| { tracing::error!("DB error: {e}"); e })?;
```

- [ ] **Step 1.3: Register migration 008 in the test harness**

In `crates/db/src/workspaces.rs`, inside `test_pool()` (after the `007_team_permissions.sql` line, ~line 447), add:
```rust
        let _ = sqlx::raw_sql(include_str!("migrations/008_submission_workspace.sql")).execute(&pool).await;
```

- [ ] **Step 1.4: Write the failing test**

Add to the `#[cfg(test)] mod tests` block in `crates/db/src/submissions.rs` (create the test module if none exists — match the `test_pool`/`TEST_DATABASE_URL` pattern from `workspaces.rs`):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    async fn test_pool() -> Option<PgPool> {
        let url = std::env::var("TEST_DATABASE_URL").ok()?;
        let pool = PgPool::connect(&url).await.ok()?;
        let sql1 = include_str!("migrations/001_init.sql").replace("__EMBEDDING_DIM__", "768");
        let _ = sqlx::raw_sql(&sql1).execute(&pool).await;
        for m in [
            include_str!("migrations/002_workspaces.sql"),
            include_str!("migrations/003_workspace_visibility.sql"),
            include_str!("migrations/004_role_update.sql"),
            include_str!("migrations/005_fts.sql"),
            include_str!("migrations/006_api_keys.sql"),
            include_str!("migrations/007_team_permissions.sql"),
            include_str!("migrations/008_submission_workspace.sql"),
        ] {
            let _ = sqlx::raw_sql(m).execute(&pool).await;
        }
        Some(pool)
    }

    async fn make_user(pool: &PgPool) -> Uuid {
        let id = Uuid::new_v4();
        let _ = sqlx::query("INSERT INTO users (id, name, email) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING")
            .bind(id).bind("Tester").bind(format!("{id}@test.com"))
            .execute(pool).await;
        id
    }

    #[tokio::test]
    async fn submission_records_and_filters_by_workspace() {
        let Some(pool) = test_pool().await else { return };
        let user = make_user(&pool).await;

        let s = create(&pool, user, "summary", &["page-a".into()], "user/abc", "team-alpha")
            .await.unwrap();
        assert_eq!(s.workspace_slug, "team-alpha");

        let in_alpha = list_pending_for_workspace(&pool, "team-alpha").await.unwrap();
        assert!(in_alpha.iter().any(|x| x.id == s.id));

        let in_beta = list_pending_for_workspace(&pool, "team-beta").await.unwrap();
        assert!(!in_beta.iter().any(|x| x.id == s.id));
    }
}
```

- [ ] **Step 1.5: Run the test, verify it fails to COMPILE**

Run:
```bash
TEST_DATABASE_URL=postgres://cowiki:cowiki@localhost:5432/cowiki cargo test -p cowiki-db submission_records_and_filters_by_workspace
```
Expected: compile error — `create` takes 5 args not 6, `list_pending_for_workspace` does not exist, `Submission` has no field `workspace_slug`. That is the failing state.

- [ ] **Step 1.6: Update the `Submission` struct + `create` + add `list_pending_for_workspace`**

In `crates/db/src/submissions.rs`, add the field to the struct (after `source_branch`):
```rust
    pub source_branch: String,
    pub workspace_slug: String,
```

Replace `create` with:
```rust
pub async fn create(
    pool: &PgPool,
    user_id: Uuid,
    summary: &str,
    page_slugs: &[String],
    source_branch: &str,
    workspace_slug: &str,
) -> sqlx::Result<Submission> {
    sqlx::query_as::<_, Submission>(
        "INSERT INTO submissions (user_id, summary, page_slugs, source_branch, workspace_slug) \
         VALUES ($1, $2, $3, $4, $5) RETURNING *",
    )
    .bind(user_id)
    .bind(summary)
    .bind(page_slugs)
    .bind(source_branch)
    .bind(workspace_slug)
    .fetch_one(pool)
    .await
}
```

Add below `list_pending`:
```rust
pub async fn list_pending_for_workspace(
    pool: &PgPool,
    workspace_slug: &str,
) -> sqlx::Result<Vec<Submission>> {
    sqlx::query_as::<_, Submission>(
        "SELECT * FROM submissions WHERE status = 'pending' AND workspace_slug = $1 \
         ORDER BY created_at DESC",
    )
    .bind(workspace_slug)
    .fetch_all(pool)
    .await
    .map_err(|e| { tracing::error!("DB list submissions for workspace failed: {e}"); e })
}
```

> Note: keep the old `list_pending` for now; it is removed in Task 4 once no caller remains. (Removing it here would break `review.rs` before Task 3 rewrites it.)

- [ ] **Step 1.7: Run the test, verify it passes**

Run:
```bash
TEST_DATABASE_URL=postgres://cowiki:cowiki@localhost:5432/cowiki cargo test -p cowiki-db submission_records_and_filters_by_workspace
```
Expected: PASS.

- [ ] **Step 1.8: Commit**

```bash
git add crates/db/src/migrations/008_submission_workspace.sql crates/db/src/lib.rs crates/db/src/submissions.rs crates/db/src/workspaces.rs
git commit -m "feat(db): scope submissions to a workspace

Add submissions.workspace_slug (migration 008), thread it through
create(), and add list_pending_for_workspace(). Enables submit/review
to resolve the correct per-workspace git repo."
```

---

## Task 2: Make `submit` workspace-scoped

The `submit` handler is the only legacy route with a live frontend caller. After this task, submissions write to the workspace repo and record their `workspace_slug`.

**Files:**
- Modify: `crates/server/src/routes/submit.rs`

### Verification reality
The `submit` handler embeds pages and calls the LLM for the summary, so it is not unit-testable without a live model. It is verified by **compile + clippy** (the change is type-level: a new `Path` extractor + swapping `state.wiki_repo` for the resolved `repo`) and a **manual smoke** in Task 7.

- [ ] **Step 2.1: Rewrite the handler signature + body**

In `crates/server/src/routes/submit.rs`:

1. Add `Path` to the axum import on line 1:
```rust
use axum::extract::{Path, State};
```

2. Remove the now-unused request field. In `SubmitRequest`, delete:
```rust
    /// Required when skip_review is true — identifies the workspace to verify authorization.
    pub workspace_slug: Option<String>,
```

3. Change the handler signature to take the workspace slug from the path and resolve the repo once:
```rust
pub async fn submit(
    State(state): State<Arc<AppState>>,
    Path(ws_slug): Path<String>,
    headers: axum::http::HeaderMap,
    Json(input): Json<SubmitRequest>,
) -> Result<Json<SubmitResponse>> {
    let user = extract_user(&state.db, &headers).await?;
    let repo = state.repo_manager.get(&ws_slug)
        .map_err(|e| AppError::Internal(format!("repo error: {e}")))?;
    super::pages::ensure_user_branch_if_needed(&repo, &input.branch)?;

    let diffs = match repo.diff_files(&input.branch, &input.page_slugs) {
```

4. Replace the embedding read (lines ~54-58) `state.wiki_repo` → `repo`:
```rust
        if let Some(content) = repo
            .read_file(&input.branch, &path)
            .map_err(|e| AppError::Internal(e.to_string()))?
```

5. In the `skip_review` block, replace the `workspace_slug` lookup (which read the optional body field) with `ws_slug` from the path:
```rust
    if input.skip_review {
        // Authorization: skip_review only allowed for personal workspaces (private + owner)
        let ws = cowiki_db::workspaces::find_by_slug(&state.db, &ws_slug)
            .await?
            .ok_or_else(|| AppError::NotFound("workspace not found".into()))?;
        if ws.visibility != "private" {
            return Err(AppError::Forbidden(
                "skip_review is only allowed for personal (private) workspaces".into(),
            ));
        }
        let role = cowiki_db::workspaces::get_member_role(&state.db, ws.id, user.id)
            .await?
            .unwrap_or_default();
        if role != "owner" {
            return Err(AppError::Forbidden(
                "skip_review is only allowed for the workspace owner".into(),
            ));
        }

        let file_paths: Vec<String> = input
            .page_slugs
            .iter()
            .map(|s| format!("wiki/{s}.md"))
            .collect();
        repo
            .merge_to_main(
                &input.branch,
                &file_paths,
                &user.name,
                &format!("commit: {summary}"),
            )
            .map_err(|e| AppError::Internal(e.to_string()))?;

        return Ok(Json(SubmitResponse {
            submission_id: uuid::Uuid::nil(),
            summary,
            duplicates,
        }));
    }
```

6. Pass `ws_slug` into the submission create call (Team Space branch):
```rust
    let submission = cowiki_db::submissions::create(
        &state.db,
        user.id,
        &summary,
        &input.page_slugs,
        &input.branch,
        &ws_slug,
    )
    .await?;
```

> Leave the dedup `find_similar(&state.db, emb, "main", 3, 0.85)` call as-is. Cross-workspace dedup scoping is a pre-existing `pages`-table concern (the `pages.workspace_slug` column is uniformly `''` today) and is explicitly out of scope — see "Known follow-ups". Duplicates here are advisory warnings only, so behavior is unchanged.

- [ ] **Step 2.2: Compile (expect router error — that's fine, fixed in Task 4)**

Run:
```bash
cargo build -p cowiki-server 2>&1 | head -30
```
Expected: the only errors are in `main.rs` where the router still wires the old `submit` signature (no `Path`). The handler file itself compiles. The router is rewired in Task 4. If `submit.rs` has its OWN errors, fix them now.

- [ ] **Step 2.3: Commit**

```bash
git add crates/server/src/routes/submit.rs
git commit -m "feat(server): make submit workspace-scoped

Resolve the per-workspace repo from a {ws_slug} path param, write to it
instead of the global repo, and record workspace_slug on the submission.
Router rewiring follows in the legacy-removal commit."
```

---

## Task 3: Make `review` workspace-scoped

Review currently reads/merges on the global repo and attributes actions to the `default` user. Rewrite it to resolve the repo from the submission's workspace and attribute actions to the authenticated reviewer.

**Files:**
- Modify: `crates/server/src/routes/review.rs`

### Design decisions (locked)
- `list_reviews` lists pending submissions **for one workspace** (`Path(ws_slug)` → `list_pending_for_workspace`).
- `get_review` and `review_action` take `Path((ws_slug, id))`. The repo is resolved from `ws_slug`; we also assert the loaded submission's `workspace_slug == ws_slug` (guards against cross-workspace id access).
- `review_action` now identifies the reviewer via `extract_user` (replacing `get_default`) and requires the reviewer to be a workspace member with role `owner` or `writer`. This is required by scoping — without a workspace we previously had nothing to authorize against, and the endpoint had no auth at all.

- [ ] **Step 3.1: Rewrite `review.rs`**

Replace the whole file body with:
```rust
use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;

use crate::error::{AppError, Result};
use crate::routes::auth::extract_user;
use crate::AppState;

pub async fn list_reviews(
    State(state): State<Arc<AppState>>,
    Path(ws_slug): Path<String>,
) -> Result<Json<Vec<cowiki_db::submissions::Submission>>> {
    let subs = cowiki_db::submissions::list_pending_for_workspace(&state.db, &ws_slug).await?;
    Ok(Json(subs))
}

#[derive(Serialize)]
pub struct ReviewDetail {
    pub submission: cowiki_db::submissions::Submission,
    pub diffs: Vec<cowiki_core::git::FileDiff>,
}

pub async fn get_review(
    State(state): State<Arc<AppState>>,
    Path((ws_slug, id)): Path<(String, uuid::Uuid)>,
) -> Result<Json<ReviewDetail>> {
    let submission = cowiki_db::submissions::find_by_id(&state.db, id)
        .await?
        .ok_or_else(|| AppError::NotFound("submission not found".into()))?;
    if submission.workspace_slug != ws_slug {
        return Err(AppError::NotFound("submission not found in this workspace".into()));
    }

    let repo = state.repo_manager.get(&ws_slug)
        .map_err(|e| AppError::Internal(format!("repo error: {e}")))?;
    let diffs = repo
        .diff_files(&submission.source_branch, &submission.page_slugs)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(ReviewDetail { submission, diffs }))
}

#[derive(Deserialize)]
pub struct ReviewAction {
    pub action: String,
}

pub async fn review_action(
    State(state): State<Arc<AppState>>,
    Path((ws_slug, id)): Path<(String, uuid::Uuid)>,
    headers: axum::http::HeaderMap,
    Json(input): Json<ReviewAction>,
) -> Result<Json<serde_json::Value>> {
    let reviewer = extract_user(&state.db, &headers).await?;

    let submission = cowiki_db::submissions::find_by_id(&state.db, id)
        .await?
        .ok_or_else(|| AppError::NotFound("submission not found".into()))?;
    if submission.workspace_slug != ws_slug {
        return Err(AppError::NotFound("submission not found in this workspace".into()));
    }

    // Authorization: reviewer must be a writer/owner of the workspace.
    let ws = cowiki_db::workspaces::find_by_slug(&state.db, &ws_slug)
        .await?
        .ok_or_else(|| AppError::NotFound("workspace not found".into()))?;
    let role = cowiki_db::workspaces::get_member_role(&state.db, ws.id, reviewer.id)
        .await?
        .unwrap_or_default();
    if role != "owner" && role != "writer" {
        return Err(AppError::Forbidden(
            "only workspace owners or writers can review".into(),
        ));
    }

    let repo = state.repo_manager.get(&ws_slug)
        .map_err(|e| AppError::Internal(format!("repo error: {e}")))?;

    match input.action.as_str() {
        "approve" => {
            let file_paths: Vec<String> = submission
                .page_slugs
                .iter()
                .map(|s| format!("wiki/{s}.md"))
                .collect();

            repo
                .merge_to_main(
                    &submission.source_branch,
                    &file_paths,
                    &reviewer.name,
                    &format!("approve: {}", submission.summary),
                )
                .map_err(|e| AppError::Internal(e.to_string()))?;

            for slug in &submission.page_slugs {
                let path = format!("wiki/{slug}.md");
                if let Some(content) = repo
                    .read_file("main", &path)
                    .map_err(|e| AppError::Internal(e.to_string()))?
                {
                    let text = String::from_utf8_lossy(&content);
                    let hash = format!("{:x}", Sha256::digest(text.as_bytes()));
                    if let Ok(emb) = state.compiler.embed(&text).await {
                        cowiki_db::pages::upsert(
                            &state.db,
                            slug,
                            slug,
                            "",
                            "main",
                            &hash,
                            Some(&emb),
                            reviewer.id,
                        )
                        .await
                        .ok();
                    }
                }
            }

            cowiki_db::submissions::update_status(&state.db, id, "approved", reviewer.id).await?;
        }
        "reject" => {
            cowiki_db::submissions::update_status(&state.db, id, "rejected", reviewer.id).await?;
        }
        _ => return Err(AppError::BadRequest("invalid action".into())),
    }

    Ok(Json(serde_json::json!({"ok": true})))
}
```

> `pages::upsert` still passes `""` for the `workspace_slug` argument position (3rd positional `summary` arg is `""` as before; the page's own `workspace_slug` column keeps its `''` default). Per-workspace `pages` scoping is a separate follow-up (see end). This preserves today's behavior exactly while fixing the git repo.

- [ ] **Step 3.2: Compile-check the handler file**

Run:
```bash
cargo build -p cowiki-server 2>&1 | grep -A3 "review.rs" | head -20
```
Expected: no errors originating in `review.rs`. Remaining errors are router wiring in `main.rs` (Task 4).

- [ ] **Step 3.3: Commit**

```bash
git add crates/server/src/routes/review.rs
git commit -m "feat(server): make review workspace-scoped

Resolve the repo from the submission's workspace, list pending per
workspace, attribute actions to the authenticated reviewer (replacing
the default-user crutch), and require owner/writer role to review."
```

---

## Task 4: Remove the legacy global repo and all legacy routes/handlers

Now that submit/review are workspace-scoped, nothing needs `state.wiki_repo`. Delete it and every legacy route/handler, and rewire the router.

**Files:**
- Modify: `crates/server/src/main.rs` (AppState field, init, router)
- Modify: `crates/server/src/routes/pages.rs` (delete legacy `list_pages`, `get_page`, `write_page`, `create_folder`)
- Modify: `crates/server/src/routes/ingest.rs` (delete legacy `ingest`)
- Modify: `crates/server/src/routes/compile.rs` (delete legacy `compile`)
- Modify: `crates/server/src/routes/workspace.rs` (drop the global-repo double branch-creation at ~lines 122, 334)
- Modify: `crates/core/src/git.rs` (delete `WikiRepoManager::default_repo`)
- Modify: `crates/db/src/submissions.rs` (delete now-unused `list_pending`)

> Keep `WikiRepo::open_or_init` — it is still called by `WikiRepoManager::get` to init each per-workspace repo (`crates/core/src/git.rs:76`). Only the global usage and `default_repo`/`_default` go away.

- [ ] **Step 4.1: Remove `wiki_repo` from `AppState` and init**

In `crates/server/src/main.rs`:

1. Delete the field (line ~20):
```rust
    pub wiki_repo: cowiki_core::git::WikiRepo,       // default repo (backward compat)
```

2. Delete the init block (lines ~63-68):
```rust
    // Default repo for backward compat
    let wiki_repo = cowiki_core::git::WikiRepo::open_or_init(&config.server.data_dir)
        .expect("failed to init default wiki repo");
    wiki_repo
        .ensure_user_branch("default")
        .expect("failed to create default user branch");
```
Keep the `let repo_manager = ...` line and the `tracing::info!("wiki repos dir: ...")` line.

3. Delete `wiki_repo,` from the `AppState { ... }` constructor (line ~96).

- [ ] **Step 4.2: Rewire the router**

In `crates/server/src/main.rs`, in the `Router::new()` chain:

Delete these legacy routes:
```rust
        .route("/api/pages", get(routes::pages::list_pages))
        .route("/api/pages", post(routes::pages::write_page))
        .route("/api/folders", post(routes::pages::create_folder))
        .route("/api/pages/{*slug}", get(routes::pages::get_page))
        .route("/api/ingest", post(routes::ingest::ingest))
        .route("/api/compile", post(routes::compile::compile))
        .route("/api/submit", post(routes::submit::submit))
        .route("/api/reviews", get(routes::review::list_reviews))
        .route("/api/reviews/{id}", get(routes::review::get_review))
        .route("/api/reviews/{id}", post(routes::review::review_action))
```

Add the workspace-scoped submit/review routes (next to the other `/api/workspaces/{ws_slug}/...` routes):
```rust
        .route("/api/workspaces/{ws_slug}/submit", post(routes::submit::submit))
        .route("/api/workspaces/{ws_slug}/reviews", get(routes::review::list_reviews))
        .route("/api/workspaces/{ws_slug}/reviews/{id}", get(routes::review::get_review))
        .route("/api/workspaces/{ws_slug}/reviews/{id}", post(routes::review::review_action))
```

- [ ] **Step 4.3: Delete legacy handlers in `pages.rs`**

In `crates/server/src/routes/pages.rs`, delete the four legacy handlers (keep the `_ws` versions and all shared helpers like `list_pages_from_repo`, `parse_frontmatter`, `ensure_user_branch_if_needed`, and the `WritePage`/`CreateFolder`/`ListParams` structs):
- `pub async fn list_pages(...)` (the non-`_ws` one, ~lines 37-)
- `pub async fn get_page(...)` (non-`_ws`)
- `pub async fn write_page(...)` (non-`_ws`, ~lines 186-)
- `pub async fn create_folder(...)` (non-`_ws`)

> If a struct like `CreateFolder` is defined immediately above the legacy `create_folder` handler, KEEP the struct (the `_ws` handler uses it); delete only the `pub async fn`.

- [ ] **Step 4.4: Delete legacy `ingest` and `compile` handlers**

In `crates/server/src/routes/ingest.rs`, delete:
```rust
/// Legacy ingest (uses default repo)
pub async fn ingest(
    State(state): State<Arc<AppState>>,
    Json(input): Json<IngestRequest>,
) -> Result<Json<IngestResponse>> {
    super::pages::ensure_user_branch_if_needed(&state.wiki_repo, &input.branch)?;
    do_ingest(&state.wiki_repo, input).await
}
```
Keep `ingest_ws` and `do_ingest`.

In `crates/server/src/routes/compile.rs`, delete:
```rust
/// Legacy compile (uses default repo)
pub async fn compile(
    State(state): State<Arc<AppState>>,
    Json(input): Json<CompileRequest>,
) -> Result<Json<CompileResponse>> {
    super::pages::ensure_user_branch_if_needed(&state.wiki_repo, &input.branch)?;
    do_compile(&state, &state.wiki_repo, &input.branch).await
}
```
Keep `compile_ws` and `do_compile`.

- [ ] **Step 4.5: Drop the global-repo double branch creation in `workspace.rs`**

In `crates/server/src/routes/workspace.rs` at ~line 122, delete the global-repo call and its comment, keeping the `repo_manager` call:
```rust
    // Create user branch in default repo AND workspace repo
    state.wiki_repo
        .ensure_user_branch(&user.id.to_string())
        .map_err(|e| AppError::Internal(e.to_string()))?;
```
Do the same at ~line 334. After editing, the remaining block should only call `state.repo_manager.get(&workspace_slug)?.ensure_user_branch(...)`. Update the leftover comment to "Create user branch in workspace repo".

- [ ] **Step 4.6: Delete `default_repo` in `git.rs`**

In `crates/core/src/git.rs`, delete:
```rust
    /// Get the "default" repo for backward compatibility.
    /// TODO: Remove once all routes use workspace-scoped repos.
    pub fn default_repo(&self) -> Result<Arc<WikiRepo>, git2::Error> {
        self.get("_default")
    }
```

- [ ] **Step 4.7: Delete the now-unused `list_pending` in `submissions.rs`**

In `crates/db/src/submissions.rs`, delete `pub async fn list_pending(...)` (the workspace-agnostic one). `list_pending_for_workspace` replaces it.

- [ ] **Step 4.8: Build the whole workspace**

Run:
```bash
cargo build 2>&1 | tail -30
```
Expected: clean build. Fix any remaining references the compiler flags. Common ones: an unused `use` for `extract_user` or `Path`, or a leftover `state.wiki_repo` — the compiler will name the file:line.

- [ ] **Step 4.9: Clippy + remaining-trace sweep**

Run:
```bash
cargo clippy --all-targets 2>&1 | tail -20
grep -rn --include="*.rs" "wiki_repo\|default_repo\|\"_default\"\|open_or_init" crates | grep -v "unwrap_or_default\|or_default()"
```
Expected: clippy clean (no new warnings); the grep returns ONLY the legitimate `WikiRepo::open_or_init` definition in `git.rs` and its call inside `WikiRepoManager::get`. No `wiki_repo`, no `default_repo`, no `"_default"` anywhere.

- [ ] **Step 4.10: Run DB tests again (regression)**

Run:
```bash
TEST_DATABASE_URL=postgres://cowiki:cowiki@localhost:5432/cowiki cargo test -p cowiki-db
```
Expected: all pass, including `submission_records_and_filters_by_workspace`.

- [ ] **Step 4.11: Commit**

```bash
git add crates/server/src/main.rs crates/server/src/routes/pages.rs crates/server/src/routes/ingest.rs crates/server/src/routes/compile.rs crates/server/src/routes/workspace.rs crates/core/src/git.rs crates/db/src/submissions.rs
git commit -m "refactor: remove legacy global wiki repo

Delete AppState.wiki_repo, its init, all legacy non-workspace routes
(/api/pages, /api/ingest, /api/compile, /api/submit, /api/reviews) and
their handlers, the WikiRepoManager::default_repo helper, the redundant
global branch creation in workspace.rs, and the unused list_pending.
All wiki operations now go through per-workspace repos."
```

---

## Task 5: Migrate the frontend to the workspace-scoped endpoints

The only live legacy caller is `submit`. The review functions exist in `api.ts` but have no UI caller (confirmed: `grep -rln listReviews web/src` → only `api.ts`). We still update them for consistency, and we make the already-optional `workspaceSlug` params required (removing the dead legacy fallbacks).

**Files:**
- Modify: `web/src/api.ts`
- Modify: `web/src/pages/MainLayout.tsx` (pass `ws.slug` to `submit`)

- [ ] **Step 5.1: Rewrite submit/review in `api.ts`**

Replace the `// ── Submit & Review ──` section:
```ts
// ── Submit & Review ──

export async function submit(branch: string, pageSlugs: string[], skipReview: boolean, workspaceSlug: string) {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/submit`, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({ branch, page_slugs: pageSlugs, skip_review: skipReview }),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || `Request failed: ${res.status}`);
  }
  return res.json();
}

export async function listReviews(workspaceSlug: string): Promise<Submission[]> {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/reviews`, { headers: h() });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || `Request failed: ${res.status}`);
  }
  return res.json();
}

export async function getReview(workspaceSlug: string, id: string): Promise<ReviewDetail> {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/reviews/${id}`, { headers: h() });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || `Request failed: ${res.status}`);
  }
  return res.json();
}

export async function reviewAction(workspaceSlug: string, id: string, action: string) {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/reviews/${id}`, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({ action }),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || `Request failed: ${res.status}`);
  }
  return res.json();
}
```

- [ ] **Step 5.2: Remove the legacy fallbacks from pages/ingest/compile/folders**

In `web/src/api.ts`, make `workspaceSlug` a required `string` and drop the ternary legacy branch in each of: `listPages`, `getPage`, `writePage`, `ingest`, `compile`, `createFolder`. Example for `listPages` (apply the same shape to the others — always use the `/workspaces/${workspaceSlug}/...` URL):
```ts
export async function listPages(branch = 'main', workspaceSlug: string): Promise<PageMeta[]> {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/pages?branch=${branch}`, { headers: h() });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || `Request failed: ${res.status}`);
  }
  return res.json();
}
```

> `writePage`, `ingest`, `compile`, `createFolder`, `getPage` follow the same pattern: drop `workspaceSlug?` → `workspaceSlug` (required), delete the `const url = workspaceSlug ? ws : legacy` ternary, and hardcode the `/workspaces/${workspaceSlug}/...` URL that was the truthy branch.

- [ ] **Step 5.3: Update the `submit` caller**

In `web/src/pages/MainLayout.tsx:389`, change:
```ts
      await submit(userBranch, slugs, isPersonal);
```
to:
```ts
      await submit(userBranch, slugs, isPersonal, ws.slug);
```

- [ ] **Step 5.4: Typecheck — let the compiler find any caller that broke**

Run:
```bash
cd web && npm run build
```
Expected: `tsc -b` passes. If a call to `listPages`/`getPage`/`writePage`/`ingest`/`compile`/`createFolder` somewhere omitted `workspaceSlug`, tsc now errors with the exact file:line — fix each by passing the active workspace slug (every call site already has an `activeWorkspace`/`ws` in scope, per the audit). Return to repo root when done (`cd ..`).

- [ ] **Step 5.5: Commit**

```bash
git add web/src/api.ts web/src/pages/MainLayout.tsx
git commit -m "feat(web): use workspace-scoped submit/review endpoints

submit() now targets /workspaces/{slug}/submit and passes the active
workspace; review helpers are workspace-scoped; pages/ingest/compile/
folders drop their dead legacy non-workspace fallbacks."
```

---

## Task 6: Docs + config sweep

**Files:**
- Modify: `README.md`, `docs/config.md`, `CONTEXT.md` (only if they reference the global repo)

- [ ] **Step 6.1: Find stale references to the global repo / legacy endpoints**

Run:
```bash
grep -rn "default repo\|data/repo\b\|/api/submit\b\|/api/reviews\b\|backward compat" README.md docs CONTEXT.md 2>/dev/null
```

- [ ] **Step 6.2: Fix anything the grep surfaces**

Update prose to describe per-workspace repos (`data/{workspace_slug}/repo`) and the workspace-scoped endpoints. If the grep returns nothing, skip. Do not invent doc sections that did not exist.

- [ ] **Step 6.3: Commit (skip if no doc changes)**

```bash
git add README.md docs CONTEXT.md
git commit -m "docs: describe per-workspace repos, drop legacy-repo references"
```

---

## Task 7: Manual end-to-end smoke test

Route handlers can't be unit-tested (LLM/embedding/git side effects), so verify the real flow once against a running stack. There is a helper script: `scripts/setup-multi-user-test.sh` and a guide at `docs/multi-user-testing-guide.md` — read the guide first; the steps below are the minimum.

- [ ] **Step 7.1: Run the stack**

```bash
docker compose up -d postgres
cp cowiki.conf.example cowiki.conf   # set a real OPENAI_API_KEY + GitHub creds
cargo run -p cowiki-server &          # REST API on :3000
( cd web && npm install && npm run dev )   # frontend on :5173
```

- [ ] **Step 7.2: Exercise the flow in a private (personal) workspace**

In the UI: ingest a small source → compile → submit. Because a personal space is `private` + owner, submit takes the `skip_review` path and commits directly. Confirm the page appears in the shared view.

Expected on disk: changes land under `data/{personal-xxxxxxxx}/repo`, and NO `data/repo` directory is created. Verify:
```bash
ls data/ | grep -x repo && echo "LEGACY REPO STILL CREATED — FAIL" || echo "no legacy global repo — OK"
```

- [ ] **Step 7.3: Exercise the team review path**

Create/join a `public` workspace as owner. Submit pages (non-skip) → `POST /api/workspaces/{slug}/submit` creates a pending submission. Then verify the review API returns it and approve works:
```bash
# replace TOKEN, SLUG, ID
curl -s -H "Authorization: Bearer $TOKEN" http://localhost:3000/api/workspaces/$SLUG/reviews | jq
curl -s -X POST -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d '{"action":"approve"}' http://localhost:3000/api/workspaces/$SLUG/reviews/$ID | jq
```
Expected: `reviews` lists the pending submission; approve returns `{"ok":true}` and merges into that workspace's `main`. A non-member token returns 403.

- [ ] **Step 7.4: Record the smoke result**

Note pass/fail for 7.2 and 7.3 in the PR description. If anything fails, fix before opening the PR for review.

---

## Task 8: Open the PR

- [ ] **Step 8.1: Push and open the PR**

```bash
git push -u origin refactor/workspace-scoped-repos
gh pr create --base main --title "Remove legacy global wiki repo; make submit/review workspace-scoped" \
  --body "$(cat <<'EOF'
## What
Completes the half-finished migration to per-workspace git repos. Gives
`submit` and `review` a workspace dimension, then removes the legacy
global "default repo" (`data/repo`) and all its routes/handlers/fields.

## Why
`submit`/`review` still wrote to a single global repo, so different users'
spaces were not fully isolated and a stale `data/repo` shadowed the
per-workspace repos. The codebase already had a `TODO: Remove once all
routes use workspace-scoped repos`.

## Changes
- db: `submissions.workspace_slug` (migration 008) + `list_pending_for_workspace`
- server: `submit`/`review` resolve the repo via `repo_manager.get(ws_slug)`;
  review attributes actions to the authenticated reviewer (owner/writer only)
- server: deleted `AppState.wiki_repo`, all legacy non-workspace routes,
  `WikiRepoManager::default_repo`, redundant global branch creation
- web: workspace-scoped submit/review; removed dead legacy fallbacks

## Testing
- `cargo test -p cowiki-db` (incl. new workspace-scoping test) — green
- `cargo clippy --all-targets` — clean
- `npm run build` (tsc) — green
- Manual smoke: personal skip-review commit + team review/approve — <fill pass/fail>

## Known follow-ups (out of scope)
- `pages.workspace_slug` is still uniformly `''`; per-workspace dedup/search
  scoping is a separate change.
- OAuth `api_key` is passed as a redirect query param (pre-existing SECURITY TODO).
EOF
)"
```

---

## Self-Review (completed during plan authoring)

- **Spec coverage:** submit→workspace (Task 2), review→workspace (Task 3), schema (Task 1), legacy removal incl. routes/handlers/`wiki_repo`/`default_repo`/`_default`/workspace.rs double-write (Task 4), frontend (Task 5), docs (Task 6), verification (Tasks 7–8). All "remove all traces" targets identified in code analysis are covered.
- **Placeholder scan:** every code step contains the exact code; the only deferred items are explicitly labeled "Known follow-ups" with rationale, not hidden TODOs.
- **Type consistency:** `create(...workspace_slug: &str)`, `list_pending_for_workspace(pool, workspace_slug)`, `Submission.workspace_slug`, and the `submit(branch, pageSlugs, skipReview, workspaceSlug)` / `reviewAction(workspaceSlug, id, action)` signatures are used identically across backend and frontend tasks.
- **Kept-not-deleted guard:** `WikiRepo::open_or_init` is retained (used by `WikiRepoManager::get`); only the global usage and `default_repo`/`_default` are removed.

## Known follow-ups (explicitly OUT of scope for this plan)
1. **`pages` table workspace scoping** — `pages.workspace_slug` is uniformly `''`; dedup (`find_similar`) and search are not workspace-isolated. Separate change.
2. **8-hex-char UUID truncation in workspace slugs** (`auth.rs`) — collision-safe only at team scale.
3. **per-user "General" workspace** created on signup — no single shared team space by default.
4. **OAuth `api_key` in redirect query** — pre-existing `SECURITY TODO` in `auth.rs`.
