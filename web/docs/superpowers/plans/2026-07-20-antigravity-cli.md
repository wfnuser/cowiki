# Antigravity CLI Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace CoWiki's Gemini CLI launcher with Antigravity CLI and connect every Antigravity workspace to the current CoWiki Space MCP.

**Architecture:** The frontend exposes `antigravity` as the Google Agent identifier and migrates saved `gemini` preferences. The Rust terminal backend writes or merges the workspace-standard `.agents/mcp_config.json` before launching `agy`, preserving all user-owned MCP entries and injecting only CoWiki's current executable and Space slug.

**Tech Stack:** React/TypeScript, Tauri/Rust, serde_json, Node test runner, Cargo tests.

---

### Task 1: Frontend Agent Contract

**Files:**
- Modify: `src/lib/agents.ts`
- Modify: `src/lib/client-settings.ts`
- Test: `tests/agent-terminal.test.ts`
- Test: `tests/client-settings.test.ts`

- [ ] **Step 1: Write failing contract tests**

Replace Gemini assertions with:

```ts
assert.equal(agentInitialCommand('antigravity'), 'agy');
assert.equal(agentDisplayName('antigravity'), 'Antigravity CLI');
```

Add a saved-setting migration assertion:

```ts
storage.setItem('cowiki.client.settings', JSON.stringify({ defaultAgent: 'gemini' }));
assert.deepEqual(loadClientSettings(storage), { defaultAgent: 'antigravity' });
```

- [ ] **Step 2: Verify the frontend tests fail**

Run:

```bash
npm run test:agent-terminal
npm run test:client-settings
```

Expected: TypeScript/runtime failures because `antigravity` is not yet an Agent kind.

- [ ] **Step 3: Implement the frontend mapping and migration**

Change the union and catalog entry to:

```ts
export type AgentKind = 'codex' | 'claude' | 'grok' | 'antigravity' | 'opencode' | 'hermes';
antigravity: { displayName: 'Antigravity CLI', providerName: 'Google', command: 'agy' },
```

Before normal Agent-kind validation in `loadClientSettings`, migrate:

```ts
if ((value as Partial<ClientSettings>).defaultAgent === 'gemini') {
  return { defaultAgent: 'antigravity' };
}
```

- [ ] **Step 4: Verify frontend tests pass**

Run:

```bash
npm run test:agent-terminal
npm run test:client-settings
```

Expected: both suites pass.

### Task 2: Antigravity Workspace MCP and Launch

**Files:**
- Modify: `src-tauri/src/terminal.rs`

- [ ] **Step 1: Write failing Rust tests**

Add tests proving:

```rust
assert!(validate_initial_command(AgentKind::Antigravity, Some("agy")).is_ok());
assert!(antigravity.shell_command.starts_with("agy --prompt-interactive "));
```

Use a temporary directory containing an existing MCP entry, invoke
`ensure_antigravity_mcp_config`, and assert the existing entry remains while
`mcpServers.cowiki.args` equals `["--mcp", "--space", "research-space"]`.

- [ ] **Step 2: Verify the Rust test fails**

Run:

```bash
cargo test terminal::tests --manifest-path src-tauri/Cargo.toml
```

Expected: compilation fails because `AgentKind::Antigravity` and the config helper do not exist.

- [ ] **Step 3: Implement Antigravity launch and MCP merge**

Rename the Rust enum variant and command:

```rust
Antigravity,
Self::Antigravity => "agy",
```

Before spawning the PTY, call:

```rust
if matches!(request.agent, AgentKind::Antigravity) {
    ensure_antigravity_mcp_config(&cwd, &executable, &space.slug)?;
}
```

Implement `ensure_antigravity_mcp_config` to create `.agents`, parse an existing
JSON object or start with `{ "mcpServers": {} }`, preserve unrelated entries,
replace only `mcpServers.cowiki`, and write pretty JSON. Launch with:

```rust
AgentKind::Antigravity => AgentLaunchCommand {
    shell_command: "agy --prompt-interactive \"${COWIKI_AGENT_PROMPT}\"".to_string(),
    environment: prompt_environment(),
},
```

- [ ] **Step 4: Verify Rust tests pass**

Run:

```bash
cargo test terminal::tests --manifest-path src-tauri/Cargo.toml
```

Expected: all terminal tests pass.

### Task 3: Full Verification and Restart

**Files:**
- Verify all modified frontend and Rust files.

- [ ] **Step 1: Run complete relevant verification**

```bash
npm test
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: all commands exit successfully.

- [ ] **Step 2: Restart the desktop development client**

Stop the active `npm run desktop:dev` session and run:

```bash
npm run desktop:dev
```

Expected: the desktop opens and its Agent picker displays `Antigravity CLI`.

- [ ] **Step 3: Confirm runtime MCP configuration**

Start Antigravity from CoWiki and inspect the selected Space:

```bash
jq '.mcpServers.cowiki' <space-repo>/.agents/mcp_config.json
```

Expected: command points to the running CoWiki desktop executable and args contain the selected Space slug.
