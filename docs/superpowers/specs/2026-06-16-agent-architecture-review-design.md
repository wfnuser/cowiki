# Agent Architecture Review — Design Spec

**Date**: 2026-06-16
**Status**: Design
**Topic**: Agent visibility, compile prompt tuning, code structure consolidation, skill spec compliance, directory layout fix

---

## Overview

6 inter-related improvements to the agent subsystem, derived from code review.

| # | Topic | Nature |
|---|-------|--------|
| 1 | Agent visibility — pi RPC mode + SSE | Architecture |
| 2 | Compile prompt — wiki count limits + /compiler command | Prompt engineering |
| 3 | AgentManager absorbs Pool + Stream | Code structure |
| 4 | Skills — keep `include_str!` | No change |
| 5 | SKILL.md — add YAML frontmatter | Spec compliance |
| 6 | Skills directory — fix `skills.json` path | Bug fix |

---

## 1. Agent Visibility — pi RPC Mode + SSE + Tracking

### Current State

- `AgentEvent` enum defined in `stream/mod.rs` with `ToolStart`/`ToolEnd`/`TaskStarted`/`TaskCompleted`/`AgentStopped` variants — never emitted
- `AgentPool` has `broadcast_event()` and `subscribe_events()` but `dispatch()` is a stub
- `AgentManager` (OneShot path) uses no event system at all
- Server has a stub SSE endpoint at `/api/agents/{ws}/events` that sends only a "connected" message
- Only visibility mechanism: `.cowiki/tracking.json` written by `WikiFsGateway`

### Design

#### pi RPC Mode

pi agent runs with an RPC flag (`--rpc` or `PI_RPC_MODE=1`) that changes stdout semantics:

```
Normal mode:  free-form text on stdout
RPC mode:     NDJSON event stream on stdout (one JSON object per line)
```

Event types pi emits on stdout:

```json
{"type":"task_started","task_id":"...","agent":"compiler"}
{"type":"tool_start","tool":"list","input":{"dir":"wiki"},"task_id":"..."}
{"type":"tool_end","tool":"list","success":true,"summary":"found 12 pages","task_id":"..."}
{"type":"tool_start","tool":"read","input":{"path":"wiki/docker.md"},"task_id":"..."}
{"type":"tool_end","tool":"read","success":true,"summary":"read 450 bytes","task_id":"..."}
{"type":"task_completed","success":true,"rounds":5,"task_id":"...","written_pages":["wiki/..."]}
```

#### AgentManager Parsing

`spawn_and_wait` in RPC mode:

```
1. Spawn pi with --rpc
2. Read stdout line-by-line
3. For each line: parse JSON → match on "type" field
   - task_started  → broadcast_event(TaskStarted)
   - tool_start    → broadcast_event(ToolStart{agent, task_id, tool, input})
   - tool_end      → broadcast_event(ToolEnd{agent, task_id, tool, success, summary})
   - task_completed → extract final result (success, rounds, written_pages)
4. On child exit: emit TaskCompleted
```

#### Server SSE

Server's `/api/agents/{ws}/events` subscribes to AgentManager's broadcast channel, converts `AgentEvent` to SSE `data:` frames, pushes to frontend.

#### Tracking File

`.cowiki/tracking.json` keeps its current role: audit trail for wiki_fs operations (written_pages, removed_pages). It is NOT used for tool_start/tool_end — those come through the RPC event stream.

#### MCP vs RPC Boundary

| Concern | Mechanism |
|---------|-----------|
| Agent dialogue flow (tool calls, rounds) | pi RPC mode → AgentManager → SSE |
| Wiki filesystem operations (writes, removes) | MCP server → `.cowiki/tracking.json` |

---

## 2. Compile Prompt — /compiler + Wiki Count Limits

### Problem

`build_compile_prompt()` in `crates/core/src/compiler/agent.rs` generates too many wiki pages because it lacks the explicit page count guidelines from SKILL.md Phase 5.

### Design

#### Revised Prompt Structure

```
/compiler

Sources to compile:
- sources/article1.md
- sources/article2.md

--- Session Context ---
workspace: repo/{ws}
branch: {branch}
execution_id: {id}

--- Tool Permissions ---
Your built-in tools: READ-ONLY (list, read, search)
cowiki MCP tools: full access (list, read, write, remove, search — all 5 primitives)

--- Workflow ---
[SKILL.md content follows, with emphasis on Phase 5...]
```

#### Phase 5 Wiki Count Limits (from SKILL.md)

Added explicitly into the prompt:

```
### Page Count Guidelines

| Source content | Wiki pages |
|---|---|
| Single unified topic | 1 |
| Source with 2-3 distinct topics | 2-3 |
| Very broad source (4+ topics) | 3-5 max |

**Do not exceed 5 wiki pages per compile session.**
```

#### pi Native Tool Restriction

pi starts with a restricted tool set:
- **Allowed**: Read, Grep, Glob (read-only inspection)
- **Blocked**: Write, Edit, Bash (no filesystem mutation)

cowiki MCP tools (list/read/write/remove/search) remain fully available — these are the sanctioned path for wiki operations, routed through WikiFsGateway with tracking and permissions.

---

## 3. AgentManager Absorbs Pool + Stream

### Current State

```
AgentManager                  AgentPool (unused stub)
├── processes (OneShot)       ├── registry (Stream agents)
├── mcp_url                   ├── broadcast_tx → AgentEvent
├── workspace_path            ├── config + reaper
└── llm                       └── dispatch() → stub

AgentEvent (stream/mod.rs)
├── ToolStart / ToolEnd       ← never emitted
├── TaskStarted / Completed
└── AgentStopped
```

### Design

Merge everything into `AgentManager`:

```rust
pub struct AgentManager {
    // ── OneShot ──
    pub(crate) processes: Arc<RwLock<HashMap<String, AgentProcess>>>,

    // ── Stream ── (migrated from AgentPool)
    registry: Arc<RwLock<HashMap<String, PooledAgent>>>,

    // ── Visibility ── (migrated from AgentPool, now shared by OneShot too)
    event_tx: broadcast::Sender<AgentEvent>,

    // ── Config ──
    mcp_url: String,
    pub workspace_path: PathBuf,
    pub llm: cowiki_utils::LlmConfig,
    idle_timeout_secs: u64,
}
```

#### Files Changed

| Action | File | Detail |
|--------|------|--------|
| **Delete** | `crates/agents/src/pool/mod.rs` | All logic moved to AgentManager |
| **Keep** | `crates/agents/src/stream/mod.rs` | `AgentEvent` type definition only — remove `pub mod pool` from lib.rs |
| **Modify** | `crates/agents/src/manager/mod.rs` | Add: registry, event_tx, broadcast_event(), subscribe_events(), register_stream_agent(), unregister_stream_agent(), start_reaper() |
| **Modify** | `crates/agents/src/lib.rs` | Remove `pub mod pool;` |

#### New AgentManager API (additions)

```rust
impl AgentManager {
    // ── Visibility ──

    /// Subscribe to agent events (SSE feed for server).
    pub fn subscribe_events(&self) -> broadcast::Receiver<AgentEvent>;

    /// Emit an event to all subscribers.
    pub fn broadcast_event(&self, event: AgentEvent);

    // ── Stream mode ──

    /// Register a long-lived agent (future use).
    pub async fn register_stream_agent(&self, name: &str, handle: Arc<dyn AgentHandle>);

    /// Unregister a stream agent.
    pub async fn unregister_stream_agent(&self, name: &str);

    /// Start idle agent reaper (spawns background task).
    pub fn start_reaper(self: &Arc<Self>);
}
```

#### OneShot Event Flow

```
AgentManager::spawn_and_wait(name, task):
  1. broadcast_event(TaskStarted { agent: name, task_id })
  2. Spawn pi process with --rpc
  3. Read stdout lines, parse NDJSON:
     - tool_start  → broadcast_event(ToolStart { ... })
     - tool_end    → broadcast_event(ToolEnd { ... })
  4. Wait for child exit
  5. broadcast_event(TaskCompleted { agent: name, task_id, success, rounds })
  6. Return AgentResponse
```

---

## 4. Skills Loading — Keep `include_str!`

**Decision**: No change. Skills continue to be embedded via `include_str!()` at compile time. This keeps deployment simple (single binary) at the cost of hot-reload — acceptable for current needs.

**Future consideration** (not in scope): A `build.rs` that copies the skills directory to the target output, read at runtime from a path adjacent to the binary, enabling hot-reload without recompilation.

---

## 5. SKILL.md Spec Compliance — Add YAML Frontmatter

### Current State

All 3 SKILL.md files start with `# Title` — no YAML frontmatter, missing `name` and `description` fields required by the SKILL spec.

### Design

Add YAML frontmatter to each:

**compiler/SKILL.md**:
```yaml
---
name: compiler
description: Transform source documents into a structured, interlinked knowledge base using cowiki's multi-directory architecture
---
```

**deep-compile/SKILL.md**:
```yaml
---
name: deep-compile
description: Compile source documents, run deterministic and heuristic lint checks, and fix issues iteratively
---
```

**review/SKILL.md**:
```yaml
---
name: review
description: Read-only analysis of wiki content — review diffs, check quality, and report findings without modifying pages
---
```

---

## 6. Skills Directory Structure — Fix skills.json Path

### Bug

In `harness/pi.rs:write_skills()`:
- Skill files are written to `.pi/agent/skills/`
- `skills.json` writes `"dir": ".pi/skills"` — wrong path, should be `.pi/agent/skills`

### Fix

Change line ~229 in `harness/pi.rs` from:
```rust
"dir": ".pi/skills"
```
to:
```rust
"dir": ".pi/agent/skills"
```

### Final Runtime Directory Structure

```
{agent_home}/
├── .mcp.json                    # MCP server URL
└── .pi/
    └── agent/
        ├── models.json          # LLM provider/model config
        ├── skills.json          # [{ name: "compile", description: "...", dir: ".pi/agent/skills" }]
        └── skills/
            ├── SKILL.md         # Main workflow
            ├── conventions.md   # Naming + frontmatter conventions
            ├── tools.md         # 5 primitives reference
            ├── patterns.md      # RIGHT/WRONG call examples
            └── efficiency.md    # Read minimization rules (compile only)
```

---

## Files Changed (Summary)

| File | Change |
|------|--------|
| `crates/agents/src/lib.rs` | Remove `pub mod pool;` |
| `crates/agents/src/manager/mod.rs` | Absorb pool registry, broadcast, reaper, subscribe. Add RPC stdout parsing in `spawn_and_wait` |
| `crates/agents/src/stream/mod.rs` | Keep `AgentEvent` type only; add RPC event parsing helper |
| `crates/agents/src/pool/mod.rs` | **Delete** |
| `crates/agents/src/harness/pi.rs` | Add `--rpc` flag support; fix `skills.json` dir path; restrict native tools |
| `crates/core/src/compiler/agent.rs` | Revise `build_compile_prompt` with /compiler command, wiki count limits |
| `crates/agents/skills/compiler/SKILL.md` | Add YAML frontmatter |
| `crates/agents/skills/deep-compile/SKILL.md` | Add YAML frontmatter |
| `crates/agents/skills/review/SKILL.md` | Add YAML frontmatter |
| `crates/server/src/main.rs` | Wire SSE endpoint to AgentManager broadcast, pass `--rpc` flag |
| `crates/server/src/routes/compile.rs` | Update prompt building; pass pi native tool restriction config |

---

## Out of Scope

- pi `--rpc` flag implementation inside pi itself (this design defines the protocol; pi implementation is separate)
- Stream-mode agent implementation (only the infrastructure — registry, subscribe — is unified; actual long-lived agent logic remains future work)
- `build.rs` file-copy approach for skills
- Hot-reload of skill files
