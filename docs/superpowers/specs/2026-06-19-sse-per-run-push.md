# SSE 前端推流：Per-Session 方案

> 状态：设计讨论 | 日期：2026-06-19 | 更新：2026-06-22（run → session 语义统一）

## 背景

> **2026-06-22 更新**：run 概念已被 session 统一替代。一次 `AgentManager::execute()` = 一个 volatile session。
> Session 同时承载跟踪、SSE events、进程生命周期。详见 [Agent Infra Session 重构 spec](2026-06-22-agent-infra-session-redesign.md)。

## 问题

当前 cowiki 用 `tokio::sync::broadcast` 全局通道推送 agent 事件：

```
pi stdout → AgentEvent → broadcast::channel(256) → N 个 SSE handler 各自过滤 workspace
```

缺点：
- **无法 late join**：前端刷新后丢失历史事件
- **所有事件混在一起**：每个 SSE handler 要过滤 `workspace_matches`
- **lag 丢事件**：capacity 256，消费慢直接跳过，不做重发
- **无 session 生命周期**：fire-and-forget，没有 queued→running→done 状态

## 方案：Per-Session 直写

每个 agent 调用是一个 **Session**，有自己的事件历史和 SSE 客户端列表。

### 架构

```
AgentManager::execute() → 内部创建 Session { id, tracking, events, clients }
GET /api/sessions/{session_id}/events → SSE text/event-stream
```

```
                    ┌─────────────────────────────────┐
                    │         AgentManager             │
                    │    ┌─────────────────────────┐   │
  execute() ────────┤────│    SessionManager        │   │
                    │    │  sessions: HashMap<      │   │
                    │    │    SessionId → Session   │   │
                    │    │  >                       │   │
                    │    └─────────────────────────┘   │
                    │                                 │
                    │  1. session_manager.create()     │
                    │  2. spawn agent CLI (stdio)      │
                    │  3. work loop → emit_event()     │
                    └─────────────────────────────────┘
                                    │
                                    ▼
┌───────────────────────────────────────────────────────┐
│                    work loop                          │
│                                                       │
│  while line = stdout.next_line() {                   │
│    parse ACP → AgentEvent                            │
│         │                                             │
│         ▼                                             │
│    session_manager.emit_event(session_id, event)      │
│    ┌──────────────────────────────────┐              │
│    │ events.push(event)    // 历史回放 │              │
│    │ for tx in clients:               │              │
│    │   tx.try_send(frame)  // 实时推送 │              │
│    └──────────────────────────────────┘              │
│  }                                                    │
│  session_manager.complete(session_id, result)         │
│  or session_manager.fail(session_id, error)           │
└───────────────────────────────────────────────────────┘
```

### 核心数据结构

```rust
// crates/agents/src/manager/session.rs

pub struct Session {
    pub id: SessionId,
    pub agent_name: String,
    pub status: SessionStatus,          // Running | Succeeded | Failed
    pub tracking: ExecutionTracking,    // written/removed files (opt-in tracking)
    pub events: Vec<StampedEvent>,      // 历史事件（支持重放）
    pub clients: Vec<mpsc::Sender<SseFrame>>,  // 活跃 SSE 连接
    pub extra_args: HashMap<String, String>,
    pub created_at: Instant,
}

enum SessionStatus { Running, Succeeded, Failed }
```

### SSE 端点

```
GET /api/sessions/{session_id}/events
Header: Last-Event-ID: 5
```

处理流程：

1. **鉴权**：require workspace membership + session 归属校验（防跨 workspace 伪造）
2. **重放历史**：遍历 `events[]`，跳过 `id <= Last-Event-ID` 的
3. **判断终态**：若 session 已结束（Succeeded/Failed），发 terminal 事件后关闭流
4. **注册为活跃客户端**：`clients.push(mpsc::Sender)`
5. **循环推送**：`while rx.recv() { write_sse(event) }`

### SSE Channel 设计

> **注意**：此多 channel 方案已被 [ACP Agent Visibility spec](2026-06-19-acp-agent-visibility.md) 的简化设计替代。ACP 采用单一的 `data` channel 透传 `SessionUpdate` JSON（自描述事件），前端按 `sessionUpdate` 字段分派。仅保留 `id`（Last-Event-ID）和 `error`（session 级异常）两个额外 channel。此处保留原始设计备查。

| channel | 何时 | 数据 |
|---|---|---|
| `start` | session 开始 | `{session_id, agent, model}` |
| `agent` | agent 事件 | `{type: "TextDelta", delta: "..."}` |
| `stderr` | agent stderr | `{chunk: "..."}` |
| `error` | 致命错误 | `{code, message, retryable}` |
| `end` | session 结束 | `{status: "succeeded", code: 0}` |

### 对比

| | broadcast（当前） | per-session（新） |
|---|---|---|
| 事件隔离 | 所有 workspace 混合 | 每个 session 独立 |
| late join | ❌ | ✅ 重放 events[] |
| lag 处理 | 静默跳过 | mpsc 反压 |
| session 生命周期 | ❌ | ✅ Running→Succeeded/Failed |
| 清理 | 无 | session 结束后 30min TTL |
| SSE 端点 | `/agents/{ws}/events` | `/sessions/{session_id}/events` |

### 关键：去掉 broadcast channel

每个 SSE 连接持有独立的 `mpsc::Sender`，`emit_event()` 直接遍历所有 sender 发送。HTTP response 的 TCP buffer 本身提供反压——不需要中间队列。
