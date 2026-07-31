# Competition Cloud Submissions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver an invite-only, browser-readable Cloud Space with enforced roles, Markdown PR diffs and review/merge, plus a cross-platform local Agent submission command.

**Architecture:** PostgreSQL remains authoritative for identities, memberships, invitations, credentials, PR state, approvals, and audit events; one bare Git repository per Space remains authoritative for content. The browser is a read-and-manage control plane, while a dependency-light Node command performs local OAuth, Git synchronization, branch push, and PR creation for the Agent skill.

**Tech Stack:** Rust 2024, Axum, SQLx/PostgreSQL, bare Git Smart HTTP, React 19, TypeScript 6, Vite 8, Node 20.

---

## File map

- `cloud/migrations/002_space_invitations.sql` — invitation persistence, validity constraints, and indexes.
- `cloud/src/invitations.rs` — invitation preview/create/list/accept/revoke HTTP boundary.
- `cloud/src/db.rs` — transactional invitation and audit persistence.
- `cloud/src/git_repo.rs` — bounded Markdown diff extraction from a bare Space repository.
- `cloud/src/pull_requests.rs` — review authorization and diff endpoint.
- `cloud/src/auth.rs` — shared one-time browser OAuth exchange for web, desktop, and CLI.
- `web/src/cloud/CloudInvitationPage.tsx` — Space-specific invitation acceptance.
- `web/src/cloud/CloudMembersView.tsx` — invitation creation/list/revocation management.
- `web/src/cloud/CloudReviewsView.tsx` — changed-file summary and unified Markdown diff.
- `web/src/cloud/client.ts` — typed APIs for invitations and PR diffs.
- `web/src/App.tsx`, `web/src/pages/LoginPage.tsx`, `web/src/auth-flow.ts` — one-time exchange and post-login return path.
- `tools/cowiki-cli/cowiki.mjs` — cross-platform `login`, `setup`, `clone`, `status`, and `submit`.
- `tools/cowiki-cli/lib/*.mjs` — isolated config, HTTP, OAuth, and Git adapters.
- `skills/cowiki-space/SKILL.md` — Agent contract that invokes the deterministic command.
- `docker-compose.dev.yml`, `.env.cloud.local.example`, `scripts/dev-cloud.sh`, `web/vite.config.ts` — reproducible local Cloud/browser startup.

### Task 1: Repair browser OAuth and generalize the one-time exchange

**Files:**
- Modify: `cloud/src/auth.rs`
- Modify: `cloud/src/db.rs`
- Modify: `cloud/tests/auth.rs`
- Modify: `web/src/auth-flow.ts`
- Modify: `web/src/App.tsx`
- Modify: `web/src/pages/LoginPage.tsx`
- Modify: `web/tests/auth-flow.test.ts`

- [ ] **Step 1: Add failing auth-flow tests**

Test that `#auth_code=cw_once_test` is parsed, exchanged with `POST /api/auth/exchange`, and that a safe `/invite/<token>` return path round-trips while `https://evil.example` is rejected.

- [ ] **Step 2: Run the focused tests and verify failure**

Run: `cd web && npm run test:auth-flow`

Expected: FAIL because web auth codes and safe return paths are not implemented.

- [ ] **Step 3: Add the shared exchange contract**

Expose both `/api/auth/exchange` and the compatibility alias `/api/auth/desktop/exchange`. Accept `client=desktop` and `client=cli` only with an exact `http://127.0.0.1:<port>/auth/callback`; issue labeled API keys through:

```rust
pub async fn exchange_code(
    pool: &PgPool,
    raw_code: &str,
    pepper: &str,
    label: &str,
) -> Result<Option<IssuedApiKey>, sqlx::Error>
```

Keep codes single-use and 60 seconds long.

- [ ] **Step 4: Exchange the browser fragment before routing**

Add:

```ts
export function parseWebOAuthFragment(hash: string): string | null
export function safeAuthReturnPath(value: string | null): string
export async function exchangeOAuthCode(apiBase: string, code: string): Promise<OAuthCredential>
```

`App` must await the exchange, store the returned credential, scrub the fragment, and redirect `/auth/callback` to the saved same-origin path.

- [ ] **Step 5: Run auth and build checks**

Run: `cd cloud && cargo test --test auth`

Expected: PASS.

Run: `cd web && npm run test:auth-flow && npm run build`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add cloud/src/auth.rs cloud/src/db.rs cloud/tests/auth.rs web/src/auth-flow.ts web/src/App.tsx web/src/pages/LoginPage.tsx web/tests/auth-flow.test.ts
git commit -m "fix(auth): complete browser and CLI OAuth exchange"
```

### Task 2: Add Space-scoped invitations with transactional permissions

**Files:**
- Create: `cloud/migrations/002_space_invitations.sql`
- Create: `cloud/src/invitations.rs`
- Create: `cloud/tests/invitations.rs`
- Modify: `cloud/src/lib.rs`
- Modify: `cloud/src/db.rs`
- Modify: `cloud/src/model.rs`

- [ ] **Step 1: Write failing invitation integration tests**

Cover:

```text
Owner/Manager create Editor or Viewer invitation
Editor cannot create or revoke invitation
Owner role cannot be invited
valid token preview does not require authentication
accept creates membership only in the invitation's Space
existing membership is never downgraded
expired/revoked/unknown tokens return 404
accept/create/revoke write audit events
```

- [ ] **Step 2: Run the focused test and verify failure**

Run: `cd cloud && TEST_DATABASE_URL=postgres://cowiki:cowiki@127.0.0.1:55432/postgres cargo test --test invitations`

Expected: FAIL because the migration and routes do not exist.

- [ ] **Step 3: Add the migration**

Create `space_invitations` with UUID identity, `space_id`, `created_by`, non-owner `role`, unique `token_hash`, `expires_at`, nullable `revoked_at`, `accepted_count`, and timestamps. Index active invitations by Space.

- [ ] **Step 4: Add transactional DB operations**

Implement:

```rust
create_space_invitation(...)
list_space_invitations(...)
preview_space_invitation(...)
accept_space_invitation(...)
revoke_space_invitation(...)
```

Lock the invitation row during accept, validate time/revocation, preserve any existing membership role, increment `accepted_count` only for a new member, and write audit rows in the same transaction.

- [ ] **Step 5: Add invitation routes**

Expose:

```text
GET    /api/invitations/:token
POST   /api/invitations/:token/accept
GET    /api/spaces/:space_id/invitations
POST   /api/spaces/:space_id/invitations
DELETE /api/spaces/:space_id/invitations/:invitation_id
```

Return the raw token only from create, build an invite URL from `COWIKI_PUBLIC_ORIGIN`, allow expiry from 1–720 hours, and map invalid/expired/revoked tokens to the same 404 response.

- [ ] **Step 6: Run invitation and existing Space tests**

Run: `cd cloud && TEST_DATABASE_URL=postgres://cowiki:cowiki@127.0.0.1:55432/postgres cargo test --test invitations --test spaces`

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add cloud/migrations/002_space_invitations.sql cloud/src/invitations.rs cloud/tests/invitations.rs cloud/src/lib.rs cloud/src/db.rs cloud/src/model.rs
git commit -m "feat(cloud): add Space invitation lifecycle"
```

### Task 3: Add server-authoritative Markdown PR diffs and reviewer permissions

**Files:**
- Modify: `cloud/src/git_repo.rs`
- Modify: `cloud/src/pull_requests.rs`
- Modify: `cloud/src/db.rs`
- Modify: `cloud/tests/git_repo.rs`
- Modify: `cloud/tests/pull_requests.rs`

- [ ] **Step 1: Add failing diff and authorization tests**

Assert that any member can read the current PR diff, the response is tied to the reconciled `baseOid` and `headOid`, only `.md` changes are rendered, output over 2 MiB is rejected, Editors cannot approve, and Owner/Manager approval succeeds.

- [ ] **Step 2: Run focused tests and verify failure**

Run: `cd cloud && TEST_DATABASE_URL=postgres://cowiki:cowiki@127.0.0.1:55432/postgres cargo test --test git_repo --test pull_requests`

Expected: FAIL on the missing diff endpoint and Editor approval.

- [ ] **Step 3: Add bounded diff extraction**

Implement:

```rust
pub struct PullRequestDiff {
    pub files: Vec<ChangedMarkdownFile>,
    pub patch: String,
}

pub fn markdown_diff(
    &self,
    space_id: Uuid,
    base_oid: &str,
    head_oid: &str,
    max_bytes: usize,
) -> Result<PullRequestDiff, GitRepoError>
```

Run Git directly against the bare repository with fixed OIDs, `--no-ext-diff`, `--no-color`, and Markdown pathspecs; do not invoke a shell.

- [ ] **Step 4: Add the diff endpoint and tighten approval**

Expose `GET /api/spaces/:space_id/pull-requests/:pull_request_id/diff`. Reconcile the live head before diffing and return the exact OIDs in the response. Change approval authorization from `can_push()` to `can_merge()`.

- [ ] **Step 5: Run tests**

Run: `cd cloud && TEST_DATABASE_URL=postgres://cowiki:cowiki@127.0.0.1:55432/postgres cargo test`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add cloud/src/git_repo.rs cloud/src/pull_requests.rs cloud/src/db.rs cloud/tests/git_repo.rs cloud/tests/pull_requests.rs
git commit -m "feat(cloud): expose reviewed Markdown pull request diffs"
```

### Task 4: Add invitation acceptance and browser auth return

**Files:**
- Create: `web/src/cloud/CloudInvitationPage.tsx`
- Modify: `web/src/App.tsx`
- Modify: `web/src/cloud/client.ts`
- Modify: `web/src/pages/LoginPage.tsx`
- Modify: `web/tests/cloud-client.test.ts`
- Modify: `web/tests/cloud-shell.test.ts`

- [ ] **Step 1: Add failing typed-client and shell tests**

Assert exact request paths and bodies for invitation preview/accept and verify the invitation page offers GitHub sign-in when anonymous and opens the invited Space after acceptance.

- [ ] **Step 2: Run tests and verify failure**

Run: `cd web && npm run test:cloud-client && npm run test:cloud-shell`

Expected: FAIL because the invitation APIs and route are missing.

- [ ] **Step 3: Add typed invitation APIs**

Add `CloudInvitationPreview`, `CloudInvitation`, `previewInvitation`, `acceptInvitation`, `listInvitations`, `createInvitation`, and `revokeInvitation`. Keep public preview separate from the bearer-authenticated request helper.

- [ ] **Step 4: Add `/invite/:token`**

Render Space name, granted role, and expiry. A signed-out user stores only the safe current path before GitHub sign-in. A signed-in user can accept; success navigates to `/cloud/spaces/:id/wiki`.

- [ ] **Step 5: Verify**

Run: `cd web && npm run test:auth-flow && npm run test:cloud-client && npm run test:cloud-shell && npm run build`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add web/src/cloud/CloudInvitationPage.tsx web/src/App.tsx web/src/cloud/client.ts web/src/pages/LoginPage.tsx web/tests/cloud-client.test.ts web/tests/cloud-shell.test.ts
git commit -m "feat(web): add invited Space join flow"
```

### Task 5: Add invitation administration to Members

**Files:**
- Modify: `web/src/cloud/CloudMembersView.tsx`
- Modify: `web/tests/cloud-shell.test.ts`

- [ ] **Step 1: Add a failing UI contract test**

Require Owners/Managers to see invitation role, expiry, copy, and revoke controls; require Editor/Viewer to see no invitation management actions.

- [ ] **Step 2: Run and verify failure**

Run: `cd web && npm run test:cloud-shell`

Expected: FAIL on missing invitation administration.

- [ ] **Step 3: Implement invitation management**

Add an “Invite link” panel above the member list. Default to Editor and seven days, show the newly created URL with a copy button, list active invitations, and reload server state after create/revoke.

- [ ] **Step 4: Verify**

Run: `cd web && npm run test:cloud-shell && npm run build`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add web/src/cloud/CloudMembersView.tsx web/tests/cloud-shell.test.ts
git commit -m "feat(web): manage Space invitation links"
```

### Task 6: Render the actual PR diff in browser review

**Files:**
- Modify: `web/src/cloud/client.ts`
- Modify: `web/src/cloud/CloudReviewsView.tsx`
- Modify: `web/tests/cloud-client.test.ts`
- Modify: `web/tests/cloud-shell.test.ts`

- [ ] **Step 1: Add failing diff-client and view tests**

Assert the diff request path, OID types, changed-file counts, patch rendering, refresh on PR selection/head change, and that only Manager/Owner receive approve/merge actions.

- [ ] **Step 2: Run and verify failure**

Run: `cd web && npm run test:cloud-client && npm run test:cloud-shell`

Expected: FAIL because the review view has metadata only.

- [ ] **Step 3: Add diff models and request**

Add:

```ts
interface CloudPullRequestDiff {
  baseOid: string;
  headOid: string;
  files: Array<{ path: string; status: string; additions: number; deletions: number }>;
  patch: string;
}
```

- [ ] **Step 4: Render safe unified diff**

Render patch text as React text nodes, splitting lines only for styling. Never inject HTML. Show changed files and line totals before actions; disable merge when the loaded diff head differs from the selected PR head.

- [ ] **Step 5: Verify**

Run: `cd web && npm run test:cloud-client && npm run test:cloud-shell && npm run build`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add web/src/cloud/client.ts web/src/cloud/CloudReviewsView.tsx web/tests/cloud-client.test.ts web/tests/cloud-shell.test.ts
git commit -m "feat(web): show Markdown diffs in Cloud reviews"
```

### Task 7: Add the cross-platform local command and Agent contract

**Files:**
- Create: `tools/cowiki-cli/package.json`
- Create: `tools/cowiki-cli/cowiki.mjs`
- Create: `tools/cowiki-cli/lib/config.mjs`
- Create: `tools/cowiki-cli/lib/cloud.mjs`
- Create: `tools/cowiki-cli/lib/oauth.mjs`
- Create: `tools/cowiki-cli/lib/git.mjs`
- Create: `tools/cowiki-cli/test/config.test.mjs`
- Create: `tools/cowiki-cli/test/git.test.mjs`
- Create: `tools/cowiki-cli/test/submit.test.mjs`
- Modify: `skills/cowiki-space/SKILL.md`

- [ ] **Step 1: Add failing CLI unit tests**

Use temporary Git repositories and a fake HTTP server to cover config permissions, loopback callback validation, setup remote configuration, Markdown-only commit, rebase conflict stop, force-with-lease push, PR create/update, Viewer rejection, and credential redaction from output.

- [ ] **Step 2: Run and verify failure**

Run: `cd tools/cowiki-cli && npm test`

Expected: FAIL because the command does not exist.

- [ ] **Step 3: Implement login and configuration**

`cowiki login --server <origin>` opens `/api/auth/github?client=cli&callback=<loopback>`, listens only on `127.0.0.1`, exchanges the one-time code, and writes the credential to the platform config directory with user-only permissions.

- [ ] **Step 4: Implement repository setup**

`cowiki setup --server <origin> --space <uuid>` validates membership through `GET /api/spaces/:id`, configures a `cowiki` remote, fetches `main` plus the caller's user branch, and writes non-secret `.cowiki/cloud.json`.

- [ ] **Step 5: Implement safe submission**

`cowiki submit -m <message>` requires local `main`, rejects an active rebase and unsupported dirty files, commits Markdown changes with the Cloud identity, fetches and rebases onto Cloud main, stops on conflict, pushes only `user/<user-id>` with force-with-lease, then creates or updates the open PR.

- [ ] **Step 6: Update the skill**

Keep direct Markdown editing as the Agent behavior, but require `cowiki status` before Cloud work and `cowiki submit -m ...` only after an explicit submit request. Prohibit raw API-key handling and manual reimplementation of Git/Cloud steps.

- [ ] **Step 7: Verify**

Run: `cd tools/cowiki-cli && npm test`

Expected: PASS.

Run: `node tools/cowiki-cli/cowiki.mjs --help`

Expected: lists `login`, `setup`, `status`, and `submit`.

- [ ] **Step 8: Commit**

```bash
git add tools/cowiki-cli skills/cowiki-space/SKILL.md
git commit -m "feat(cli): add Agent-driven Cloud submission command"
```

### Task 8: Make local Cloud and browser startup reproducible

**Files:**
- Create: `.env.cloud.local.example`
- Create: `docker-compose.dev.yml`
- Create: `scripts/dev-cloud.sh`
- Modify: `web/vite.config.ts`
- Modify: `docs/cloud-deployment.md`
- Modify: `.gitignore`

- [ ] **Step 1: Add a local startup contract test**

Extend `cloud/tests/container_contract.sh` to require the local override, browser-origin configuration, and Vite proxies for `/api` and `/git`.

- [ ] **Step 2: Run and verify failure**

Run: `bash cloud/tests/container_contract.sh`

Expected: FAIL on missing local startup files and wrong Vite API port.

- [ ] **Step 3: Add development configuration**

Use PostgreSQL on host port `55432`, Cloud on `8787`, and the browser on `5173`. Set `COWIKI_PUBLIC_ORIGIN=http://localhost:5173`; proxy `/api`, `/git`, and `/healthz` from Vite to Cloud. Keep real GitHub OAuth credentials only in ignored `.env.cloud.local`.

- [ ] **Step 4: Add one-command startup**

`scripts/dev-cloud.sh` validates non-placeholder OAuth credentials, starts PostgreSQL and Cloud with Docker Compose, installs existing web dependencies when needed, starts Vite, checks both health endpoints, and prints `http://localhost:5173/cloud`.

- [ ] **Step 5: Verify local services**

Run: `cp .env.cloud.local.example .env.cloud.local`, fill the existing local OAuth credentials, then run `scripts/dev-cloud.sh`.

Expected: `curl --fail http://localhost:8787/healthz` and `curl --fail http://localhost:5173/healthz` both return `{"status":"ok"}`.

- [ ] **Step 6: Commit**

```bash
git add .env.cloud.local.example docker-compose.dev.yml scripts/dev-cloud.sh web/vite.config.ts docs/cloud-deployment.md .gitignore cloud/tests/container_contract.sh
git commit -m "chore(dev): add local Cloud browser environment"
```

### Task 9: End-to-end competition verification

**Files:**
- Verify: `docs/cloud-deployment.md`

- [ ] **Step 1: Run all server tests**

Run: `cd cloud && TEST_DATABASE_URL=postgres://cowiki:cowiki@127.0.0.1:55432/postgres cargo test`

Expected: PASS with no skipped PostgreSQL assertions.

- [ ] **Step 2: Run all web tests and production build**

Run: `cd web && npm test && npm run lint && npm run build`

Expected: PASS.

- [ ] **Step 3: Run CLI tests**

Run: `cd tools/cowiki-cli && npm test`

Expected: PASS.

- [ ] **Step 4: Run the fresh-user smoke path**

Create Owner, Manager, Editor, Viewer, and outsider fixtures. Verify invite acceptance, browser main read, Editor submit, Manager diff/approve/merge, merged browser content, Viewer submit rejection, outsider 404, invitation revocation, and credential revocation.

- [ ] **Step 5: Inspect the branch**

Run: `git diff --check && git status --short && git log --oneline origin/dev..HEAD`

Expected: no whitespace errors, no uncommitted files, and focused commits for each completed subsystem.
