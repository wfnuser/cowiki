# Agent Infra Session 重构 — Design Spec

**Date**: 2026-06-22 | **Status**: Implemented
**Topic**: 重构 agent infra 架构：统一 session 语义、agent type 归属、dispatcher 桥接层、per-session 内存 tracking、去掉 MCP / 用 cowiki CLI

---

## 核心决策

### 1. agents crate = 纯 infra

`crates/agents` 完全不感知 cowiki 业务（compile/review/deep-compile），只提供通用 agent 执行能力：
spawn 进程、装 skills、发 prompt、session 管理、tracking。

### 2. core dispatcher = 桥接层

`core/src/dispatch.rs` 的 `AgentDispatcher` trait 做业务语义到 agent infra 的翻译：
"compile 任务" → agent_name="compiler" + skill="cowiki-compiler" + 语义 prompt。

### 3. 统一 Session 语义

一次 `AgentManager::execute()` = 一个挥发性 Session。Session 承载：
- opt-in file tracking（per-session 内存隔离）
- scoped token mint/verify
- 进程生命周期管理
- 不持久化（one-shot 模式）

### 4. 去掉 MCP，使用 cowiki CLI

Agent 不再通过 MCP 协议调用 wiki 操作，而是直接运行 `cowiki` CLI 命令
（`cowiki list`, `cowiki write`, `cowiki read` 等）。
CLI 通过 `COWIKI_BASE_URL` + `COWIKI_API_KEY` 环境变量认证，
server 的 HTTP API route 完成 git 写入后回调 SessionManager 的 tracking。

---

## Crate 级架构

```
crates/
├── agents/              ← 纯 agent infra（不感知 cowiki 业务）
│   ├── harness/         ← AgentHandle trait + PiAgentHandle + CopilotAgentHandle
│   ├── acp/             ← ACP JSON-RPC 2.0 协议客户端（Copilot 用）
│   ├── manager/         ← AgentManager + SessionManager + process lifecycle + skills
│   ├── types/           ← UsageInfo, AgentConfig, AgentResult
│   └── skills/          ← 内置 SKILL.md 常量
│
├── core/                ← 依赖 agents
│   ├── dispatch.rs      ← AgentDispatcher trait（桥接层）
│   ├── compile.rs       ← do_compile() + 语义 prompt 构建 + 后处理
│   └── ...
│
├── server/              ← DI 拼装 + HTTP routes
│   └── main.rs          ← AgentDispatcher impl；page routes 调 SessionManager tracking
│
└── ...
```

**依赖方向：**
- `agents` ← 无依赖（纯 infra）
- `core` ← 依赖 `agents`
- `server` ← 依赖 `agents` + `core`

---

## AgentManager API

```rust
// crates/agents/src/manager/mod.rs

impl AgentManager {
    pub async fn execute(
        &self,
        agent_name: &str,       // "compiler"
        prompt: &str,           // 最终 prompt（dispatcher 拼好）
        skills: &[&str],        // ["cowiki-compiler"]
        workspace: &str,
        extra_args: HashMap<String, String>,
    ) -> Result<AgentResult, AgentError>;
}
```

- `agent_name` — 对应 `agents/{name}/agent.toml` manifest
- `prompt` — dispatcher 拼好的完整 prompt（含 slash command）
- `skills` — 从 agents crate 内置常量加载并安装到 agent_home
- `extra_args` — 透传为环境变量
- AgentManager **不知道** task_type、compile/review 等业务概念

---

## SessionManager — Opt-in Tracking

```rust
// crates/agents/src/manager/session.rs

pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<SessionId, Arc<RwLock<Session>>>>>,
    ws_to_session: Arc<RwLock<HashMap<String, SessionId>>>,
}

pub struct Session {
    pub id: SessionId,
    pub agent_name: String,
    pub status: SessionStatus,          // Running | Succeeded | Failed
    pub tracking: ExecutionTracking,    // written_files, removed_files
    pub tracking_active: bool,
    pub tracked_workspace: Option<String>,
    pub extra_args: HashMap<String, String>,
    pub created_at: Instant,
}

impl SessionManager {
    pub fn new() -> Self;
    pub async fn create(agent_name: String, extra_args: HashMap<String, String>) -> SessionId;
    pub async fn complete(&self, id: &SessionId);
    pub async fn fail(&self, id: &SessionId, error: &str);

    // Opt-in tracking — enabled per session
    pub async fn start_tracking(&self, session_id: &SessionId, workspace: &str);
    pub async fn stop_tracking(&self, session_id: &SessionId) -> Option<ExecutionTracking>;

    // Called by server HTTP routes — only records if tracking is active
    pub async fn track_write(&self, workspace: &str, path: &str) -> bool;
    pub async fn track_remove(&self, workspace: &str, path: &str) -> bool;

    // Scoped token
    pub fn mint_scoped_token(&self, id: &SessionId) -> String;      // "cowiki_ses_{uuid}"
    pub fn verify_token(&self, token: &str) -> Option<SessionId>;

    pub async fn cleanup_stale(&self, ttl: Duration);
}
```

**Tracking 流程：**

```
execute()
  │ start_tracking(session_id, workspace)
  │
  ▼ agent 运行 → cowiki CLI → Server HTTP API
  │ POST /api/workspaces/{ws}/pages → write_page_ws()
  │ git write 成功后 → session_manager.track_write(ws, path)
  │ 有活跃 tracking session 就记录，没有就跳过（普通用户请求）
  │
execute() returns
  │ stop_tracking(session_id) → AgentResult.written_files
```

**Server route hook:** page write/delete route 在 git 操作成功后调用 `session_manager.track_write/track_remove`。

---

## AgentDispatcher trait（core 桥接层）

```rust
// crates/core/src/dispatch.rs

#[async_trait]
pub trait AgentDispatcher: Send + Sync {
    async fn dispatch(
        &self,
        agent_name: &str,
        prompt: &str,
        skills: &[&str],
        workspace: &str,
        extra_args: HashMap<String, String>,
    ) -> Result<AgentResult, String>;
}
```

**AgentDispatcher trait 的两个目的：**
1. 自动 harness 选择策略（pi vs copilot），通过内部 AgentManager 的 agent type resolution
2. 测试 mock

---

## AgentResult

```rust
pub struct AgentResult {
    pub success: bool,
    pub rounds: u32,
    pub usage: UsageInfo,
    pub written_files: Vec<String>,
    pub removed_files: Vec<String>,
    pub error: Option<String>,
}
```

---

## Compile 调用链

```
POST /api/workspaces/{ws}/compile { branch }
  │
  ▼
server::routes::compile::compile_ws()
  │ state.as_ref() as &dyn AgentDispatcher
  ▼
core::compile::do_compile(dispatcher, compiler, repo, ...)
  │
  ├─ 1. list sources, detect changed (hash compare)
  ├─ 2. build_compile_prompt(sources, ws, branch) → 语义 prompt
  ├─ 3. dispatcher.dispatch("compiler", &prompt, &["cowiki-compiler"], ws_slug, extra_args)
  │    │
  │    ├─ AgentDispatcher impl (server) 加 slash command → agent_manager.execute()
  │    │    ├─ session = session_manager.create(...)
  │    │    ├─ start_tracking(session_id, ws)
  │    │    ├─ mint scoped_token → COWIKI_API_KEY
  │    │    ├─ install skills → agent_home/skills/
  │    │    ├─ spawn agent (pi or copilot) with COWIKI_BASE_URL + COWIKI_API_KEY
  │    │    ├─ agent runs → cowiki CLI → server routes → track_write/track_remove
  │    │    ├─ stop_tracking(session_id) → ExecutionTracking
  │    │    └─ return AgentResult
  │    │
  │    └─ return AgentResult
  │
  ├─ 4. for path in result.written_files:
  │      body = repo.read_file(branch, path)
  │      embed body → DB upsert
  │
  └─ 5. save compile state, return CompileResponse
```

---

## Prompt 分层

| 层 | 位置 | 产出 |
|----|------|------|
| 语义 prompt | `core/compile.rs` | sources 列表 + 编译规则 + workspace/branch 上下文 |
| Agent prompt | core dispatcher impl | 语义 prompt 前加 slash command |
| Raw prompt | `agents/manager` | 直接传给 agent 进程 stdin/ACP |

---

## ACP SDK 用法

- **Copilot harness**：用 `agent-client-protocol` 0.15 SDK 做 ACP 握手（`initialize` → `session/new` → `session/prompt`），session/new 不含 MCP servers
- **Pi harness**：NDJSON RPC 模式（`--mode rpc`），agent 使用 `bash` + `read` tools 运行 `cowiki` CLI
- Pi harness 不再加载 pi-mcp-adapter extension、不写 `.mcp.json`

---

## Agent Skills

- skills 从 `crates/agents/skills/` 内置常量加载（`include_str!()`）
- `_tools.md` 改为 cowiki CLI 命令参考（替代 MCP tool 参考）
- `compiler/deep-compile/review` SKILL.md 引用 `cli/skills/cowiki-cli/commands.md`
- `install_skills(agent_home, skills)` 写入 `{agent_home}/skills/{name}/SKILL.md`

---

## Scoped Token

格式：`"cowiki_ses_{uuid}"`。通过 `COWIKI_API_KEY` 环境变量传给 agent CLI。
Server auth middleware 识别 `cowiki_ses_` 前缀作为内部 session 请求。
当前为简单前缀格式，后续可用 HMAC 签名。

---

## 关键设计原则

1. **git 是真相源**：tracking 只记 path，body 从 repo 读
2. **agents 不感知业务**：task_type 常量在 core
3. **volatile session**：one-shot session 不持久化
4. **per-session 隔离**：每个 session 独立的 tracking
5. **tracking 对普通用户透明**：无活跃 session 时 track_write 无操作

---

## Out of Scope（后续）

- Stream/long-lived session 支持
- SSE 前端推流（Agent events/visibility）
- 前端 Agent 状态面板 UI
- Skill 热更新
- HMAC 签名 scoped token
