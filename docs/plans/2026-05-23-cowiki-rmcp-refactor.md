# RALPLAN-DR: cowiki MCP Server — Consensus Implementation Plan

> **Historical note (2026-05-26)**: The paths in this plan refer to `crates/rmcp-server/` which has since been moved to `cowiki-mcp-server/` as an independent top-level crate. See `.omg/plans/cowiki-mcp-server-independence.md` for the current architecture.

**Date:** 2026-05-23 (revised from 2026-05-22)
**Mode:** DELIBERATE (完全重写：从手写协议迁移到官方 rmcp SDK)
**Status:** v2 — rmcp-powered architecture

---

## 1. Architecture Decision: rmcp SDK

### Previous Decision (v1 — REVOKED)
手写 JSON-RPC dispatch + SSE transport，使用 `rust-mcp-schema` 仅作类型层。

### New Decision (v2 — CURRENT)
使用 **rmcp v1.7.0**（`modelcontextprotocol/rust-sdk`）作为 MCP 协议层，**hyper** 作为 HTTP 传输层。

| 维度 | v1 (废弃) | v2 (当前) |
|------|-----------|-----------|
| MCP 协议 | 手写 ~1200 行 | `rmcp::StreamableHttpService` (~50 行) |
| HTTP 框架 | axum | **hyper** (via `TowerToHyperService`) |
| 端口 | 3000 (与主服务共用) | **9380** (独立进程) |
| Tool 定义 | 手写 HashMap dispatch | `#[tool_router(server_handler)]` 宏 |
| Session | 手写 DashMap | `rmcp::LocalSessionManager` |
| 代码量 | ~1200 行协议代码 | ~200 行工具逻辑 |

### Drivers
1. **不要手写** — 用户明确要求使用 rmcp 官方库，不写 JSON-RPC/SSE/session 胶水代码
2. **独立端口** — MCP 服务独立于主 HTTP API，互不干扰
3. **复用业务逻辑** — 工具处理器仍直接调用 `cowiki_core` / `cowiki_db`

---

## 2. Technical Architecture

### 2.1 Crate 结构

```
crates/rmcp-server/
├── Cargo.toml
│   ├── rmcp = { version = "1.7", features = ["server", "macros", "transport-streamable-http-server"] }
│   ├── hyper-util (TowerToHyperService)
│   ├── tokio
│   ├── cowiki-core
│   └── cowiki-db
├── src/
│   ├── main.rs              # hyper 启动, 端口 9380
│   ├── server.rs            # CowikiServer struct, 持有 AppState
│   └── tools/
│       ├── mod.rs           # 工具模块声明
│       ├── ingest.rs        # cowiki_ingest
│       ├── compile.rs       # cowiki_compile
│       ├── read.rs          # cowiki_read
│       ├── write.rs         # cowiki_write
│       ├── search.rs        # cowiki_search
│       ├── submit.rs        # cowiki_submit
│       ├── list.rs          # cowiki_list
│       ├── review_list.rs   # cowiki_review_list
│       ├── review_get.rs    # cowiki_review_get
│       └── review_decide.rs # cowiki_review_decide
```

### 2.2 数据流

```
MCP Client (VS Code / Claude)          rmcp-server (hyper :9380)
     │                                        │
     │── POST /mcp (initialize) ─────────────→│ StreamableHttpService
     │   Authorization: Bearer cw_xxx          │   ├─ auth (Bearer → User lookup)
     │←── Mcp-Session-Id + capabilities ──────│   └─ session created (LocalSessionManager)
     │                                        │
     │── POST /mcp (tools/call) ─────────────→│
     │   Mcp-Session-Id: xxx                  │   #[tool_router] dispatch
     │   params: {name:"cowiki_search", ...}   │   └─ CowikiServer::cowiki_search()
     │←── {result: [...]} ────────────────────│       └─ Compiler::embed() + DB::search()
```

### 2.3 核心模式

```rust
// server.rs — 持有业务状态
#[derive(Clone)]
pub struct CowikiServer {
    pub db: sqlx::PgPool,
    pub wiki_repo: Arc<WikiRepo>,
    pub compiler: Arc<Compiler>,
}

// 每个工具用 #[tool] 宏声明
#[tool_router(server_handler)]
impl CowikiServer {
    #[tool(description = "语义搜索 wiki 页面")]
    async fn cowiki_search(
        &self,
        Parameters(params): Parameters<SearchParams>,
    ) -> Result<CallToolResult, ErrorData> {
        // 直接调用 self.db, self.compiler...
    }
}

// main.rs — hyper 启动
let service = TowerToHyperService::new(
    StreamableHttpService::new(
        || Ok(CowikiServer::new(state)),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default(),
    )
);
let listener = TcpListener::bind("0.0.0.0:9380").await?;
loop {
    let (stream, _) = listener.accept().await?;
    let svc = service.clone();
    tokio::spawn(async move {
        Builder::new(TokioExecutor::default())
            .serve_connection(TokioIo::new(stream), svc).await
    });
}
```

---

## 3. Implementation Phases

### Phase 1: 脚手架 (预计 1h)

| Step | Task |
|------|------|
| 1.1 | 创建 `crates/rmcp-server/Cargo.toml`，添加 workspace 成员 |
| 1.2 | 创建 `src/main.rs` — hyper 启动 + StreamableHttpService |
| 1.3 | 创建 `src/server.rs` — CowikiServer struct |
| 1.4 | 验证：`GET /mcp` 返回 200，`POST /mcp` initialize 成功 |

### Phase 2: 工具迁移 (预计 2h)

| Step | Task |
|------|------|
| 2.1 | 将 10 个工具从手写 handler 迁移到 `#[tool]` 宏 |
| 2.2 | 复用现有 `cowiki_core` / `cowiki_db` 调用逻辑 |
| 2.3 | 验证：`tools/list` 返回 10 个工具，`tools/call` 全部可用 |

### Phase 3: 认证与清理 (预计 0.5h)

| Step | Task |
|------|------|
| 3.1 | Bearer Token 认证集成（复用 `cowiki_db::users::find_by_api_key`） |
| 3.2 | 从 `crates/server/` 移除旧 MCP 代码 |
| 3.3 | 更新 `Cargo.toml` workspace members |
| 3.4 | 端到端验证 |

---

## 4. 与主服务关系

```
:3000 — cowiki-server (axum)
  ├── /api/*        REST API (Web UI)
  ├── /api/auth/*   认证
  └── (MCP 已移除)

:9380 — cowiki-rmcp-server (hyper + rmcp)
  └── /mcp          MCP Streamable HTTP
       ├── POST     JSON-RPC 请求
       ├── GET      SSE 流
       └── DELETE   终止会话
```

两个进程共享：
- PostgreSQL (同一数据库)
- Git repo (同一 data_dir)
- 用户/API Key 认证体系
