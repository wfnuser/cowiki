# Local Review and Source Flow Implementation Plan

> **For agentic workers:** Follow test-driven development for every behavior.

**Goal:** Replace inline local Review accordions with list/detail navigation,
fix Source layout overflow, and expose deterministic Source import separately
from optional default-Agent synthesis.

**Architecture:** Local review selection stays inside the desktop Review
component so cloud routes remain unchanged. A dedicated local detail component
owns diff actions. Add Source reports imported Source identities and requests an
Agent Change through the existing desktop Agent panel. Shared pure helpers
provide testable view models and Source document normalization.

**Tech stack:** React, TypeScript, Tauri, Rust, Node test runner.

---

### Task 1: Lock the review navigation contract

**Files:**
- Modify: `tests/agent-terminal.test.ts`
- Modify: `src/components/review/local-review-model.ts`
- Modify: `src/components/review/LocalReviewInbox.tsx`
- Create: `src/components/review/LocalReviewDetail.tsx`

- [ ] Add failing tests for Current Draft selection, Agent Change selection,
  branch labels, and `Create Checkpoint` language.
- [ ] Run `npm run test:agent-terminal` and verify the new assertions fail.
- [ ] Replace accordion state with a selected list/detail state.
- [ ] Move diff rendering and actions into `LocalReviewDetail`.
- [ ] Run the focused tests and build.

### Task 2: Lock and fix Source rendering

**Files:**
- Modify: `tests/page-frontmatter.test.ts`
- Modify: `tests/page-header.test.ts`
- Modify: `src/lib/page-frontmatter.ts`
- Modify: `src/pages/MainLayout.tsx`
- Modify: `src/index.css`

- [ ] Add failing tests for frontmatter-free Source render and non-shrinking
  header actions.
- [ ] Reproduce the failures.
- [ ] Reuse the frontmatter splitter for read-only Source Markdown.
- [ ] Add bounded breadcrumb/action styles and safe long-path heading wrapping.
- [ ] Run focused tests and build.

### Task 3: Expose Source import phases

**Files:**
- Modify: `tests/source-ingest.test.ts`
- Modify: `src/lib/source-ingest.ts`
- Modify: `src/components/AddSourceDialog.tsx`
- Modify: `src/pages/MainLayout.tsx`
- Modify: `src/components/terminal/AgentTerminalPanel.tsx`
- Modify: `src/components/terminal/terminal-tabs.ts`

- [ ] Add failing tests for imported Source identities and Agent task copy.
- [ ] Run `npm run test:source-ingest` and verify failure.
- [ ] Keep the dialog open after successful import with a clear success state.
- [ ] Show the Settings-selected Agent and allow explicit organization in an
  isolated Agent Change.
- [ ] Pass the imported Source paths into the Agent task.
- [ ] Run focused frontend and terminal tests.

### Task 4: Verify and publish

- [ ] Run `npm test`.
- [ ] Run `npm run build`.
- [ ] Run `cargo test --manifest-path src-tauri/Cargo.toml`.
- [ ] Inspect `git diff` for unrelated changes.
- [ ] Commit, push the feature branch, and create or update its pull request.
