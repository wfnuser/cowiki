# Cowiki Agent Integration Design

> Status: Draft v1 | Date: 2026-06-09 | Spec: `docs/compile-system-design.md`

## Overview

Cowiki integrates external agent runtimes (PiAgent, OpenClaw, future Rust agents) as persistent, per-repo team members. Each agent operates within its own directory under `agents/<name>/`, manages its own state (SOUL, sessions, prompts), and communicates with cowiki through a standardized HTTP protocol.

### Core Principles

**Agent owns state. Cowiki owns execution.** The agent manages its own memory (sessions, compression, branching). Cowiki is a service gateway: it proxies LLM calls (managing API keys centrally) and executes tools (deterministic, server-side). The agent thinks; cowiki acts.

**Directory is identity.** An agent's directory under `agents/` is its home. Cowiki redirects the agent runtime's default data directory here. Everything the agent creates, remembers, or configures lives in this directory — portable, git-trackable, and inspectable by humans.

## End-to-End Flow

### Startup

```
cowiki server start
  │
  ├── 1. Ensure agent runtime in COWIKI_DATA_DIR/agents/{type}/
  │       (PiAgent: git clone + npm install, cached; skip if current)
  │
  ├── 2. Scan agents/ directory, read each agent.toml
  │       → agent {name, type, task}
  │
  ├── 3. For each agent, spawn process:
  │       env:
  │         COWIKI_AGENT_ID       = "{name}-{uuid4}"
  │         COWIKI_AGENT_HOME     = "workspace/agents/{name}/"
  │         COWIKI_AGENT_TOKEN    = "<shared-secret>"
  │         COWIKI_SERVER_URL     = "http://127.0.0.1:9400"
  │
  ├── 4. Wait for handshake:
  │       agent → POST /agent/register
  │       { agent_id, type, task, token, sessions: [...] }
  │       cowiki validates token, indexes sessions
  │
  └── 5. Ready. Agent registered in HarnessRegistry.
```

### Compile Request

```
User clicks "Compile" in Web UI → POST /api/compile { branch }
  │
  ├── cowiki selects agent by task type ("shallow_compile")
  │     └── default: agents/compiler (created on first use if absent)
  │
  ├── Acquire pool permit (tier-gated: 1 for free, 4 for pro, 16 for enterprise)
  │
  ├── Build AgentRequest:
  │     {
  │       task_type: "shallow_compile",
  │       session_id: "sess-003",           ← user selected or new
  │       system_prompt: "...",             ← from agent.toml config
  │       user_input: "compile sources/webapp-docs-site/",
  │       tools: [ls_sources, read_manifest, ...],  ← injected by task
  │       workspace_path: "workspace/",
  │       config: { max_rounds: 20, ... }
  │     }
  │
  ├── POST /agent/run  → agent process
  │
  └── SSE stream events to frontend (phase, agent-start, tool-call, ...)
```

### Agent Thinking Loop (inside PiAgent process)

```
Agent receives /agent/run request
  │
  ├── Load session from sessions/sess-003.jsonl
  ├── Read SOUL.md from agents/compiler/SOUL.md (if exists)
  │
  └── Think loop:
        │
        ├── Need LLM?
        │     POST /agent/llm/stream → cowiki
        │       { agent_id, messages: [...], config: {model, ...} }
        │     ← SSE stream: text_delta, toolcall_start, stop, usage
        │
        ├── Need tool execution?
        │     POST /agent/tool/exec → cowiki
        │       { agent_id, tool_name: "create_wiki", args: {...} }
        │     ← { success: true, result: {...} }
        │
        ├── Loop continues until: LLM returns stop, max_rounds hit, or error
        │
        └── Save session → sessions/sess-003.jsonl
              Report new session to cowiki if created
```

### Shutdown

```
cowiki server stop
  │
  ├── For each running agent:
  │     SIGTERM → agent process
  │     Wait 5s
  │     SIGKILL if still alive
  │
  └── Cleanup. Agent sessions persist on disk in agents/{name}/sessions/.
```

## Agent Directory Model

```
workspace/
  agents/
    compiler/                    ← Default agent (auto-created)
      agent.toml                 ← Cowiki-managed manifest
      SOUL.md                    ← Agent-owned (created by agent itself)
      sessions/                  ← Agent-owned
        sess-001.jsonl
        sess-002.jsonl
      prompts/                   ← Agent-owned
        compile-strategy.md
      settings.json              ← Agent-owned
    reviewer-1/                  ← User-created (on demand)
      agent.toml
      ...                        ← Different agents have different internals
```

**Cowiki's contract:** It owns `agent.toml` only. Everything else inside the agent directory is managed by the agent runtime. Cowiki never reads or writes `SOUL.md`, `sessions/`, `prompts/` — those are the agent's private domain.

### agent.toml Schema

```toml
[agent]
name = "compiler"               # Unique name within this workspace
type = "piagent"                # Runtime type — tells cowiki which binary to launch
task = "shallow_compile"        # Task type — determines tools and routing
```

Three fields. No tools config (injected by task). No model config (cowiki manages). No storage config (agent directory IS the storage).

### Agent Home Directory Redirection

Each agent runtime has a default data directory. Cowiki overrides it:

| Agent Runtime | Default Home | Cowiki Redirect |
|---------------|-------------|-----------------|
| PiAgent | `~/.pi/` | `workspace/agents/{name}/` via `COWIKI_AGENT_HOME` env var |
| OpenClaw | `~/.openclaw/` | `workspace/agents/{name}/` via `COWIKI_AGENT_HOME` env var |
| Rust agents | Crate-specific | `workspace/agents/{name}/` via `COWIKI_AGENT_HOME` env var |

The agent runtime decides what to store inside its home directory. Cowiki provides the path and stays out of the way.

## Agent Runtime Location

Agent binaries are NOT stored in the repo. They live in `COWIKI_DATA_DIR`:

```
$COWIKI_DATA_DIR/
  agents/
    piagent/                    ← PiAgent runtime (shared across all workspaces)
      package.json
      package-lock.json
      node_modules/
      dist/
```

**Download on startup:** If `COWIKI_DATA_DIR/agents/piagent/` is missing or outdated, cowiki fetches the runtime (git clone + npm install for PiAgent, cargo install for Rust agents). The runtime is cached and reused across all workspaces. Only per-repo state (sessions, SOUL.md) lives in `agents/{name}/`.

## Protocol

### Endpoints Exposed by Agent Process

Cowiki calls these:

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/agent/run` | POST | Dispatch task → agent thinking loop |
| `/agent/health` | GET | Liveness check |

### Endpoints Exposed by Cowiki Server

Agent process calls these:

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/agent/register` | POST | Handshake: agent reports identity + existing sessions |
| `/agent/llm/stream` | POST | LLM proxy: agent sends messages, cowiki streams back assistant events (SSE) |
| `/agent/tool/exec` | POST | Tool execution: agent requests tool, cowiki executes and returns result |
| `/agent/session/created` | POST | Agent notifies cowiki of new session |

### Request/Response Types

**AgentRequest (cowiki → agent):**
```json
{
  "task_type": "shallow_compile",
  "session_id": "sess-003",
  "system_prompt": "You are a wiki compiler...",
  "user_input": "compile sources/webapp-docs-site/",
  "tools": [
    { "name": "ls_sources", "description": "...", "parameters": {...} }
  ],
  "workspace_path": "/path/to/workspace",
  "config": { "max_rounds": 20, "token_budget": 100000 }
}
```

**LlmStreamRequest (agent → cowiki):**
```json
{
  "agent_id": "compiler-a1b2c3d4",
  "messages": [
    { "role": "system", "content": "You are a wiki compiler..." },
    { "role": "user", "content": "compile sources/webapp-docs-site/" }
  ],
  "config": { "model": "claude-sonnet-4-6", "max_tokens": 8192 }
}
```

Cowiki appends its own system instructions, calls Anthropic/OpenAI with its API key, and streams SSE events back.

**ToolCallRequest (agent → cowiki):**
```json
{
  "agent_id": "compiler-a1b2c3d4",
  "tool_name": "create_wiki",
  "args": { "title": "Docker Networking", "body": "...", "path": "wiki/infra/docker-networking.md" }
}
```

Cowiki executes the tool against the workspace git repo and returns the result.

### Session Routing

Cowiki maintains a lightweight session index — only routing metadata, never content:

```
SessionIndex (in-memory, cowiki runtime):
  compiler:
    sess-001 → { label: "compiled docker docs", created_at: ..., last_active: ... }
    sess-002 → { label: "review PR #45",       created_at: ..., last_active: ... }
  reviewer-1:
    sess-001 → { label: "initial review",       created_at: ..., last_active: ... }
```

- Agent reports sessions at registration (handshake) and on creation (`/agent/session/created`)
- Cowiki stores only `{id, label, created_at, last_active}` — enough to populate a session picker in the UI
- Content lives in agent's `sessions/*.jsonl` files. Cowiki never reads them.

### Task Type → Tool Injection

Tools are defined in code, not configuration. Cowiki injects tools based on `agent.toml`'s `task` field:

| Task Type | Tools Injected |
|-----------|---------------|
| `shallow_compile` | `ls_sources`, `read_manifest`, `ls_source_dir`, `read_source`, `create_wiki`, `edit_wiki`, `read_wiki`, `rm_wiki`, `create_entity`, `edit_entity`, `read_entity`, `create_concept`, `edit_concept`, `read_concept` |
| `deep_compile` | `read_wiki`, `read_entity`, `read_concept`, `edit_wiki`, `edit_entity`, `ls_wiki`, `ls_entities`, `ls_concepts`, `rm_wiki` |
| `review_submission` | Reserved |
| `search` | Reserved |

This is implemented as `tools_for_task(task: &str) -> Vec<ToolDef>` in `crates/agents/src/tools.rs`. Adding a new task type means adding a new match arm — no config changes needed.

## Default Agent

On first workspace open (or first compile request), cowiki auto-creates the default agent:

```
agents/compiler/agent.toml:
  [agent]
  name = "compiler"
  type = "piagent"
  task = "shallow_compile"
```

If the directory already exists, no action. Users create additional agents (e.g., `reviewer-1` for `review_submission`) through the Web UI or by creating `agents/<name>/agent.toml` manually.

## Concurrency & Pool

Reuses the existing `AgentPool` from `crates/core/src/compiler/pool.rs`:

- **ShallowCompile**: Multiple concurrent requests. Queued if pool full. 503 if queue exhausted.
- **DeepCompile**: Per-space mutex (one lint run per space). 409 if already running.
- **Agent process**: One process per agent directory. The pool manages parallel task dispatch to the same process — the agent runtime (PiAgent) handles internal concurrency.

Tier gating from existing protocol:
| Tier | ShallowCompile | DeepCompile |
|------|---------------|-------------|
| Free | 1 | 1 |
| Pro | 4 | 1 |
| Enterprise | 16 | 1 |

## Security

### Handshake Token

Cowiki generates a random 32-byte shared secret at process start. The agent must echo this token in its `/agent/register` call. Mismatch → process terminated. This prevents rogue processes from impersonating agents.

### Localhost-Only (MVP)

All communication is over `127.0.0.1`. The existing `AgentClient::validate_harness_url` enforces this. The agent process receives `COWIKI_SERVER_URL=http://127.0.0.1:9400`.

Future: TLS + mutual auth for remote agents (not in scope).

### Tool Authorization

Tools are injected by cowiki based on task type. The agent cannot request tools it wasn't given. `ToolCallRequest` validation: tool name must be in the agent's registered tool set; otherwise reject with 403.

## Observability

Reuses the existing `CompileEvent` SSE stream from `crates/agents/src/events.rs`:

```
event: agent-start   { agent_id: "compiler-a1b2", session_id: "sess-003", task: "shallow_compile" }
event: llm-round     { round: 3, token_count: 4500 }
event: tool-call     { tool: "create_wiki", args: {...}, round: 3 }
event: tool-result   { tool: "create_wiki", success: true, summary: "Created wiki/infra/docker-networking.md" }
event: phase         { phase: "shallow", status: "completed", pages_count: 3 }
```

## Web UI

### Agent Management Page (`Settings → Agents`)

- Lists configured agents with status (running, error, stopped)
- Each agent card shows: name, type, task, session count, last active
- Actions: Start/Stop/Restart process, View sessions, Delete agent
- "Add Agent" button → form: name, type (dropdown), task (dropdown)

### Session Picker (in Compile Dialog)

When user triggers compile:
- Agent selector: which agent to route to (default: compiler)
- Session selector: existing session or [New session]
- Session shows label + last active time (from SessionIndex)

## File Changes Summary

### New files
| File | Purpose |
|------|---------|
| `crates/agents/src/tools.rs` | `tools_for_task()` — task→tool mapping |
| `crates/agents/src/server.rs` | `LlmStreamHandler` + `ToolHandler` traits; SSE streaming helpers |

### Modified files
| File | Changes |
|------|---------|
| `crates/agents/src/protocol.rs` | Add `LlmStreamRequest`, `ToolCallRequest`, `ToolCallResponse`, `SessionInfo`, `HandshakeResponse` |
| `crates/agents/src/registry.rs` | Add `agent_id` field; register sessions on handshake |
| `crates/core/src/compiler/pool.rs` | Wire to actual agent processes (currently has semaphores but no real agent integration) |
| `crates/core/src/compiler/shallow.rs` | Replace legacy `Compiler` with `AgentClient::run()` dispatch |
| `crates/server/src/routes/compile.rs` | Route through AgentClient + AgentPool instead of legacy Compiler |
| `crates/server/src/routes/mod.rs` | Add agent management routes |
| `crates/server/src/main.rs` | Agent process lifecycle on startup/shutdown |

### New routes
| Route | Purpose |
|-------|---------|
| `POST /agent/register` | Agent handshake |
| `POST /agent/llm/stream` | LLM proxy (SSE) |
| `POST /agent/tool/exec` | Tool execution callback |
| `POST /agent/session/created` | Session creation notification |
| `GET /api/agents` | List agents + status |
| `GET /api/agents/:name` | Agent detail |
| `PUT /api/agents/:name/config` | Update agent.toml |
| `PUT /api/agents/:name/soul` | Update SOUL.md |
| `POST /api/agents` | Create new agent |
| `DELETE /api/agents/:name` | Remove agent |

## Related

- [`docs/compile-system-design.md`](../compile-system-design.md) — Compile pipeline (ShallowCompile + DeepCompile)
- [`docs/plans/2026-06-07-extractor-design-proposal.md`](plans/2026-06-07-extractor-design-proposal.md) — File-level extractor design
- `crates/agents/` — Agent protocol crate (existing)
- `crates/core/src/compiler/` — Compiler orchestration (existing)
- [PiAgent (davidondrej/pi-agent)](https://github.com/davidondrej/pi-agent) — TypeScript agent runtime
- [A2A Protocol](https://developers.google.com/agent-to-agent) — Google Agent-to-Agent protocol reference
