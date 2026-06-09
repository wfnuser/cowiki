# Draft — Epic G: Test organization

> Status: **draft for review** (codex). Lowest risk; relates to #17 (test infrastructure).

## Problem
`crates/db/src/workspaces.rs` is ~1287 lines, most of it inline `#[cfg(test)]` DB-integration tests (role/member/invitation/delete) sharing a hand-rolled `test_pool()`. The same `test_pool` boilerplate (and its migration list) is copy-pasted into `submissions.rs`, `pages.rs` (added by #66) — every new migration must be registered in all of them.

## Target shape
1. **Move DB-integration tests out of `src/`** into `crates/db/tests/` (or `crates/server/tests/`, where `permission_api_tests.rs` already lives), grouped by area:
   ```
   crates/db/tests/
     common/mod.rs        // ONE shared test_pool() + seed helpers (single migration list)
     workspaces_members.rs
     workspaces_invitations.rs
     workspaces_crud.rs
     submissions.rs
     pages.rs
   ```
   Keep pure unit tests (e.g. `Role` parsing) inline in `src/`.
2. **Single migration list.** `common::test_pool()` is the only place migrations are registered for tests — delete the three duplicated lists. Better: have `test_pool` call the real `cowiki_db::run_migrations` so test schema == prod schema by construction (no separate list to drift).

## Open decisions for review
1. **`test_pool` per-test isolation.** Today tests share one DB and lean on unique UUIDs / cleanup. Options: (a) keep shared DB + unique data; (b) one schema/transaction-rollback per test; (c) `sqlx::test` macro (needs `DATABASE_URL` + offline data). Proposal: (b) wrap each test in a transaction that rolls back — cheap isolation, no cross-test bleed. (This also fixes the invitation-test flakiness seen earlier.)
2. **Use `run_migrations` directly** in `test_pool` (eliminates the duplicated lists) — any reason not to? It already takes `embedding_dim`.
3. Scope vs #17 (test infra epic): is this the right first slice, or fold into #17? Proposal: do the mechanical move now; defer the harness/E2E parts to #17.

## Notes
- Low conflict risk (test-only), but touches the shared `test_pool` that #61/#66 modified — coordinate the single-list consolidation with those landing.
- The `create_test_users` seeding bug (missing `api_key`, reused unique names) was already fixed in an earlier PR; the consolidated `common` helper should carry that fix.
