# Cloud Git, PostgreSQL, and Pull Request Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a production MVP in which a desktop Space syncs its local `main` through authenticated Git Smart HTTP to a PostgreSQL-backed Cloud Space and submits a live `user/<id>` pull request that Owner or Manager can fast-forward merge.

**Architecture:** Add a standalone Axum Cloud service with PostgreSQL as the control plane and one bare Git repository per Space. Add a focused desktop sync module that preserves existing remotes, owns only the `cowiki` remote, rebases clean local `main`, pushes with a lease, and returns structured conflicts; expose capabilities through Tauri commands without changing the UI.

**Tech Stack:** Rust 2024, Axum 0.8, Tokio, SQLx/PostgreSQL, git2 plus the system Git CGI backend, Tauri 2, Docker Compose, Node contract tests.

---

## File map

- `cloud/Cargo.toml`: standalone service dependencies.
- `cloud/migrations/001_control_plane.sql`: PostgreSQL schema and constraints.
- `cloud/src/config.rs`: validated environment configuration.
- `cloud/src/db.rs`: SQLx pool, migrations, identity, Space, membership, PR, approval, and audit queries.
- `cloud/src/auth.rs`: GitHub OAuth, one-time desktop exchange, API-key hashing, and request authentication.
- `cloud/src/git_repo.rs`: bare repository lifecycle, ref inspection, compare-and-swap merge, and hook installation.
- `cloud/src/git_http.rs`: authenticated CGI adapter for Git Smart HTTP.
- `cloud/src/spaces.rs`: Space REST endpoints and bootstrap metadata.
- `cloud/src/pull_requests.rs`: live-head PR, approval invalidation, and fast-forward merge endpoints.
- `cloud/src/lib.rs`: router and application state.
- `cloud/src/main.rs`: startup, migration, signal handling.
- `cloud/tests/*.rs`: unit/API/PostgreSQL/Git integration contracts.
- `web/src-tauri/src/cloud_sync.rs`: local `main` normalization, Cloud link persistence, fetch/rebase/push, and structured conflicts.
- `web/src-tauri/src/local_engine.rs`: initialize Cloud link table, expose repository path helpers, and require `main` for new/imported Spaces.
- `web/src-tauri/src/local_engine/agent_changes.rs`: rename background refs to `agent/<change-id>`.
- `web/src-tauri/src/lib.rs`: Tauri command wiring.
- `web/src/local-api.ts`: typed invoke contracts only; no UI changes.
- `docker-compose.cloud.yml`, `.env.cloud.example`, `cloud/Dockerfile`: production-shaped local deployment.
- `scripts/cloud-e2e.sh`: real PostgreSQL plus Git/PR acceptance flow.
- `docs/cloud-deployment.md`: operations, storage, and backup runbook.

### Task 1: Cloud crate and PostgreSQL foundation

**Files:**
- Create: `cloud/Cargo.toml`
- Create: `cloud/src/config.rs`
- Create: `cloud/src/db.rs`
- Create: `cloud/src/error.rs`
- Create: `cloud/src/model.rs`
- Create: `cloud/src/lib.rs`
- Create: `cloud/src/main.rs`
- Create: `cloud/migrations/001_control_plane.sql`
- Test: `cloud/tests/foundation.rs`

- [x] **Step 1: Write failing foundation tests**

Test that missing `DATABASE_URL`, a relative repository root, malformed public origin, and a too-short secret are rejected, and assert the migration contains the four role values plus the partial unique open-PR index.

- [x] **Step 2: Run the focused test and observe failure**

Run: `cargo test --manifest-path cloud/Cargo.toml --test foundation`
Expected: FAIL because the `cowiki_cloud` crate and configuration types do not exist.

- [x] **Step 3: Implement the minimal Cloud crate and schema**

Define `Config::from_iter`, `AppError`, shared DTOs, `db::connect_and_migrate`, and an Axum `/healthz` router. Use SQLx `migrate!()` and PostgreSQL enums/check constraints for `owner`, `manager`, `editor`, and `viewer`. Add `UNIQUE (space_id, head_ref) WHERE status = 'open'`.

- [x] **Step 4: Run foundation tests**

Run: `cargo test --manifest-path cloud/Cargo.toml --test foundation`
Expected: PASS.

- [x] **Step 5: Commit foundation**

```bash
git add cloud
git commit -m "feat(cloud): add PostgreSQL control plane foundation"
```

### Task 2: Revocable desktop authentication

**Files:**
- Create: `cloud/src/auth.rs`
- Modify: `cloud/src/db.rs`
- Modify: `cloud/src/lib.rs`
- Test: `cloud/tests/auth.rs`

- [x] **Step 1: Write failing auth tests**

Cover SHA-256 API-key hashing, constant-time verification, 10-minute OAuth state expiry, 60-second one-time desktop exchange, replay rejection, and rejection of unallowlisted callback origins.

- [x] **Step 2: Run the auth tests and observe failure**

Run: `cargo test --manifest-path cloud/Cargo.toml --test auth`
Expected: FAIL because authentication handlers and stores are absent.

- [x] **Step 3: Implement authentication**

Add GitHub authorize/callback handlers, one-time code exchange, Bearer extraction, revocation-aware API-key lookup, and an `AuthenticatedUser` Axum extractor. Return the existing desktop shape `{ apiKey, userName, userId }`.

- [x] **Step 4: Run auth tests**

Run: `cargo test --manifest-path cloud/Cargo.toml --test auth`
Expected: PASS.

- [x] **Step 5: Commit authentication**

```bash
git add cloud/src cloud/tests/auth.rs
git commit -m "feat(cloud): add revocable desktop authentication"
```

### Task 3: Bare Space repositories and protected refs

**Files:**
- Create: `cloud/src/git_repo.rs`
- Modify: `cloud/src/lib.rs`
- Test: `cloud/tests/git_repo.rs`

- [x] **Step 1: Write failing repository tests**

Create temporary bare repos and assert path derivation uses UUIDs, initialization is idempotent, bootstrap accepts only equal `main` and `user/<owner>` OIDs, normal validation permits only `user/<authenticated-id>`, and compare-and-swap fast-forward rejects stale or non-descendant heads.

- [x] **Step 2: Run the repository tests and observe failure**

Run: `cargo test --manifest-path cloud/Cargo.toml --test git_repo`
Expected: FAIL because `GitRepoStore` does not exist.

- [x] **Step 3: Implement repository lifecycle and policy**

Use `git init --bare --initial-branch=main`, configure `http.receivepack=true`, install a generated executable `pre-receive` hook, expose OID/ancestor/ref-update helpers, and serialize mutations with per-Space Tokio mutexes.

- [x] **Step 4: Run repository tests**

Run: `cargo test --manifest-path cloud/Cargo.toml --test git_repo`
Expected: PASS.

- [x] **Step 5: Commit repositories**

```bash
git add cloud/src/git_repo.rs cloud/src/lib.rs cloud/tests/git_repo.rs
git commit -m "feat(cloud): store Spaces as protected bare Git repositories"
```

### Task 4: Authenticated Git Smart HTTP

**Files:**
- Create: `cloud/src/git_http.rs`
- Modify: `cloud/src/lib.rs`
- Test: `cloud/tests/git_http.rs`

- [x] **Step 1: Write failing protocol tests**

Assert upload-pack works for Viewer, receive-pack rejects Viewer, unauthorized requests return a Git-compatible 401, route paths cannot escape the repository root, and authenticated Editor can bootstrap then push only their own user ref.

- [x] **Step 2: Run the Git HTTP tests and observe failure**

Run: `cargo test --manifest-path cloud/Cargo.toml --test git_http`
Expected: FAIL because no Git routes exist.

- [x] **Step 3: Implement the CGI adapter**

Translate Axum method/query/headers/body into `git http-backend` CGI variables, pass authenticated `COWIKI_USER_ID`, `COWIKI_ROLE`, and bootstrap state through the child environment, parse `Status` and response headers, and stream the bounded response body back to Axum.

- [x] **Step 4: Run Git HTTP tests**

Run: `cargo test --manifest-path cloud/Cargo.toml --test git_http`
Expected: PASS.

- [ ] **Step 5: Commit Smart HTTP**

```bash
git add cloud/src/git_http.rs cloud/src/lib.rs cloud/tests/git_http.rs
git commit -m "feat(cloud): serve authenticated Git Smart HTTP"
```

### Task 5: Spaces, memberships, and bootstrap API

**Files:**
- Create: `cloud/src/spaces.rs`
- Modify: `cloud/src/db.rs`
- Modify: `cloud/src/lib.rs`
- Test: `cloud/tests/spaces.rs`

- [ ] **Step 1: Write failing Space API tests**

Assert creation gives the caller Owner membership, list/get reveal only memberships, slug conflicts return 409, role order is enforced, and repository URLs use the immutable Space UUID.

- [ ] **Step 2: Run Space tests and observe failure**

Run: `cargo test --manifest-path cloud/Cargo.toml --test spaces`
Expected: FAIL because Space handlers are absent.

- [ ] **Step 3: Implement Space and membership queries/handlers**

Create the database record transactionally, initialize the bare repo, compensate the database insert if repo creation fails, return `gitUrl`, `mainRef`, and `userRef`, and use the existing four-role authorization matrix.

- [ ] **Step 4: Run Space tests**

Run: `cargo test --manifest-path cloud/Cargo.toml --test spaces`
Expected: PASS.

- [ ] **Step 5: Commit Space API**

```bash
git add cloud/src/spaces.rs cloud/src/db.rs cloud/src/lib.rs cloud/tests/spaces.rs
git commit -m "feat(cloud): add Git-backed Space membership API"
```

### Task 6: Live-branch pull requests, approvals, and merge

**Files:**
- Create: `cloud/src/pull_requests.rs`
- Modify: `cloud/src/db.rs`
- Modify: `cloud/src/lib.rs`
- Test: `cloud/tests/pull_requests.rs`

- [ ] **Step 1: Write failing PR tests**

Cover one open PR per user branch, later branch pushes updating `head_oid`, approval invalidation on head change, Editor merge denial, Owner/Manager merge permission, stale `expected_head_oid`, non-fast-forward denial, compare-and-swap main advancement, and retry after main already equals the expected head.

- [ ] **Step 2: Run PR tests and observe failure**

Run: `cargo test --manifest-path cloud/Cargo.toml --test pull_requests`
Expected: FAIL because PR endpoints are absent.

- [ ] **Step 3: Implement PR reconciliation and merge**

Use `SELECT ... FOR UPDATE` during merge, reconcile Git before returning every PR, delete approvals when the live head changes, store approvals against the current OID, and update `main` only through `GitRepoStore::fast_forward_main(expected_head_oid)`.

- [ ] **Step 4: Run PR tests**

Run: `cargo test --manifest-path cloud/Cargo.toml --test pull_requests`
Expected: PASS.

- [ ] **Step 5: Commit PR workflow**

```bash
git add cloud/src/pull_requests.rs cloud/src/db.rs cloud/src/lib.rs cloud/tests/pull_requests.rs
git commit -m "feat(cloud): add live-branch pull request workflow"
```

### Task 7: Normalize local main and Agent Change refs

**Files:**
- Modify: `web/src-tauri/src/local_engine.rs`
- Modify: `web/src-tauri/src/local_engine/agent_changes.rs`
- Modify: `web/src/api.ts`
- Test: Rust tests in the same modules
- Test: `web/tests/agent-terminal.test.ts`

- [ ] **Step 1: Add failing local Git tests**

Assert new Spaces start on `main`, a clean imported non-main repository gains and checks out `main`, a dirty conversion is rejected without mutation, existing remotes remain untouched, and background refs are exactly `refs/heads/agent/<change-id>`.

- [ ] **Step 2: Run focused tests and observe failure**

Run: `cargo test --manifest-path web/src-tauri/Cargo.toml local_engine::tests::new_space_uses_main` and `npm --prefix web run test:agent-terminal`
Expected: at least one assertion fails on the legacy branch name.

- [ ] **Step 3: Implement branch normalization**

Set `init.defaultBranch=main` through git2 repository initialization options, create/check out local `main` only under the clean/import rules, change `BRANCH_REF_PREFIX` to `refs/heads/agent/`, and update serialized source-branch labels.

- [ ] **Step 4: Run local tests**

Run: `cargo test --manifest-path web/src-tauri/Cargo.toml local_engine` and `npm --prefix web run test:agent-terminal`
Expected: PASS.

- [ ] **Step 5: Commit local branch semantics**

```bash
git add web/src-tauri/src/local_engine.rs web/src-tauri/src/local_engine/agent_changes.rs web/src/api.ts web/tests/agent-terminal.test.ts
git commit -m "refactor(desktop): make main the local Space draft branch"
```

### Task 8: Desktop Cloud link, sync, submit, and conflict commands

**Files:**
- Create: `web/src-tauri/src/cloud_sync.rs`
- Modify: `web/src-tauri/src/local_engine.rs`
- Modify: `web/src-tauri/src/lib.rs`
- Modify: `web/src/local-api.ts`
- Test: Rust tests in `web/src-tauri/src/cloud_sync.rs`
- Test: `web/tests/cloud-sync-contract.test.ts`

- [ ] **Step 1: Write failing sync state-machine tests**

Use local temporary remotes to assert link preserves `origin`, adds `cowiki`, atomically bootstraps both refs, dirty auto-sync is a no-op, clean sync rebases, submit commits a dirty tree, push targets only `user/<id>`, force-with-lease rejects a concurrent device, conflicts return paths while leaving rebase state, and continue/abort work.

- [ ] **Step 2: Run focused tests and observe failure**

Run: `cargo test --manifest-path web/src-tauri/Cargo.toml cloud_sync` and `node --experimental-strip-types --test web/tests/cloud-sync-contract.test.ts`
Expected: FAIL because the module and commands do not exist.

- [ ] **Step 3: Implement Cloud sync**

Persist only `{space_id, base_url, git_url, user_id}` in local SQLite. Pass Bearer credentials to Git through `GIT_CONFIG_COUNT/GIT_CONFIG_KEY_0/GIT_CONFIG_VALUE_0`, never through remote URLs. Implement explicit command outputs with states `unlinked`, `dirty`, `up_to_date`, `synced`, `conflicted`, `submitted`, and `lease_rejected`.

- [ ] **Step 4: Wire Tauri and TypeScript contracts**

Expose `cloud_link_space`, `cloud_get_status`, `cloud_sync_if_clean`, `cloud_submit`, `cloud_rebase_continue`, and `cloud_rebase_abort`. Keep all rendering and controls out of this PR.

- [ ] **Step 5: Run desktop sync tests**

Run: `cargo test --manifest-path web/src-tauri/Cargo.toml cloud_sync` and `node --experimental-strip-types --test web/tests/cloud-sync-contract.test.ts`
Expected: PASS.

- [ ] **Step 6: Commit desktop sync**

```bash
git add web/src-tauri/src/cloud_sync.rs web/src-tauri/src/local_engine.rs web/src-tauri/src/lib.rs web/src/local-api.ts web/tests/cloud-sync-contract.test.ts
git commit -m "feat(desktop): sync local main through Cloud pull requests"
```

### Task 9: Deployment and real end-to-end verification

**Files:**
- Create: `.env.cloud.example`
- Create: `cloud/Dockerfile`
- Create: `cloud/entrypoint.sh`
- Create: `docker-compose.cloud.yml`
- Create: `scripts/cloud-e2e.sh`
- Create: `docs/cloud-deployment.md`
- Modify: `.github/workflows/ci.yml`
- Test: `cloud/tests/container_contract.sh`

- [ ] **Step 1: Write failing container contract test**

Assert the image runs as non-root, declares a writable repository volume, requires `DATABASE_URL`, has `git http-backend`, and Compose includes PostgreSQL health checks plus Cloud dependency ordering.

- [ ] **Step 2: Run the contract and observe failure**

Run: `bash cloud/tests/container_contract.sh`
Expected: FAIL because deployment files do not exist.

- [ ] **Step 3: Implement deployment artifacts and runbook**

Use a multi-stage Rust build, a non-root runtime user, `tini`, Git, CA certificates, a persistent `/var/lib/cowiki/repos` volume, PostgreSQL 17, startup migration, structured logs, and documented coordinated backup/restore procedures.

- [ ] **Step 4: Implement and run real E2E**

Run: `bash scripts/cloud-e2e.sh`
Expected: PASS after creating PostgreSQL, starting Cloud, authenticating fixture users, bootstrapping a repository, submitting, approving, merging, syncing a second clone, and rejecting a stale merge.

- [ ] **Step 5: Run the complete regression suite**

Run:

```bash
cargo test --manifest-path cloud/Cargo.toml
cargo test --manifest-path web/src-tauri/Cargo.toml
npm --prefix web test
npm --prefix web run build
bash cloud/tests/container_contract.sh
```

Expected: all commands exit 0; frontend build may retain the pre-existing chunk-size warning.

- [ ] **Step 6: Commit operations support**

```bash
git add .env.cloud.example cloud/Dockerfile cloud/entrypoint.sh docker-compose.cloud.yml scripts/cloud-e2e.sh docs/cloud-deployment.md .github/workflows/ci.yml cloud/tests/container_contract.sh
git commit -m "ops(cloud): package PostgreSQL and Git service for production"
```

### Task 10: Final audit and separate pull request

**Files:**
- Modify only files required by audit findings.

- [ ] **Step 1: Review the complete diff against the design**

Run: `git diff --check && git diff --stat origin/feat/review-source-flow...HEAD && git log --oneline origin/feat/review-source-flow..HEAD`
Expected: no whitespace errors, no UI component changes, and focused commits matching Tasks 1-9.

- [ ] **Step 2: Verify security-sensitive invariants**

Search for plaintext token persistence, remote URLs containing credentials, direct main pushes, unchecked repository paths, SQLite usage inside `cloud/`, and missing expected-head comparisons. Fix any match that violates the design and rerun the relevant focused test.

- [ ] **Step 3: Rebase onto the latest integration base**

Run: `git fetch origin && git rebase origin/feat/review-source-flow`
Expected: clean rebase; if #132 has merged to `dev`, rebase onto updated `origin/dev` and target `dev` instead.

- [ ] **Step 4: Re-run the complete regression suite**

Run the five commands from Task 9 Step 5.
Expected: all exit 0.

- [ ] **Step 5: Push and create a separate PR**

```bash
git push -u origin feat/cloud-git-postgres
gh pr create --repo wfnuser/cowiki --base feat/review-source-flow --head feat/cloud-git-postgres --title "feat(cloud): add Git and PostgreSQL collaboration MVP" --body-file /tmp/cowiki-cloud-pr-body.md
```

If #132 is merged before this step, use `--base dev`. The PR body must list architecture, behavior, exclusions, deployment steps, and exact verification results.
