import assert from "node:assert/strict"
import { readFileSync } from "node:fs"
import test from "node:test"

const workflowPath = new URL("../../.github/workflows/macos-desktop.yml", import.meta.url)
const tauriConfigPath = new URL("../src-tauri/tauri.conf.json", import.meta.url)

test("macOS desktop builds run for dev pull requests, manual dispatches, and desktop tags", () => {
  const workflow = readFileSync(workflowPath, "utf8")

  assert.match(
    workflow,
    /^on:\n  pull_request:\n    branches: \[dev\]\n  workflow_dispatch:\n  push:\n    tags:\n      - "desktop-v\*"/m,
  )
  assert.match(workflow, /build-macos:\n[\s\S]*?runs-on: macos-15-intel/)
  assert.match(workflow, /targets: x86_64-apple-darwin/)
  assert.match(workflow, /run: npm ci/)
  assert.match(
    workflow,
    /run: npm run desktop:build -- --target x86_64-apple-darwin --bundles dmg/,
  )
})

test("the x86_64 DMG is retained and desktop tags publish the same artifact", () => {
  const workflow = readFileSync(workflowPath, "utf8")

  assert.match(
    workflow,
    /uses: actions\/upload-artifact@v4\n\s+with:\n\s+name: CoWiki-macOS-x86_64/,
  )
  assert.match(
    workflow,
    /path: web\/src-tauri\/target\/x86_64-apple-darwin\/release\/bundle\/dmg\/\*\.dmg/,
  )
  assert.match(workflow, /if-no-files-found: error/)
  assert.match(
    workflow,
    /release:\n\s+if: startsWith\(github\.ref, 'refs\/tags\/desktop-v'\)/,
  )
  assert.match(workflow, /release:[\s\S]*?permissions:\n\s+contents: write/)
  assert.match(
    workflow,
    /uses: actions\/download-artifact@v4\n\s+with:\n\s+name: CoWiki-macOS-x86_64/,
  )
  assert.ok(workflow.includes("GH_REPO: ${{ github.repository }}"))
  assert.match(workflow, /gh release view "\$GITHUB_REF_NAME"/)
  assert.match(
    workflow,
    /gh release create "\$GITHUB_REF_NAME" --verify-tag --generate-notes/,
  )
  assert.match(
    workflow,
    /gh release upload "\$GITHUB_REF_NAME" release-assets\/\*\.dmg --clobber/,
  )
  assert.ok(
    workflow.includes(
      "This DMG is unsigned and unnotarized. macOS Gatekeeper may require explicit approval through Finder or Privacy & Security.",
    ),
  )
})

test("Tauri bundling is enabled", () => {
  const config = JSON.parse(readFileSync(tauriConfigPath, "utf8"))

  assert.equal(config.bundle.active, true)
})
