# macOS DMG Release Design

## Goal

Build the Tauri macOS desktop client in GitHub Actions and make its DMG available both as a workflow artifact and, for desktop version tags, as a GitHub Release asset.

## Scope

- Pull requests targeting `dev`, manual dispatches, and `desktop-v*` tags build one x86_64 DMG on the fixed `macos-15-intel` runner.
- Every successful build uploads the DMG as a GitHub Actions artifact and fails if no DMG was produced.
- A `desktop-v*` tag additionally creates a GitHub Release and uploads the same DMG.
- Tauri bundling is enabled in the checked-in configuration.
- The workflow contract is covered by a small Node test that is also run by the existing CI workflow.
- History and Agent Review behavior and code remain untouched.

## Workflow Architecture

The new `.github/workflows/macos-desktop.yml` workflow has two jobs. The `build-macos` job uses read-only repository permissions, installs the locked npm and Cargo dependencies, runs `npm run desktop:build -- --target x86_64-apple-darwin --bundles dmg`, and uploads `web/src-tauri/target/x86_64-apple-darwin/release/bundle/dmg/*.dmg` with a fourteen-day retention period. The artifact is named `CoWiki-macOS-x86_64` so its platform and architecture are explicit.

The `release` job runs only for refs matching `refs/tags/desktop-v`. It downloads the build artifact and uses the preinstalled GitHub CLI plus the job-scoped `contents: write` permission to create a release for that tag with generated notes and attach the DMG. This avoids a third-party release action and keeps write permission out of pull-request and manual builds.

## Signing and Distribution

The workflow does not read Apple signing or notarization secrets. The resulting DMG is unsigned and unnotarized, so macOS Gatekeeper may warn or block a normal first launch. Testers must explicitly approve the app through Finder or macOS Privacy & Security. Public tag releases provide a stable direct GitHub download; workflow artifacts are temporary and generally require GitHub authentication.

Signing and notarization are intentionally deferred. Adding them later should be a separate change using repository secrets for an Apple certificate, certificate password, Apple ID or App Store Connect credentials, and team ID without changing the unsigned build path.

## Failure Handling

- `npm ci`, frontend compilation, Rust compilation, or DMG bundling failures fail the build job.
- `actions/upload-artifact` uses `if-no-files-found: error`, so a successful compiler run without a DMG cannot silently pass.
- The release job depends on the build artifact and therefore cannot publish an incomplete release.
- The release command is idempotent for reruns: it creates the release when absent, then uploads the DMG with `--clobber`.

## Verification

A static Node contract test checks the required triggers, fixed runner, locked install, DMG build command, artifact path and failure mode, tag-only release guard, least-privilege release permission, and active Tauri bundling. The implementation is also verified with the full Web test suite, the production frontend build, Cargo formatting/check/tests, YAML parsing, `actionlint`, and a local unsigned DMG build on the available macOS host.
