# Draft — Epic D: Submission status enum + status presentation

> Status: **draft for review** (codex). One Epic, layered: backend semantic layer, then frontend presentation layer.

## Problem
Submission status strings (`pending` / `approved` / `rejected` / `merged`, and the planned `changes_requested`) are scattered as raw literals across DB queries, `review.rs`, and the frontend, with no single source of truth. The frontend `statusBadge` map is duplicated in `ReviewList.tsx` and `ReviewDetail.tsx` and has already drifted (`'Changes req.'` vs `'Changes requested'`).

Current literal sites:
- backend: `review.rs:93/96/145/148` (`"approved"`/`"merged"`/`"rejected"`), `submissions.rs list_pending_for_workspace` `IN ('pending','approved','rejected','merged')`.
- frontend: `ReviewList.tsx` (`statusBadge`, filter logic `s.status === '...'`), `ReviewDetail.tsx` (`statusBadge`, `decided = status !== 'pending'`).

## D1 — Backend semantic layer
Add a `SubmissionStatus` enum (in `crates/db/src/submissions.rs` or a small `status.rs`):
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubmissionStatus { Pending, Approved, Rejected, Merged /*, ChangesRequested */ }

impl SubmissionStatus {
    pub fn as_str(&self) -> &'static str { /* "pending" | ... */ }
    pub const ACTIVE: &[SubmissionStatus] = &[Self::Pending, Self::Approved, Self::Rejected, Self::Merged];
}
impl std::str::FromStr for SubmissionStatus { /* ... */ }
```
- `update_status` takes `SubmissionStatus` instead of `&str`; `review.rs` passes the enum (no more `"approved"` literals).
- `list_pending_for_workspace`'s `IN (...)` is built from `SubmissionStatus::ACTIVE` (or a const string), so adding a variant updates the query.
- Keep `Submission.status: String` at the row level (DB column is text) but expose a typed accessor; OR change the field to the enum with `sqlx` `#[sqlx(try_from = "String")]`. **(decision below)**

## D2 — Frontend presentation layer
One module `web/src/lib/review.ts`:
```ts
export type ReviewStatus = 'pending' | 'approved' | 'rejected' | 'merged';
export const statusBadge: Record<ReviewStatus, { token: keyof Tokens; label: string }> = { ... };
```
- Fold the badge **colors** into `web/src/lib/design.ts` tokens (no local `C.amberSoft` literals re-mapped per component) — this is also Epic #10's design-token consolidation.
- `ReviewList` + `ReviewDetail` import the single `statusBadge`; resolve the label drift to **`'Changes requested'`** (proposed).

## Open decisions for review
1. **`Submission.status` typing.** Keep `String` + helper, or make it the enum end-to-end (stricter, but `sqlx` mapping + any unknown DB value becomes a hard error)? Proposal: enum end-to-end with an explicit `Unknown` fallback variant so a bad row doesn't 500.
2. **Include `changes_requested` now** (it's in the review-workflow design #10/#19) or only the 4 live ones? Proposal: define the variant now, don't wire the flow (that's #10).
3. **Canonical labels/colors** — confirm `pending → "Review needed"`, `rejected → "Changes requested"`, etc., and which design token each maps to.

## Notes
- Touches `review.rs` (conflicts with #64/#66 — keep all) and the two review components (conflicts with #67's `timeAgo` extraction — trivial).
