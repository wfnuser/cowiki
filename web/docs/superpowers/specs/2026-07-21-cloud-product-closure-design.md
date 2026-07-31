# Cloud Space Product Closure Design

## Goal

Turn the existing PostgreSQL + bare-Git Cloud backend into a usable collaboration loop after a Cloud session already exists. OAuth and the desktop browser-return login flow are explicitly excluded and will be delivered in a separate pull request.

The first complete loop is:

1. A signed-in desktop user publishes an existing local Space.
2. CoWiki creates a Cloud Space and initializes Cloud `main` plus `user/<user-id>` from local `main`.
3. Members can browse Cloud `main` read-only in the browser.
4. Owners and Managers can manage membership and merge pull requests.
5. Editors can publish their own work through a pull request; Viewers remain read-only.

## Scope

### Included

- A replaceable `CloudSession` boundary containing the Cloud origin, API key, user id, and display name.
- A development-only way for tests to inject a Cloud session without implementing login UI.
- Desktop Publish, Sync, and Submit actions for local Spaces.
- Automatic commit during Submit after the user supplies or confirms a commit message.
- Browser Cloud Space list and read-only Cloud `main` browsing.
- Browser pull-request list/detail and merge action.
- Browser member list, add/update role, and remove actions.
- Shared visual language and reusable existing layout/view components where their contracts fit.
- PostgreSQL authorization and bare-Git content remain authoritative.

### Deferred

- OAuth callback completion and desktop login error handling.
- User-facing rebase continuation or abort controls.
- Rich conflict resolution UI.
- Browser editing.
- Public anonymous sharing.
- Ownership transfer.
- Approval policy configuration and comment infrastructure.

If automatic synchronization encounters a Git conflict, the first version stops safely, reports that the local Space needs manual attention, and does not push or create/update a pull request.

## Architecture

### Session boundary

Cloud-capable code consumes this interface and does not know how authentication happened:

```ts
export interface CloudSession {
  baseUrl: string;
  apiKey: string;
  userId: string;
  userName: string;
}
```

Production eventually receives it from the browser-return OAuth PR. Contract tests and local development may inject it through an explicit test bootstrap. No Cloud credential is written to a Git remote, repository config, or Space metadata.

### Cloud API client

Create one typed client for the new `/api/spaces` domain. It owns response mapping and authorization headers for:

- current user;
- Space list/detail;
- read-only Git tree and Markdown content;
- members;
- pull requests, approvals, and merge.

The legacy `/api/workspaces` API remains separate. New Cloud UI must not extend or emulate the legacy server contract.

### Browser shell

The hosted browser experience uses a focused Cloud shell with the same design tokens and shared presentational building blocks as desktop. It is not a copy of the existing state-heavy desktop `MainLayout`.

- Home lists Cloud Spaces available to the current member.
- A Space opens on the Cloud `main` Wiki.
- Space navigation exposes Wiki, Reviews, and Members according to role.
- Wiki content is read-only.
- Reviews follow the live `user/<id>` branch head.
- Members is editable only for Owner and Manager.

This separation keeps desktop filesystem behavior out of browser code while preserving a nearly identical visual system.

### Desktop Cloud panel

Desktop adds one Space-scoped Cloud panel/dialog.

For an unlinked Space it shows `Publish to Cloud`. Publishing asks for a Cloud name and slug, then invokes the existing `cloud_link_space` command. The command creates the Cloud Space, creates an automatic initial commit only when required, configures the credential-free `cowiki` remote, and atomically initializes Cloud `main` and the user's branch.

For a linked Space it shows a compact status and only the primary actions required for the first loop:

- `Sync from Cloud` when the worktree is clean;
- `Submit changes` when the Draft is dirty or ahead;
- a link to open the Cloud Space in the browser.

Submit asks for one commit message, commits eligible local changes, fetches and automatically rebases on Cloud `main`, pushes local `main` to `user/<user-id>`, and creates or updates the live pull request. A conflict stops the operation with no push.

## Cloud read API

Add authenticated, membership-scoped endpoints:

```text
GET /api/spaces/:space_id/tree?ref=main
GET /api/spaces/:space_id/content?ref=main&path=<repo-relative-markdown-path>
```

Rules:

- The first version permits only `main` for browser content reads.
- Paths must be normalized repository-relative paths with no traversal, NUL, `.git`, or hidden runtime metadata.
- Tree results include visible Markdown pages and folders only.
- Content reads return UTF-8 Markdown plus the resolved commit oid.
- Missing paths return 404; binary or invalid UTF-8 content returns 415.
- Every caller must be a Space member.

The API reads directly from the bare Git repository. It does not materialize a worktree or create a second snapshot store.

## Permissions

The existing role model remains authoritative:

| Capability | Owner | Manager | Editor | Viewer |
|---|---:|---:|---:|---:|
| Read Cloud `main` | Yes | Yes | Yes | Yes |
| Fetch Git | Yes | Yes | Yes | Yes |
| Push own `user/<id>` | Yes | Yes | Yes | No |
| Create/update PR | Yes | Yes | Yes | No |
| Approve PR | Yes | Yes | Yes | No |
| Merge PR | Yes | Yes | No | No |
| Add/change/remove non-owner members | Yes | Yes | No | No |

Owner membership cannot be removed or changed. Adding a member requires that the target GitHub handle has signed in at least once. A Manager cannot create a second Owner.

## Data flow

### Publish

```text
Desktop local main
  -> POST /api/spaces
  -> authenticated Git Smart HTTP atomic push
       main -> refs/heads/main
       main -> refs/heads/user/<user-id>
  -> save non-secret Cloud link metadata locally
  -> browser Space URL becomes available
```

If any initialization step fails, local history is unchanged. The server keeps an empty repository safe for retry; the desktop saves the link only after the remote initialization succeeds.

### Submit

```text
dirty local main
  -> automatic commit with confirmed message
  -> fetch cowiki/main
  -> rebase local main onto cowiki/main
  -> conflict: stop, do not push
  -> push with force-with-lease semantics to user/<user-id>
  -> POST /api/spaces/:id/pull-requests
  -> open PR follows subsequent pushes to the same user branch
```

### Merge

```text
Owner/Manager opens PR
  -> server reconciles current live user-branch head
  -> client sends expected head oid
  -> server compare-and-swap merges into main
  -> stale head returns 409 and client refreshes
```

## Error handling

- Authentication failures clear no local data and direct the user to sign in when the separate login PR is present.
- Authorization failures explain the missing role without exposing Space existence to non-members.
- Dirty worktrees are never auto-synced in the background.
- Submit may auto-commit only after explicit user confirmation of the message.
- Git conflicts never trigger an automatic push or PR update.
- Stale user-branch leases and stale PR heads require a refresh instead of overwriting remote work.
- Member management uses optimistic disabled states but reloads the server-authoritative list after every mutation.

## Testing

- Rust unit tests for tree traversal, content reads, path rejection, membership authorization, and role restrictions.
- Cloud integration tests against PostgreSQL and real bare Git repositories.
- Frontend contract tests for the Cloud session boundary, route mapping, role-gated actions, and desktop publish/submit state transitions.
- Production builds for web and Tauri.
- A two-user end-to-end script that publishes a Space, adds a Manager, submits an Editor branch, merges as Manager, and verifies a fresh clone of Cloud `main`.

## Acceptance criteria

- With an injected session, a user can publish an existing local Space without editing Git configuration manually.
- Cloud stores control-plane data in PostgreSQL and Markdown history in the Space's bare Git repository.
- Any member can open the browser Space and read Cloud `main`.
- Owner/Manager can manage non-owner members.
- Owner/Manager can merge an open PR; Editor and Viewer cannot.
- A local dirty Draft can be committed, rebased, pushed, and submitted from the desktop Cloud panel.
- A rebase conflict stops before push and does not require a first-version Continue Rebase UI.
- No OAuth/login implementation is included in this change.
