# Cloud Git, PostgreSQL, and Pull Request Design

Date: 2026-07-21
Status: Approved for implementation
Base: `origin/feat/review-source-flow` at `b821fda` (includes PR #134)

## Outcome

CoWiki Cloud becomes a collaboration control plane around ordinary Git repositories. Each Cloud Space owns one bare Git repository. PostgreSQL stores users, sessions, memberships, Space metadata, pull requests, approvals, and audit records. Markdown and Git history remain the source of truth for knowledge.

The desktop keeps a normal editable local `main`. A Cloud remote named `cowiki` exposes the shared `cowiki/main` and the signed-in user's `cowiki/user/<user-id>`. Submitting a local draft rebases it onto the latest Cloud `main`, pushes it to `user/<user-id>`, and creates or updates that branch's pull request. Only an explicit merge operation can advance Cloud `main`.

## Scope

This production MVP includes:

- PostgreSQL-backed Cloud identity, sessions, Space metadata, memberships, pull requests, approvals, and audit events.
- One persistent bare Git repository per Space.
- Authenticated Git Smart HTTP fetch and push.
- A protected Cloud `main` and one `user/<id>` branch per contributor.
- First-link bootstrap from the current local `main`.
- Desktop commands for link, status, clean sync, submit, rebase continue, and rebase abort.
- Visible structured conflict results for a later UI to render.
- Container configuration, migrations, health checks, and separate database/repository backup guidance.

This PR intentionally excludes:

- Browser Cloud browsing and editing UI.
- Document comments, PR comments, and `@agent` comment execution.
- Multiple open pull requests from the same user branch.
- Cloud-side content extraction or an additional snapshot store.
- Direct pushes to Cloud `main`.

## Repository model

### Local repository

- CoWiki creates new repositories with `main` as the initial branch.
- Existing repositories keep their history and existing remotes.
- If an imported repository has `main`, CoWiki uses it.
- If it has no `main`, a clean attached `HEAD` is copied to a new local `main` and checked out.
- A detached or dirty imported repository that needs branch conversion is rejected with a recoverable message rather than being rewritten silently.
- Humans and live Agents edit local `main`.
- Background Agent Changes use `agent/<change-id>` and merge back to local `main`.
- Existing `origin` configuration is never changed.

### Cloud repository

Each Space has a bare repository at `<repo-root>/<space-id>.git` with:

- `refs/heads/main`: shared accepted knowledge.
- `refs/heads/user/<user-id>`: the user's live submitted head.

The desktop adds a remote named `cowiki` and configures fetch mappings for both branches. Remote-tracking refs are read-only views; local edits never happen on `cowiki/main`.

### First link

The client creates a Cloud Space, then performs one atomic push in which the current local `main` initializes both Cloud `main` and `user/<owner-id>` at the same OID. The server permits this only when the Cloud repository has no `main`, the caller is the owner, the receive contains exactly those two branch creations, and both refs point at the same commit. No pull request is created for bootstrap.

## Sync and submit state machine

### Automatic clean sync

When authenticated Cloud metadata is available and local `main` is clean:

1. Fetch `cowiki/main` and `cowiki/user/<id>`.
2. Rebase local `main` onto `cowiki/main` when local `main` is not already based on it.
3. Report `synced` or `up_to_date`.
4. If rebase conflicts, leave Git's rebase state intact and return the conflicting paths plus continue/abort actions.

Dirty working trees are never stashed automatically. Automatic sync returns `dirty` without changing files.

### Submit

1. Validate that no merge/rebase is already in progress.
2. If the working tree is dirty, stage all repository changes and commit using the user-confirmed message and signed-in identity.
3. Fetch Cloud refs.
4. Rebase local `main` onto the latest `cowiki/main`.
5. On conflict, stop before push and return the conflicting paths.
6. Push local `main` to `refs/heads/user/<id>` using a force-with-lease against the fetched user ref. Rebase can rewrite commits, so a plain fast-forward push is insufficient; the lease prevents overwriting a newer device's push.
7. Create or update the one open pull request for that Space and head branch.

The pull request follows the branch. Later pushes update its displayed head rather than opening another pull request.

## Pull request rules

- Head is always `user/<id>` and base is always `main`.
- PostgreSQL enforces at most one open pull request per `(space_id, head_ref)`.
- API reads reconcile the stored `head_oid` with the Git ref.
- A head change deletes existing approvals and records the new OID.
- Approval records include the exact approved head OID.
- Merge accepts `expected_head_oid`, locks the PR row, reconciles the live head, and rejects stale callers.
- Merge is allowed only when `main` is an ancestor of the head.
- The server advances `main` using compare-and-swap `git update-ref <new> <old>`; it never creates a merge commit in this MVP.
- If Cloud `main` advanced, the merge response tells the client to fetch/rebase/push again.
- A successful merge closes the PR and records the resulting main OID.

## Roles and authorization

The existing role names remain authoritative:

| Role | Read/fetch | Push own user branch | Create/update PR | Approve | Merge/manage members |
| --- | --- | --- | --- | --- | --- |
| Owner | Yes | Yes | Yes | Yes | Yes |
| Manager | Yes | Yes | Yes | Yes | Yes |
| Editor | Yes | Yes | Yes | Yes | No |
| Viewer | Yes | No | No | No | No |

No role may push directly to `main`. Owner and Manager advance it through the merge API so stale-head and audit checks cannot be bypassed.

## Authentication

- Browser OAuth produces a short-lived one-time desktop exchange code.
- The desktop loopback callback exchanges that code for a revocable API key and user identity, matching the existing desktop contract.
- REST requests use `Authorization: Bearer <api-key>`.
- Git Smart HTTP accepts the same Bearer credential. Desktop Git subprocesses receive it through process environment-backed Git configuration rather than embedding it in the remote URL or repository config.
- API keys are hashed at rest; plaintext values are returned once.
- Redirect targets are allowlisted and one-time codes expire quickly.

## PostgreSQL control plane

The initial migration creates:

- `users`
- `oauth_states`
- `desktop_exchange_codes`
- `api_keys`
- `spaces`
- `space_members`
- `pull_requests`
- `pull_request_approvals`
- `audit_events`

Foreign keys cascade only for subordinate records. Space deletion is not exposed by this MVP. Slugs are globally unique for simple routing, while the Git repository path is derived only from the UUID and never from user input.

## Git Smart HTTP boundary

Axum authenticates and authorizes every Git route, then adapts the request to `git http-backend`. Upload-pack requires Viewer or higher. Receive-pack requires Editor or higher. A repository `pre-receive` hook inherits the authenticated user and role from the request process and enforces:

- normal pushes may change only `refs/heads/user/<authenticated-user-id>`;
- deleting the user branch is rejected while it owns an open pull request;
- direct `main`, tag, note, and arbitrary ref updates are rejected;
- the one tightly constrained owner bootstrap transaction is permitted.

The hook is defense in depth; request routing also rejects unauthorized receive-pack before starting Git.

## Consistency and concurrency

- Per-Space server locks serialize bootstrap and merge ref updates inside one process.
- Git compare-and-swap protects refs across processes.
- PostgreSQL row locks serialize PR merge state.
- A retry after Git advanced but before PostgreSQL committed detects that `main == expected_head_oid` and completes the PR record idempotently.
- Desktop mutation uses the existing LocalEngine mutation lock.
- Force-with-lease protects a user's Cloud branch against concurrent devices.

## Storage and deployment

The Cloud container mounts a persistent repository volume and connects to PostgreSQL using `DATABASE_URL`. It runs migrations at startup and fails closed if the repository root is absent or not writable. PostgreSQL and repository storage are backed up independently; a restore is valid only when both are restored to a mutually consistent checkpoint. The first deployment runs one application replica; horizontal replicas require a distributed per-Space lock or routing affinity in addition to Git compare-and-swap.

## API surface

REST endpoints:

- `GET /healthz`
- `GET /api/auth/github/start`
- `GET /api/auth/github/callback`
- `POST /api/auth/desktop/exchange`
- `GET /api/me`
- `POST /api/spaces`
- `GET /api/spaces`
- `GET /api/spaces/:space_id`
- `POST /api/spaces/:space_id/pull-requests`
- `GET /api/spaces/:space_id/pull-requests`
- `GET /api/spaces/:space_id/pull-requests/:pr_id`
- `POST /api/spaces/:space_id/pull-requests/:pr_id/approve`
- `POST /api/spaces/:space_id/pull-requests/:pr_id/merge`
- Git Smart HTTP under `/git/:space_id.git/*path`

Desktop Tauri commands:

- `cloud_link_space`
- `cloud_get_status`
- `cloud_sync_if_clean`
- `cloud_submit`
- `cloud_rebase_continue`
- `cloud_rebase_abort`

## Verification

- Unit tests cover roles, ref validation, API-key hashing, Git protocol parsing, local branch normalization, conflict discovery, and status modeling.
- PostgreSQL integration tests run against a real disposable PostgreSQL service and execute migrations.
- End-to-end tests create two local clones, bootstrap a Space, submit, approve, merge, sync, and exercise stale-head and conflict behavior.
- Existing frontend, Tauri, and Agent Change tests remain green, including the PR #134 version switcher contract.
