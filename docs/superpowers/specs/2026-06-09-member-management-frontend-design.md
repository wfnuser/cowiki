# Member Management Frontend — Design Spec

Date: 2026-06-09
Context: PR #41 backend role-management refactor already merged to `dev`

## Summary

Rebuild the `MembersView.tsx` full-page component as the single member management surface, with colored avatars, working role dropdowns, member removal, invite flow, and relative "last active" timestamps. Remove the redundant members management dialog from `MainLayout.tsx`.

## Data Model: `last_active_at`

### Migration `010`

Add `last_active_at TIMESTAMPTZ` to `users` table (nullable).
### Backend changes

- `User` struct (`users.rs`): add `last_active_at: Option<DateTime<Utc>>`
- All existing User queries: add `last_active_at` to `SELECT` columns
- New function `touch_last_active(pool, user_id)` — updates `last_active_at = now()` on each authenticated request, called from the auth extractor
- `MemberResponse` (`workspace.rs` routes): add `last_active_at: Option<String>` (ISO 8601)
- `list_members` handler: join users table to fetch `last_active_at`

### Frontend changes

- `MemberInfo` (`api.ts`): add `last_active_at: string | null`
- Display as relative time in table ("2 hours ago", "yesterday", "3 days ago") or `--` if null

## UI Design

### Avatar

- 36px circle showing first letter of username, uppercased
- Background: deterministic color from `spaceTileColors` array via `hashCode(name) % spaceTileColors.length`
- Text color: white (`#fff`), font-weight 600, font-size 14px
- Hash function: `[...name].reduce((h, c) => h + c.charCodeAt(0), 0)`

### Table layout

Grid: `1fr 120px 120px 40px` with 12px gap, 10px vertical padding per row.

```
[MEMBER]              [ROLE]     [LAST ACTIVE]  [ACTIONS]
[avatar] Name         owner ▼    2 hours ago     ...
        email@...
```

**Member column:** flex row with avatar + name/email stacked vertically. Name 14.5px weight 550, email 12.5px color muted. Text overflow ellipsis.

**Role column:** inline styled dropdown. For owners (`isOwner && m.role !== 'owner'`), clicking opens a compact popover with owner/writer/reader options. For non-owners or the owner themselves, shown as read-only text. Role text colors: owner=`C.accent`, writer=`C.ink2`, reader/viewer=`C.muted`.

**Last active column:** 12px text, color `C.faint`. Null → `--`.

**Actions column:** `...` button → dropdown with "Remove member" (red). Clicking "Remove" shows an inline confirmation: "Remove [name]?" with Confirm/Cancel buttons. Owner row has no remove button. Non-owner users see no actions button at all.

### Header

Row above the table with "Members" title + count badge + "Invite people" button (accent background, UserPlus icon). Only shown when `isOwner`.

### States

| State | Display |
|-------|---------|
| Loading | "Loading members..." centered |
| Error | Error message in red |
| Empty | Users icon + "No members found." centered |
| Loaded | Table as above |

### Edge cases

- Owner viewing their own row: role shown as read-only, no remove button
- Owner viewing another owner (shouldn't happen normally): role read-only, no remove
- Member with no email: show "No email" in muted
- Very long names: text-overflow ellipsis, avatar keeps fixed size
- Removing self: can't — backend prevents it, and UI hides the button for owner's own row

## MainLayout Cleanup

Remove from `MainLayout.tsx`:

- State: `showMembersPanel`, `membersList`, `membersLoading`, `membersError`
- Functions: `openMembersPanel`, `handleRemoveMember`, `handleChangeRole`
- JSX: the entire members management `<Dialog>` block
- The gear icon's `onSettings` handler that calls `openMembersPanel`

Retain: `handleInvite`, `showInviteDialog`, `inviteEmail`, `inviteRole` (invite dialog stays).

### Invite Dialog Role Selector

Replace the native `<select>` with inline-styled role option buttons matching the MembersView role popover style:

- Layout: horizontal flex row, 8px gap
- Each option: pill-shaped button, `padding: 6px 14px`, `borderRadius: 8`, `fontSize: 13`, `fontWeight: 500`, `textTransform: capitalize`
- Selected state: background = role color at ~15% opacity (e.g., owner → `C.accentSoft`), text = role color
- Unselected: background transparent, border `1px solid C.line`, color `C.muted`
- Hover on unselected: background `C.rail`
- Role colors: owner=`C.accent`, writer=`C.ink2`, reader=`C.muted`

## Files

| File | Action |
|------|--------|
| `crates/db/migrations/010_last_active_at.sql` | New |
| `crates/db/src/users.rs` | Modify: add field + `touch_last_active` |
| `crates/server/src/routes/workspace.rs` | Modify: add `last_active_at` to response + query |
| `web/src/api.ts` | Modify: add `last_active_at` to `MemberInfo` |
| `web/src/components/views/MembersView.tsx` | Rewrite |
| `web/src/pages/MainLayout.tsx` | Clean up: remove members dialog |
