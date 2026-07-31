# Cloud Space Product Closure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make an existing local Space publishable to the PostgreSQL + bare-Git Cloud, then expose authenticated read-only Wiki, member management, and pull-request merge flows in a browser without implementing OAuth.

**Architecture:** Keep OAuth behind a replaceable `CloudSession` boundary. Extend the Cloud service with membership-scoped bare-Git read endpoints, add a typed Cloud web client and focused browser shell, and connect the existing Tauri Cloud commands to a Space-scoped desktop dialog. PostgreSQL remains the control plane and bare Git remains the content source of truth.

**Tech Stack:** Rust/Axum/SQLx/PostgreSQL/git2, React/TypeScript/Vite/Tauri, Node contract tests, Docker-based end-to-end tests.

---

### Task 1: Cloud main read API

**Files:**
- Modify: `cloud/src/git_repo.rs`
- Create: `cloud/src/content.rs`
- Modify: `cloud/src/lib.rs`
- Create: `cloud/tests/content.rs`

- [ ] **Step 1: Write failing membership and content tests**

Create tests that initialize a real bare Space repository, publish `index.md` and a nested Markdown page, then assert:

```rust
assert_eq!(tree.status(), StatusCode::OK);
assert_eq!(content.status(), StatusCode::OK);
assert_eq!(non_member.status(), StatusCode::NOT_FOUND);
assert_eq!(traversal.status(), StatusCode::BAD_REQUEST);
assert_eq!(binary.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
```

- [ ] **Step 2: Run the focused test and verify RED**

Run: `cargo test --manifest-path cloud/Cargo.toml --test content`

Expected: compilation or route-not-found failures because `content::routes` does not exist.

- [ ] **Step 3: Implement repository tree/content reads**

Add focused `RepositoryStore` methods that resolve `refs/heads/main`, walk visible Markdown blobs without a worktree, normalize repository-relative paths, and return the resolved oid plus bytes.

- [ ] **Step 4: Implement authenticated routes**

Expose:

```text
GET /api/spaces/{space_id}/tree?ref=main
GET /api/spaces/{space_id}/content?ref=main&path=index.md
```

Require membership through the existing not-found-preserving membership guard. Reject every ref except `main`.

- [ ] **Step 5: Verify and commit**

Run:

```bash
cargo test --manifest-path cloud/Cargo.toml --test content
cargo test --manifest-path cloud/Cargo.toml
cargo fmt --manifest-path cloud/Cargo.toml -- --check
```

Commit: `feat(cloud): expose authenticated main content`

### Task 2: Typed Cloud client and injected session

**Files:**
- Create: `web/src/cloud/session.ts`
- Create: `web/src/cloud/client.ts`
- Create: `web/src/cloud/routes.ts`
- Create: `web/tests/cloud-client.test.ts`
- Modify: `web/package.json`

- [ ] **Step 1: Write failing client contract tests**

Assert that an injected session is normalized, every request uses `Authorization: Bearer`, UUID Space routes round-trip, and role helpers produce the existing matrix:

```ts
assert.equal(canManageMembers('manager'), true);
assert.equal(canMerge('editor'), false);
assert.equal(canPush('viewer'), false);
```

- [ ] **Step 2: Run and verify RED**

Run: `node --experimental-strip-types --test web/tests/cloud-client.test.ts`

Expected: module-not-found for `src/cloud/client.ts`.

- [ ] **Step 3: Implement the session boundary and client**

Use:

```ts
export interface CloudSession {
  baseUrl: string;
  apiKey: string;
  userId: string;
  userName: string;
}
```

Support current user, Space list/detail, tree/content, members, PR list/detail/approve/merge. Keep session injection explicit and outside production OAuth behavior.

- [ ] **Step 4: Verify and commit**

Run the focused test and `npm --prefix web run build`.

Commit: `feat(web): add typed Cloud session client`

### Task 3: Browser Cloud shell

**Files:**
- Create: `web/src/cloud/CloudApp.tsx`
- Create: `web/src/cloud/CloudHome.tsx`
- Create: `web/src/cloud/CloudSpaceView.tsx`
- Create: `web/src/cloud/CloudWikiView.tsx`
- Create: `web/src/cloud/CloudReviewsView.tsx`
- Create: `web/src/cloud/CloudMembersView.tsx`
- Modify: `web/src/App.tsx`
- Create: `web/tests/cloud-shell.test.ts`
- Modify: `web/package.json`

- [ ] **Step 1: Write failing route and role-gating tests**

Verify the browser shell consumes `/api/spaces`, exposes Wiki/Reviews/Members, renders Markdown read-only, and hides management/merge actions from Editor and Viewer.

- [ ] **Step 2: Run and verify RED**

Run: `node --experimental-strip-types --test web/tests/cloud-shell.test.ts`

- [ ] **Step 3: Implement the shared visual shell**

Reuse CoWiki design tokens, typography, avatar, dropdown, dialog, and Markdown rendering components. Do not import Tauri APIs or duplicate desktop filesystem state.

- [ ] **Step 4: Implement member and PR mutations**

Reload server-authoritative data after add/change/remove, approval, or merge. A stale expected head refreshes the PR instead of retrying automatically.

- [ ] **Step 5: Verify and commit**

Run focused tests, full web tests, and production build.

Commit: `feat(web): add read-only Cloud Space shell`

### Task 4: Desktop publish and submit panel

**Files:**
- Create: `web/src/components/cloud/CloudSpaceDialog.tsx`
- Create: `web/src/components/cloud/cloud-space-model.ts`
- Modify: `web/src/pages/MainLayout.tsx`
- Modify: `web/src/local-api.ts`
- Create: `web/tests/cloud-space-dialog.test.ts`
- Modify: `web/package.json`

- [ ] **Step 1: Write failing state-model tests**

Cover unlinked publish, linked clean sync, dirty submit, submitted PR link, and conflicted safe-stop states. Assert that no Continue/Abort rebase wording is rendered.

- [ ] **Step 2: Run and verify RED**

Run: `node --experimental-strip-types --test web/tests/cloud-space-dialog.test.ts`

- [ ] **Step 3: Implement the dialog model and UI**

Use the existing Tauri commands. Publish requests name/slug; Submit requires a confirmed commit message. A conflict shows a concise safe-stop message and no recovery action in this version.

- [ ] **Step 4: Wire the Space-scoped entry**

Show `Publish to Cloud` for unlinked Spaces and a quiet Cloud status/action entry for linked Spaces. Obtain `CloudSession` through the injected session boundary; do not add OAuth behavior or a production API-key form.

- [ ] **Step 5: Verify and commit**

Run focused tests, full web tests, build, and Tauri check.

Commit: `feat(desktop): publish and submit Cloud Spaces`

### Task 5: Permission and workflow end-to-end verification

**Files:**
- Modify: `scripts/cloud-e2e.sh`
- Modify: `.github/workflows/ci.yml`
- Modify: `docs/cloud-deployment.md`

- [ ] **Step 1: Extend the E2E assertions before implementation**

Add a Viewer and Editor, then assert Viewer push/PR/merge denial, Editor PR success/merge denial, Manager member management, Manager merge, content API visibility, and fresh-clone equality.

- [ ] **Step 2: Run and verify the new assertion fails where coverage is missing**

Run: `./scripts/cloud-e2e.sh`

- [ ] **Step 3: Complete only the missing permission/workflow behavior**

Keep the existing role model and not-found behavior. Do not introduce approval policy or ownership transfer.

- [ ] **Step 4: Run full verification**

```bash
cargo test --manifest-path cloud/Cargo.toml
cargo clippy --manifest-path cloud/Cargo.toml --all-targets -- -D warnings
cargo check --manifest-path web/src-tauri/Cargo.toml
npm --prefix web test
npm --prefix web run build
./scripts/cloud-e2e.sh
git diff --check origin/feat/review-source-flow...HEAD
```

- [ ] **Step 5: Document and commit**

Document local session injection for testing, publish flow, role matrix, storage, and backup requirements.

Commit: `test(cloud): verify publish and permission workflow`

### Task 6: PR review closure

**Files:**
- Modify only files required by actionable PR #132/#135 review findings that are within this spec.

- [ ] **Step 1: Classify every review comment**

Record each as implemented here, deferred to login PR, already resolved, or unrelated.

- [ ] **Step 2: Add a failing regression test for every in-scope defect**

Run each focused test and verify RED before changing production code.

- [ ] **Step 3: Implement and verify fixes**

Run the focused tests and the full verification matrix.

- [ ] **Step 4: Push and update PR #135**

Summarize the product closure, deferred login boundary, verification evidence, and any remaining explicit limitations.
