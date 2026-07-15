# macOS DMG Release Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build an unsigned x86_64 CoWiki DMG in GitHub Actions, retain it as an artifact, and attach it to GitHub Releases created from `desktop-v*` tags.

**Architecture:** A dedicated macOS workflow builds on the standard x64 `macos-15-intel` runner with explicit `x86_64-apple-darwin` targeting. A separate tag-only release job downloads the artifact and publishes it with the built-in GitHub token, while a Node contract test protects the workflow triggers, build command, artifact path, permissions, and Tauri bundle setting.

**Tech Stack:** GitHub Actions, Tauri 2, Node.js 24, npm, Rust stable, Node test runner, GitHub CLI

---

### Task 1: Add a failing desktop workflow contract test

**Files:**
- Create: `web/tests/macos-desktop-workflow.test.ts`
- Modify: `web/package.json`
- Modify: `.github/workflows/ci.yml`

- [ ] **Step 1: Write the failing test**

Create `web/tests/macos-desktop-workflow.test.ts`:

```typescript
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const workflowPath = new URL("../../.github/workflows/macos-desktop.yml", import.meta.url);
const tauriConfigPath = new URL("../src-tauri/tauri.conf.json", import.meta.url);

test("macOS desktop builds run for dev pull requests, manual dispatches, and desktop tags", () => {
  const workflow = readFileSync(workflowPath, "utf8");

  assert.match(workflow, /^on:\n  pull_request:\n    branches: \[dev\]\n  workflow_dispatch:\n  push:\n    tags:\n      - "desktop-v\*"/m);
  assert.match(workflow, /build-macos:\n[\s\S]*?runs-on: macos-15-intel/);
  assert.match(workflow, /targets: x86_64-apple-darwin/);
  assert.match(workflow, /run: npm ci/);
  assert.match(workflow, /run: npm run desktop:build -- --target x86_64-apple-darwin --bundles dmg/);
});

test("the x86_64 DMG is retained and desktop tags publish the same artifact", () => {
  const workflow = readFileSync(workflowPath, "utf8");

  assert.match(workflow, /uses: actions\/upload-artifact@v4\n\s+with:\n\s+name: CoWiki-macOS-x86_64/);
  assert.match(workflow, /path: web\/src-tauri\/target\/x86_64-apple-darwin\/release\/bundle\/dmg\/\*\.dmg/);
  assert.match(workflow, /if-no-files-found: error/);
  assert.match(workflow, /release:\n\s+if: startsWith\(github\.ref, 'refs\/tags\/desktop-v'\)/);
  assert.match(workflow, /release:[\s\S]*?permissions:\n\s+contents: write/);
  assert.match(workflow, /uses: actions\/download-artifact@v4\n\s+with:\n\s+name: CoWiki-macOS-x86_64/);
  assert.match(workflow, /gh release view "\$GITHUB_REF_NAME"/);
  assert.match(workflow, /gh release create "\$GITHUB_REF_NAME" --verify-tag --generate-notes/);
  assert.match(workflow, /gh release upload "\$GITHUB_REF_NAME" release-assets\/\*\.dmg --clobber/);
});

test("Tauri bundling is enabled", () => {
  const config = JSON.parse(readFileSync(tauriConfigPath, "utf8"));
  assert.equal(config.bundle.active, true);
});
```

- [ ] **Step 2: Expose the test through npm and existing CI**

Add this package script:

```json
"test:desktop-workflow": "node --experimental-strip-types --test tests/macos-desktop-workflow.test.ts"
```

Append `npm run test:desktop-workflow` to the existing `test` script and add this step after `npm ci` in the existing Web CI job:

```yaml
- name: Test desktop workflow contract
  run: npm run test:desktop-workflow
```

- [ ] **Step 3: Run the test to verify RED**

Run: `npm run test:desktop-workflow`

Expected: FAIL because `.github/workflows/macos-desktop.yml` does not exist and `bundle.active` is still false.

### Task 2: Implement the macOS build and release workflow

**Files:**
- Create: `.github/workflows/macos-desktop.yml`
- Modify: `web/src-tauri/tauri.conf.json`

- [ ] **Step 1: Enable Tauri application bundling**

Change the existing bundle configuration to:

```json
"bundle": {
  "active": true,
  "targets": "all"
}
```

The workflow still passes `--bundles dmg`, so CI produces only the requested macOS disk image.

- [ ] **Step 2: Add the build job**

Create `.github/workflows/macos-desktop.yml` with pull-request targeting `dev`, manual, and `desktop-v*` tag triggers. Give the workflow default `contents: read`, run `build-macos` on `macos-15-intel`, install Node 24 and stable Rust with the `x86_64-apple-darwin` target, restore the Rust cache, run `npm ci`, and execute the following complete workflow:

```yaml
name: macOS Desktop

on:
  pull_request:
    branches: [dev]
  workflow_dispatch:
  push:
    tags:
      - "desktop-v*"

permissions:
  contents: read

jobs:
  build-macos:
    runs-on: macos-15-intel
    defaults:
      run:
        working-directory: web
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 24
          cache: npm
          cache-dependency-path: web/package-lock.json
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: x86_64-apple-darwin
      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: web/src-tauri -> target
      - name: Install dependencies
        run: npm ci
      - name: Build unsigned x86_64 DMG
        run: npm run desktop:build -- --target x86_64-apple-darwin --bundles dmg
      - name: Upload DMG artifact
        uses: actions/upload-artifact@v4
        with:
          name: CoWiki-macOS-x86_64
          path: web/src-tauri/target/x86_64-apple-darwin/release/bundle/dmg/*.dmg
          if-no-files-found: error
          retention-days: 14

  release:
    if: startsWith(github.ref, 'refs/tags/desktop-v')
    needs: build-macos
    runs-on: ubuntu-latest
    permissions:
      contents: write
    steps:
      - name: Download DMG artifact
        uses: actions/download-artifact@v4
        with:
          name: CoWiki-macOS-x86_64
          path: release-assets
      - name: Create release and upload DMG
        env:
          GH_TOKEN: ${{ github.token }}
        run: |
          gh release view "$GITHUB_REF_NAME" >/dev/null 2>&1 || \
            gh release create "$GITHUB_REF_NAME" --verify-tag --generate-notes \
              --title "CoWiki Desktop $GITHUB_REF_NAME"
          gh release upload "$GITHUB_REF_NAME" release-assets/*.dmg --clobber
```

Upload `web/src-tauri/target/x86_64-apple-darwin/release/bundle/dmg/*.dmg` as `CoWiki-macOS-x86_64` with `if-no-files-found: error` and fourteen-day retention.

- [ ] **Step 3: Add the least-privilege release job**

Verify the workflow's `release` job is dependent on `build-macos`, guarded by `startsWith(github.ref, 'refs/tags/desktop-v')`, and is the only job granted `contents: write`. It must download `CoWiki-macOS-x86_64` into `release-assets`, create the tag's release if absent, and upload `release-assets/*.dmg --clobber` using `GH_TOKEN: ${{ github.token }}` as shown in Step 2.

- [ ] **Step 4: Run the contract test to verify GREEN**

Run: `npm run test:desktop-workflow`

Expected: 3 tests pass, 0 fail.

### Task 3: Verify the complete change

**Files:**
- Verify: `.github/workflows/macos-desktop.yml`
- Verify: `.github/workflows/ci.yml`
- Verify: `web/tests/macos-desktop-workflow.test.ts`
- Verify: `web/src-tauri/tauri.conf.json`

- [ ] **Step 1: Validate workflow syntax and conventions**

Run:

```bash
ruby -e 'require "yaml"; ARGV.each { |path| YAML.safe_load(File.read(path), aliases: true); puts "valid YAML: #{path}" }' .github/workflows/ci.yml .github/workflows/macos-desktop.yml
actionlint .github/workflows/ci.yml .github/workflows/macos-desktop.yml
```

Expected: both YAML files parse and `actionlint` reports no errors.

- [ ] **Step 2: Run Web and Rust verification**

Run:

```bash
cd web
npm test
npm run build
cargo fmt --all --manifest-path src-tauri/Cargo.toml -- --check
cargo check --manifest-path src-tauri/Cargo.toml --locked
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

Expected: all tests, build, formatting, native check, and Rust tests pass. The pull-request workflow performs the x86_64 check and release build on its native Intel runner.

- [ ] **Step 3: Build and inspect a local unsigned DMG**

On the macOS host, run:

```bash
cd web
npm run desktop:build -- --bundles dmg
file src-tauri/target/release/bundle/dmg/*.dmg
find src-tauri/target/release/bundle/dmg -name '*.dmg' -type f -maxdepth 1
```

Expected: Tauri reports a generated host-native DMG, `file` recognizes the disk image, and one `.dmg` path is printed. The pull-request workflow supplies the required x86_64 artifact verification on `macos-15-intel`.

- [ ] **Step 4: Review the requirements and diff**

Run `git diff --check`, `git diff --stat origin/dev`, `git diff origin/dev`, and `git status --short`. Confirm no History or Agent Review files changed and no generated build output is tracked.

### Task 4: Publish the pull request

**Files:**
- Commit the workflow, contract test, Tauri configuration, CI hook, and design/plan documents.

- [ ] **Step 1: Commit the verified implementation**

Run `git add` only for the reviewed files, then commit with `ci: build and publish macOS DMG`.

- [ ] **Step 2: Push without rewriting history**

Run: `git push -u origin ci/macos-dmg-release`

Expected: the branch is created on `origin`; no force push is used.

- [ ] **Step 3: Create a pull request targeting dev**

Use `gh pr create --base dev --head ci/macos-dmg-release`. The body must document triggers, artifact and Release locations, local verification, the absence of required custom secrets, and the unsigned/unnotarized Gatekeeper limitation.

- [ ] **Step 4: Inspect the remote PR**

Run `gh pr view --json url,baseRefName,headRefName,state,statusCheckRollup` and confirm the PR is open, targets `dev`, and reports the new macOS build check.
