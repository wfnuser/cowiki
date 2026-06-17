# Agent Compile 架构重构 — Design Spec

**Date**: 2026-06-16
**Status**: Design (approved)
**Topic**: 去掉 gRPC 通信层和 Node.js runtime，重构为 stdin/stdout + MCP server 架构。引入 AgentPool 管理长驻 agent 生命周期，预留 remote agent 扩展。

---

## 问题概述

### 当前架构问题

1. **过度耦合 gRPC**：agent 通过 gRPC 双向流连接 server，ToolGateway + LlmProxy + AgentRegistry 三道 gRPC service 增加复杂度
2. **Node.js 中间层**：`third-parties/piagent/dist/index.js` 是额外的 runtime 依赖，需要 npm install，增加部署复杂度
3. **LLM 调用绕路**：agent → gRPC LlmProxy → 外部 API，增加一跳网络延迟，且 cowiki 集中管理所有 API key
4. **Agent 状态不可见**：前端无法看到 agent 当前在做什么（tool calls）
5. **无 agent 池化**：stream 模式 agent 管理散落在 AgentManager 内部，缺乏统一的生命周期抽象

### 目标

- 去掉 gRPC 通信层、LlmProxy、Node.js runtime
- pi 作为系统预装二进制，通过 stdin/stdout 进行任务调度
- MCP server 作为独立 crate，提供 wiki 操作（list/read/write/remove/search），HTTP/SSE 传输
- AgentPool 统一管理长驻 agent 生命周期（借鉴 Google AX 的 executor registry 模式）
- Server 通过 broadcast channel + SSE 向前端暴露 agent 状态事件
- 清理所有过时文档和代码

---

## 1. Crate 级架构

```
crates/
├── core/           ← 不变：WikiFsGateway, Compiler, embedder
├── db/             ← 不变
├── utils/          ← 不变
├── extractor/      ← 不变
├── agents/         ← 重构
│   ├── pool/       ← 新增：AgentPool（调度、生命周期）
│   ├── agent/      ← 新增：AgentHandle trait + StdioAgentHandle
│   ├── skills/     ← 不变（embedded skill defaults）
│   ├── stream/     ← 新增：AgentEvent + broadcast channel
│   ├── manager/    ← 瘦身：去掉 spawn/nodjs，OneShot 直接 spawn_and_wait，Stream 委托给 AgentPool
│   └── types/      ← 简化：去掉 gRPC 相关类型
├── mcp-server/     ← 新增 crate（替换 gRPC ToolGateway）
│   ├── tools/      ← list/read/write/remove/search 工具
│   ├── transport/  ← SSE transport (axum Router)
│   └── constraint.rs ← 3 层权限模型（从当前 tool_gateway.rs 移植）
└── server/         ← 简化
    ├── routes/     ← compile.rs 新增 mode 参数
    ├── grpc/       ← 整个删除
    └── main.rs     ← 去掉 gRPC server，注册 /mcp/sse + /api/agents/{ws}/events
```

### 清理清单

| 删除项 | 原因 |
|--------|------|
| `third-parties/` 整个目录 | pi 系统预装，不再需要内置 runtime |
| `crates/agents/src/harness/piagent.rs` | 不再需要 Node.js piagent 适配器 |
| `crates/agents/src/harness/mod.rs` (ThinkLoop/AgentRunner trait) | AgentHandle trait 替代 |
| `crates/server/src/grpc/tool_gateway.rs` | MCP server 替代 |
| `crates/server/src/grpc/llm_proxy.rs` | pi 用自己的 LLM |
| `crates/server/src/grpc/agent_registry.rs` | stdin/stdout 替代 bidi stream |
| `crates/server/src/grpc/` 整个目录 | gRPC 整体移除 |
| `proto/` | 不再需要 protobuf |
| `cowiki-mcp-server/` 整个目录 | 旧 TS MCP server，被 `crates/mcp-server` 替代 |

| 保留但修改 | 变更 |
|-----------|------|
| `crates/agents/src/manager/mod.rs` | 瘦身，去掉 nodjs spawn |
| `crates/agents/src/manager/process.rs` | 重写：`node dist/index.js` → `pi --no-session`，gRPC env → MCP env |
| `crates/server/src/main.rs` | 删除 gRPC server spawn，注册 /mcp/sse + /api/agents/:ws/events |
| `crates/server/src/routes/compile.rs` | task 结构简化，execution_id tracking，新增 mode 参数 |
| `scripts/e2e-compile-test.sh` | 更新测试流程，覆盖 pipeline/agent/auto 三种模式 |

---

## 2. AgentHandle trait

参照 Google AX `agent.Agent` 接口设计。AX 的核心 abstraction：

```go
type Agent interface {
    Connect(ctx, conversationID, execID, start, executor, outputHandler) error
    Close() error
}
```

对应到 Rust：

```rust
// crates/agents/src/agent/handle.rs

/// 抽象的 Agent 通信接口（对应 AX agent.Agent interface）
#[async_trait]
pub trait AgentHandle: Send + Sync {
    /// 连接 agent 并执行任务。agent 通过 output_handler 回调报告进度事件。
    /// 对应 AX: Agent.Connect(ctx, conversationID, execID, start, executor, outputHandler)
    async fn connect(
        &self,
        ctx: AgentContext,
        start: AgentStart,
        output_handler: Box<dyn Fn(AgentEvent) -> Result<(), AgentError> + Send>,
    ) -> Result<AgentState, AgentError>;

    /// 关闭 agent 释放资源
    async fn close(&self) -> Result<(), AgentError>;

    /// agent 类型标识
    fn agent_type(&self) -> &str;
}

/// 当前实现：通过 stdin/stdout 与 pi 进程通信
pub struct StdioAgentHandle {
    child: Mutex<Option<tokio::process::Child>>,
    reader_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    agent_home: PathBuf,
    config: StdioAgentConfig,
}

/// 预留：远程 agent（后续实现）
// pub struct RemoteAgentHandle {
//     http_client: reqwest::Client,
//     endpoint: String,
// }
```

### 两种模式 — 职责划分

| 模式 | 生命周期 | 管理者 | 说明 |
|------|---------|--------|------|
| **OneShot** | spawn → execute → exit | AgentManager::spawn_and_wait() | 每次新建 pi 进程，stdin 传入 task，stdout 解析结果 |
| **Stream** | 长驻进程，多任务复用 | AgentPool + registry | pool 维护长驻 agent，idle reaper 回收 |

**OneShot 不需要 AgentPool**，因为每次都是独立进程用完即销毁。"池化"只在需要复用 agent 时才有意义。

---

## 3. AgentPool

参照 AX executor 的 registry + event log 模式。AX 核心：

```go
// AX: defaultExecutor 持有 registry map[string]agent.Agent
func DefaultExecutor(eventLog EventLog, registry map[string]agent.Agent) agent.Executor
```

对应到 Rust：

```rust
// crates/agents/src/pool/mod.rs

/// AgentPool 管理 Stream 模式 agent 生命周期
/// 对应 AX executor.registry + EventLog
pub struct AgentPool {
    /// agent 注册表：name → PooledAgent
    /// 对应 AX: registry map[string]agent.Agent
    registry: Arc<RwLock<HashMap<String, PooledAgent>>>,

    /// 事件广播：所有 agent 的输出事件合并到此 channel
    event_tx: broadcast::Sender<AgentEvent>,

    /// 配置
    config: AgentPoolConfig,
}

struct PooledAgent {
    handle: Arc<dyn AgentHandle>,
    mode: AgentMode,
    status: AgentStatus,     // Starting | Idle | Active | Stopped
    last_active: Instant,
    agent_id: String,
}

impl AgentPool {
    /// 注册长驻 agent
    pub async fn register(&self, name: &str, handle: Arc<dyn AgentHandle>);

    /// 调度任务到长驻 agent（往 stdin 发 task，从 stdout 等结果）
    pub async fn dispatch(&self, name: &str, task: TaskRequest) -> Result<AgentResponse, AgentError>;

    /// 订阅 agent 事件（供 Server → SSE → 前端）
    pub fn subscribe_events(&self) -> broadcast::Receiver<AgentEvent>;

    /// 空闲回收（对应 AX 的 COMPLETED 状态管理）
    pub fn start_reaper(self: &Arc<Self>);
}
```

### Agent 状态事件（前端展示）

```rust
// crates/agents/src/stream/mod.rs

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum AgentEvent {
    TaskStarted    { agent: String, task_id: String },
    ToolStart      { agent: String, task_id: String, tool: String, input: serde_json::Value },
    ToolEnd        { agent: String, task_id: String, tool: String, success: bool, summary: String },
    TaskCompleted  { agent: String, task_id: String, success: bool, rounds: u32 },
    AgentStopped   { agent: String, reason: String },
}
```

Server 通过 SSE endpoint `/api/agents/{ws}/events` 推送给前端。前端可展示 "正在 write wiki/docker.md" 等。

---

## 4. MCP Server

替换当前 gRPC ToolGateway，提供与 CLI 一致的 wiki 操作能力。HTTP/SSE 传输。

### 暴露的工具

| 工具 | 对应 CLI 命令 | 参数 | 约束 |
|------|-------------|------|------|
| `list` | `cowiki list` | `dir` (wiki/entities/concepts/sources) | Layer 2/3 |
| `read` | `cowiki read` | `path` | Layer 2/3 |
| `write` | `cowiki write` | `path`, `body` | Layer 1/2 |
| `remove` | — | `path` | Layer 1/2 |
| `search` | `cowiki search` | `query` | Layer 1 |

**仅 5 个基础 wiki 原语。** `ingest` 和 `compile` 不暴露为 MCP tools（由 Server HTTP API 触发），避免递归。

### 预留扩展

- 后续可按需增加 tool，如 `submit`、`review`，但保持 MCP 为"agent 操作 wiki 的最小界面"

### 3 层约束模型（从当前 ToolGateway 移植）

```
check_permissions(tool, args, task_type, source_scope):
  1. tool in task_type.allowlist?      → Layer 1: tool gating
  2. args.dir in task_type.dir_scope?  → Layer 2: directory scope
  3. if dir is "sources":              → Layer 3: source scope
       path in source_scope?
  4. execute via WikiFsGateway
```

### 并发安全 — execution_id tracking

每个 dispatch 生成唯一 `execution_id`，MCP server 按 ID 隔离 tracking：

```rust
// MCP server 按 execution_id 追踪，防止并发场景串数据
tracking: RwLock<HashMap<String, ExecutionTracking>>

struct ExecutionTracking {
    written_pages: Vec<String>,
    removed_pages: Vec<String>,
}
```

```
pi 每次 MCP 调用时带上 execution_id：
  → POST /mcp/{session}/message  body: { ..., _meta: { execution_id } }

Server dispatch 完成后按 ID 获取：
  let tracking = state.mcp_server.take_tracking(&execution_id);
```

### pi 连接方式

pi 启动时获得 MCP server URL，通过 MCP SDK 连接：

```
pi --no-session
  → 连接 http://localhost:9380/mcp/sse
  → 自动发现所有 tools (list/read/write/remove/search)
  → /compile-skill 触发 → agent 调用 MCP tools 操作 wiki
```

---

## 5. Skills 管理

参照 AX 的 `skills.Discover()` + `SystemPrompt()` 模式。

### AgentManager 写入 skills

```
AgentManager::spawn_and_wait(name, task)
  │
  ├─ 1. task_type → 选择 skill 文件集
  │     compile      → SKILL.md, conventions.md, tools.md, patterns.md, efficiency.md
  │     deep-compile  → SKILL.md, lint-checks.md, tools.md, patterns.md
  │     review       → SKILL.md, tools.md, patterns.md
  │
  ├─ 2. 写入 .pi/skills/{name}/
  │     每个 skill 目录含 SKILL.md（YAML frontmatter: name, description + 指令）
  │     格式遵循 agentskills.io（与 AX 一致）
  │
  └─ 3. 写入 .pi/skills.json
        [{ "name": "compile", "description": "...", "dir": ".pi/skills/compile" }, ...]
```

### pi 自动加载（与 AX 一致）

```
pi --no-session 启动时:
  1. 读取 $PI_AGENT_HOME/.pi/skills.json
  2. 解析每个 skill 的 SKILL.md frontmatter
  3. 注入 <available_skills> 到 system prompt
  4. /compile-skill → 匹配 skill name → 加载 Body() 到上下文
```

### /compile-skill 参数格式

AgentManager 拼接 prompt，通过 stdin 发送：

```
/compile-skill 请帮我编译以下源文件:
- sources/article1.md
- sources/arxiv/paper2.md

workspace: {ws}
branch: {branch}
```

---

## 6. Compile 流程（端到端）

```
POST /api/workspaces/{ws}/compile { branch, mode? }
  │
  ├─ 0. mode 参数决定路径:
  │     CompileMode::Pipeline → 强制 pipeline
  │     CompileMode::Agent    → 强制 agent dispatch
  │     CompileMode::Auto     → complexity gate 自动选择（默认）
  │
  ├─ 1. load_state, list sources, detect changed files (不变)
  │
  ├─ Pipeline 路径（不变）:
  │   Step 1-4 → write pages → embed → DB upsert → save state
  │
  └─ Agent 路径（重构）:
      │
      ▼
  ┌─ AgentManager::spawn_and_wait("compiler", task) ──────────────┐
  │                                                                │
  │  1. 确保 agent home: agents/compiler/                          │
  │  2. 写入 models.json (from cowiki config.llm)                  │
  │  3. 写入 .pi/skills/{compile}/*.md                             │
  │  4. 生成 execution_id (UUID)                                   │
  │  5. Reset MCP tracking for execution_id                        │
  │  6. spawn: pi --no-session                                     │
  │     env: PI_AGENT_HOME=agents/compiler                         │
  │     env: COWIKI_MCP_URL=http://localhost:9380/mcp/sse          │
  │     env: COWIKI_WORKSPACE={ws}                                 │
  │     env: COWIKI_BRANCH={branch}                                │
  │     env: COWIKI_SOURCE_SCOPE=sources/a.md,sources/b.md         │
  │     env: COWIKI_EXECUTION_ID={uuid}                            │
  │  7. stdin → "/compile-skill 请编译 sources/xxx\n"             │
  │  8. stdout JSON-line 解析                                      │
  │     → tool_start/tool_end → broadcast.send(event)              │
  │     → 最后一行 TaskResult JSON                                 │
  │  9. wait for process exit                                      │
  │ 10. 获取 MCP tracking: take_tracking(execution_id)             │
  │     → written_pages, removed_pages                             │
  └────────────────────────────────────────────────────────────────┘
      │
      ▼
  ┌─ 后处理 ───────────────────────────────────────────────────────┐
  │  written_pages → read content → embed → DB upsert              │
  │  save compile state                                            │
  │  return CompileResponse                                        │
  └────────────────────────────────────────────────────────────────┘

  ┌─ SSE /api/agents/{ws}/events ──────────────────────────────────┐
  │  → 前端展示 "正在 write wiki/docker.md"                         │
  └────────────────────────────────────────────────────────────────┘
```

### AgentResponse 简化

Agent 不再需要汇报 `written_pages`（MCP server 自动追踪），只返回执行状态：

```rust
pub struct TaskResult {
    pub success: bool,
    pub rounds: u32,
    pub usage: UsageInfo,        // token 用量回传
    pub error: Option<String>,
}
```

token 用量聚合到 Server 的 `UsageResponse` 中。

### CompileRequest 新增 mode 参数

```rust
pub struct CompileRequest {
    pub branch: String,
    #[serde(default)]
    pub mode: Option<CompileMode>,
}

pub enum CompileMode {
    Auto,       // 默认：complexity gate 自动选择
    Pipeline,   // 强制走 fixed 4-step pipeline
    Agent,      // 强制走 agent dispatch
}
```

### e2e test 覆盖

```bash
# Pipeline 路径
curl -X POST /api/workspaces/test/compile \
  -d '{"branch":"draft","mode":"pipeline"}'

# Agent 路径
curl -X POST /api/workspaces/test/compile \
  -d '{"branch":"draft","mode":"agent"}'

# Auto（默认）
curl -X POST /api/workspaces/test/compile \
  -d '{"branch":"draft"}'
```

---

## 7. pi 的 LLM 配置

pi 使用自己的 LLM stream，不再经过 cowiki 的 LlmProxy。

### models.json（AgentManager 写入）

```json
{
  "providers": {
    "openai": {
      "baseUrl": "https://api.openai.com/v1",
      "api": "openai-completions",
      "models": [{ "id": "gpt-4o-mini" }]
    }
  }
}
```

- 初始阶段从 cowiki config.llm 生成
- 后续可替换为独立配置
- API key 通过环境变量 `OPENAI_API_KEY` 传入

### Token 用量回传

pi 的 TaskResult 中包含 `usage: {input_tokens, output_tokens}`。AgentManager 解析后聚合到 Server 的 `UsageResponse`。

---

## 8. Server 启动变更

### 当前

```
main():
  → spawn gRPC server (port 9400): ToolGateway + LlmProxy + AgentRegistry
  → axum server (port 9380): HTTP API
  → validate piagent at third-parties/piagent/dist/index.js
```

### 重构后

```
main():
  → axum server (port 9380):
      Router::new()
        .merge(http_api_routes())       ← 现有 HTTP API
        .merge(mcp_server.router())     ← /mcp/sse + /mcp/:session/message
        .route("/api/agents/:ws/events", get(agent_events))  ← SSE
  → validate "pi" binary in PATH (no more third-parties/)
```

只有一个端口，所有通信统一通过 axum。

---

## 9. 与 AX 的对照总结

| AX 概念 | cowiki 对应 |
|---------|-----------|
| `agent.Agent` interface | `AgentHandle` trait |
| `agent.LocalAgent` | `StdioAgentHandle` (stdin/stdout) |
| `agent.RemoteAgent` | 预留 `RemoteAgentHandle` (HTTP) |
| `executor.Executor` | `AgentPool::dispatch()` |
| `executor.registry map[string]Agent` | `AgentPool.registry: HashMap<String, PooledAgent>` |
| `executor.EventLog` | `broadcast::Sender<AgentEvent>` + SSE |
| `skills.Discover()` + `SystemPrompt()` | AgentManager 写入 `.pi/skills/` → pi 自动加载 |
| `proto.State (PENDING/COMPLETED/FAILED)` | `AgentStatus` enum |

---

## 10. Out of Scope（后续）

- Remote agent（`RemoteAgentHandle` + 远程部署）
- AgentPool 进程预启动池（warm pool）
- 前端 agent 状态面板 UI 设计
- Skill 热更新（无需重启 pi 进程）
- MCP server 的 `ingest`/`compile` tools
