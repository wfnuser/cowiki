# Header Source and External Browser Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the duplicate Add Source header action and make the Cloud dialog open Cloud URLs in the system browser.

**Architecture:** Keep ingestion in the existing Sources overflow menu. Add a narrow Tauri command that validates HTTP(S) URLs before delegating to the existing platform browser opener, and call it through a focused frontend helper.

**Tech Stack:** React, TypeScript, Node test runner, Tauri 2, Rust

---

### Task 1: Remove Add Source From the Page Header

**Files:**
- Modify: `web/tests/page-header.test.ts`
- Modify: `web/src/pages/MainLayout.tsx`

- [ ] **Step 1: Write the failing header contract test**

Extract the `app-header-actions` source region and assert that it has no
`Add Source`, while `SpacePanel` still owns the ingestion entry:

```ts
test('source ingestion lives in the Sources menu instead of the page header', () => {
  const headerActions = mainLayout
    .split('{/* Right: actions */}')[1]
    .split('{/* User menu moved to Rail bottom avatar */}')[0];
  assert.doesNotMatch(headerActions, /Add Source/);
  assert.match(spacePanel, /<DropdownMenuItem onClick=\{onShowIngest\}>/);
  assert.match(spacePanel, /> Add Source<\\/DropdownMenuItem>/);
});
```

- [ ] **Step 2: Run the test and verify RED**

Run:

```bash
cd web && npm run test:page-header
```

Expected: FAIL because the page header still renders `Add Source`.

- [ ] **Step 3: Remove only the header button**

Delete the `setShowIngest(true)` header button from `MainLayout.tsx`. Remove
the `Upload` import from that file only if no other `Upload` usage remains.
Do not change `SpacePanel` or `AddSourceDialog`.

- [ ] **Step 4: Run the test and verify GREEN**

Run:

```bash
cd web && npm run test:page-header
```

Expected: PASS, including the assertion that Sources retains its ingestion
menu.

### Task 2: Open Cloud URLs in the System Browser

**Files:**
- Create: `web/src/external-links.ts`
- Modify: `web/tests/cloud-space-dialog.test.ts`
- Modify: `web/src/components/cloud/CloudSpaceDialog.tsx`
- Modify: `web/src-tauri/src/lib.rs`

- [ ] **Step 1: Write failing frontend and Rust tests**

Add a frontend contract proving the dialog uses the external-link helper:

```ts
test('Open in browser delegates to the desktop external URL boundary', () => {
  assert.match(dialog, /openExternalUrl/);
  assert.doesNotMatch(dialog, /window\\.open/);
});
```

Add a Rust unit test for a pure URL validator:

```rust
#[test]
fn external_browser_urls_accept_only_http_and_https() {
    assert!(validate_external_url("https://cloud.cowiki.app/cloud").is_ok());
    assert!(validate_external_url("http://localhost:8787/cloud").is_ok());
    assert!(validate_external_url("file:///tmp/private").is_err());
    assert!(validate_external_url("javascript:alert(1)").is_err());
}
```

- [ ] **Step 2: Run the tests and verify RED**

Run:

```bash
(cd web && npm run test:cloud-space-dialog)
cargo test --manifest-path web/src-tauri/Cargo.toml external_browser_urls_accept_only_http_and_https
```

Expected: frontend FAIL because the helper is absent; Rust FAIL because
`validate_external_url` is undefined.

- [ ] **Step 3: Implement the validated Tauri command**

Add a pure `validate_external_url` function that parses the URL, requires
`http` or `https`, and requires a host. Add an `open_external_url` Tauri
command that validates before calling the existing `open_system_browser`,
then register it in `generate_handler!`.

- [ ] **Step 4: Implement the frontend boundary**

Create:

```ts
import { invoke } from '@tauri-apps/api/core';

export function openExternalUrl(url: string): Promise<void> {
  return invoke('open_external_url', { url });
}
```

Replace `window.open` in `CloudSpaceDialog` with an async call to
`openExternalUrl`. Clear the previous error before opening and render any
command error through the dialog's existing error state.

- [ ] **Step 5: Run focused tests and verify GREEN**

Run:

```bash
(cd web && npm run test:cloud-space-dialog)
cargo test --manifest-path web/src-tauri/Cargo.toml external_browser_urls_accept_only_http_and_https
```

Expected: both PASS.

### Task 3: Verify and Commit to Dev

**Files:**
- Verify all files changed in Tasks 1–2.

- [ ] **Step 1: Run full verification**

```bash
(cd web && npm test)
(cd web && npm run build)
cargo test --manifest-path web/src-tauri/Cargo.toml --locked
cargo fmt --all --manifest-path web/src-tauri/Cargo.toml -- --check
git diff --check
```

Expected: all commands exit 0.

- [ ] **Step 2: Commit and push**

```bash
git add docs/superpowers/plans/2026-07-23-header-source-and-external-browser.md \
  web/tests/page-header.test.ts \
  web/tests/cloud-space-dialog.test.ts \
  web/src/pages/MainLayout.tsx \
  web/src/components/cloud/CloudSpaceDialog.tsx \
  web/src/external-links.ts \
  web/src-tauri/src/lib.rs
git commit -m "fix(desktop): keep Cloud links outside the client"
git push origin dev
```
