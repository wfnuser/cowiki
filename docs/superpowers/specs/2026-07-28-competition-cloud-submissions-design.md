# Competition Cloud Submissions

## Goal

Give every competition participant a reliable way to read a shared Space and submit local work without installing the CoWiki desktop client. Keep editing local-first while making identity, permissions, membership, review, and merge authoritative in CoWiki Cloud.

## Product boundary

- The browser is the collaboration control plane: sign-in, invited-space access, read-only Space browsing, member management, pull-request diff/review, approval, and merge.
- Local files remain the editing surface. Browser editing is out of scope.
- Participants install Git, a supported Agent CLI, the CoWiki Space skill, and a small cross-platform CoWiki command.
- The Agent calls the command; it does not reproduce credential, Git, rebase, or Cloud API logic itself.

## Joining a Space

An invitation belongs to exactly one Space. An Owner or Manager creates an invitation link with a role, expiry, and revocable token. Competition invitations default to `Editor`.

Opening the link requires GitHub sign-in. Accepting it creates or updates membership only for that Space, then opens its browser view. Invitations are not public discovery links and cannot grant `Owner`.

## Roles

| Action | Owner | Manager | Editor | Viewer |
| --- | --- | --- | --- | --- |
| Read Cloud `main` | Yes | Yes | Yes | Yes |
| Submit own branch / PR | Yes | Yes | Yes | No |
| Review and approve PR | Yes | Yes | No | No |
| Merge PR | Yes | Yes | No | No |
| Create/revoke invitations | Yes | Yes | No | No |
| Manage non-owner members | Yes | Yes | No | No |
| Bootstrap the first Cloud revision | Yes | No | No | No |

Every REST endpoint and Git operation enforces the same server-side role checks. Browser visibility is not treated as authorization.

## Storage

PostgreSQL is authoritative for users, Spaces, memberships, invitations, API credentials, pull requests, head-specific approvals, and audit events. Invitation tokens and API credentials are stored only as hashes.

Each Space has one server-side bare Git repository. Git is authoritative for Markdown content and history. The browser reads the accepted `main` branch; participants push only to their own `user/<user-id>` branch.

Database changes that accompany Git operations use explicit states and reconciliation so an interrupted request cannot silently report an unmerged commit as merged.

## Participant flow

1. Open the Space invitation and sign in with GitHub.
2. Accept the invitation and read the Space in the browser.
3. Run CoWiki setup locally. The command opens the system browser and receives a short-lived, one-time exchange result; long-lived credentials are not copied from browser storage.
4. The Agent edits the local Markdown repository.
5. `cowiki submit` validates membership, commits eligible changes, fetches Cloud `main`, rebases safely, pushes the caller's user branch, and creates or updates its open PR.
6. A conflict stops before push and gives a recoverable local error.

## Administrator flow

Any signed-in user can create a shared Space and becomes its Owner. The Owner explicitly publishes the first revision from a clean local repository; the bootstrap atomically creates Cloud `main` and the Owner branch. Owners and Managers can then see members, issue or revoke invitations, change non-owner roles, and remove non-owner members. The review screen shows the actual Markdown diff, changed files, submitter, current head, approval state, and merge readiness.

Approval is tied to the exact PR head and is invalidated by a new push. Merge requires the expected head, an open PR, a mergeable branch based on current Cloud `main`, and an authorized reviewer. All management, approval, and merge actions produce audit events.

## Failure and security handling

- Invalid, expired, or revoked invitations reveal no private Space content.
- Non-members receive the same not-found-style response for private Space resources.
- Removed or downgraded members lose access immediately on the next request.
- Git rejects direct pushes to `main`, pushes to another user's branch, and Viewer pushes.
- Authentication exchange codes are single-use and short-lived; API credentials can be revoked.
- Rebase conflicts and stale PR heads never trigger a partial merge.

## Competition acceptance test

A fresh Windows participant can install the prerequisites, join one invited Space, read it in the browser, authenticate the local tool, submit a Markdown change, and see the resulting PR. A Manager can inspect its diff, approve it, merge it, and see the new content on Cloud `main`. A Viewer cannot submit, an outsider cannot read, and revoked invitations and credentials stop working.

## Deferred

Browser editing, public self-join, desktop-client distribution, comments, ownership transfer, Space deletion, advanced branch management, and organization-wide administration are not required for the competition path.
