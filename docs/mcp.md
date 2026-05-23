# cowiki MCP Server

cowiki 通过 **Model Context Protocol (MCP)** 向 AI Agent 暴露 wiki 功能。Agent 可以直接摄入源文档、编译页面、搜索知识、提交审核——全部通过 MCP 协议完成。

## 架构

MCP 服务是**独立进程**，作为 MCP→REST 代理：MCP 工具通过 HTTP 调用 cowiki-server 的 REST API，零业务逻辑重复。

```
MCP Client ──→ rmcp-server (:8080) ──→ cowiki-server REST API (:3000) ──→ core/db
              (rmcp + hyper)            (axum)
```

两个进程共享同一套认证（Bearer token 透传）。

## 协议版本

- **Transport**: Streamable HTTP（MCP 协议 2025-03-26）
- **SDK**: rmcp v1.7 (`modelcontextprotocol/rust-sdk`)
- **端点**: `http://localhost:8080/mcp`
- **认证**: `Authorization: Bearer <api_key>`

## 快速开始

### 启动服务

确保 cowiki-server 已在 :3000 运行：

```bash
# 终端 1: 启动 REST API
cargo run -p cowiki-server

# 终端 2: 启动 MCP 代理
cargo run -p cowiki-rmcp-server
```

MCP server 通过 `[mcp-server]` 配置段的 `api_url` 连接到后端 REST API（默认 `http://localhost:3000/`）。

> **注意**：如果没有 `cowiki.conf` 配置文件，两个服务均会使用环境变量作为默认值。MCP server 端口默认为 `8080`，后端 API 地址默认为 `http://localhost:3000/`。

### 获取 API Key

通过 cowiki-server REST API 注册：

```bash
curl -X POST http://localhost:3000/api/auth/register \
  -H "Content-Type: application/json" \
  -d '{"name":"my-agent","email":"agent@example.com"}'
# → {"api_key": "cw_xxxxxxxxxx", ...}
```

> Dynamic Client Registration (DCR) 计划中，尚未实现。

### 初始化 MCP 会话

```bash
curl -X POST http://localhost:8080/mcp \
  -H "Content-Type: application/json" \
  -H "Accept: application/json, text/event-stream" \
  -H "Authorization: Bearer cw_xxxxxxxxxx" \
  -d '{
    "jsonrpc": "2.0", "id": 1, "method": "initialize",
    "params": {
      "protocolVersion": "2025-03-26",
      "capabilities": {},
      "clientInfo": {"name": "my-agent", "version": "1.0"}
    }
  }'
```

响应包含 `Mcp-Session-Id` header。

### 调用工具

```bash
curl -X POST http://localhost:8080/mcp \
  -H "Content-Type: application/json" \
  -H "Accept: application/json, text/event-stream" \
  -H "Mcp-Session-Id: d56b1486-5495-4373-9a55-9544cdc30701" \
  -H "Authorization: Bearer cw_xxxxxxxxxx" \
  -d '{
    "jsonrpc": "2.0", "id": 2, "method": "tools/call",
    "params": {
      "name": "cowiki_search",
      "arguments": {"query": "machine learning", "limit": 5}
    }
  }'
```

> **注意**：非 `initialize` 请求也建议携带 `Authorization` header，MCP server 需要它来识别用户身份和解析个人空间分支。

---

## 端点规范

| 方法 | 路径 | 说明 |
|------|------|------|
| `POST` | `/mcp` | JSON-RPC 请求 |
| `GET` | `/mcp` | SSE 流（需 session） |
| `DELETE` | `/mcp` | 终止会话 |

### POST /mcp 请求头

| Header | 必填 | 说明 |
|--------|:--:|------|
| `Content-Type` | ✅ | `application/json` |
| `Accept` | ✅ | `application/json, text/event-stream` |
| `Authorization` | ✅ | `Bearer <api_key>`（所有请求均需） |
| `Mcp-Session-Id` | 非 `initialize` 时 | 初始化返回的 session ID |

### POST /mcp 响应规则

| 请求内容 | HTTP 状态 |
|----------|:--:|
| 纯通知（无 `id`） | `202` |
| `initialize` | `200` + `Mcp-Session-Id` header |
| 其他请求 | `200` + JSON-RPC 响应 |
| 认证失败 | `401` |
| 无效 session | `404` |

---

## 工具列表（10 个）

所有工具的 Rust 实现位于 `crates/rmcp-server/src/server.rs`，通过 `#[tool_router]` 宏注册。
工具调用后端 `crates/server/src/routes/` 下的对应 REST 端点。

### 知识操作

| 工具 | 后端 API | 说明 | 关键参数 |
|------|----------|------|------|
| `cowiki_ingest` | `POST /api/ingest` | 摄入源文档到个人空间 | `source_type`, `content`, `filename`? |
| `cowiki_compile` | `POST /api/compile` | 编译个人空间源文档为 wiki 页面 | （无） |
| `cowiki_read` | `GET /api/pages/{slug}` | 读取 main 空间页面 | `slug` |
| `cowiki_write` | `POST /api/pages` | 创建/编辑个人空间页面 | `slug`, `body`, `title`?, `summary`? |
| `cowiki_list` | `GET /api/pages` | 列出 main 空间所有页面 | （无） |
| `cowiki_search` | `GET /api/search` | 语义搜索 main 空间 | `query`, `limit`? |
| `cowiki_submit` | `POST /api/submit` | 提交个人页面到审核队列 | `page_slugs` |

> 所有工具无需传入 `branch` 参数 — 写入操作自动使用当前用户的个人空间 (`user/{user.id}`)，读取/搜索/列表默认使用 `main` 分支。

### 审核操作

| 工具 | 后端 API | 说明 | 关键参数 |
|------|----------|------|------|
| `cowiki_review_list` | `GET /api/reviews` | 待审核列表 | 无 |
| `cowiki_review_get` | `GET /api/reviews/{id}` | 审核详情+diff | `id` |
| `cowiki_review_decide` | `POST /api/reviews/{id}` | 批准/拒绝 | `id`, `action` |

---

## 典型工作流

```
贡献知识:  ingest → compile → read → write → submit
查询知识:  search → read → list
参与审核:  review_list → review_get → review_decide
```

---

## 错误码

| JSON-RPC Code | 含义 | 常见原因 |
|:------------:|------|------|
| `-32700` | JSON 解析错误 | 请求体格式错误 |
| `-32602` | 无效参数 | 工具参数类型/缺失 |
| `-32603` | 服务器内部错误 | 后端 API 异常、网络不通、404 等 |
| `-32001` | 未授权 / session 过期 | 缺少或无效 Authorization |
| `-32002` | 资源未找到 | slug 不存在 |
| `-32003` | 禁止访问 | 无权限操作 |

---

## 配置 MCP 客户端

### VS Code (mcp.json)

在项目根目录或 `~/.vscode/mcp.json` 中配置：

```json
{
  "servers": {
    "cowiki": {
      "url": "http://localhost:8080/mcp",
      "type": "http",
      "headers": {
        "Authorization": "Bearer cw_your_api_key_here"
      }
    }
  }
}
```

### Claude Desktop

```json
{
  "mcpServers": {
    "cowiki": {
      "url": "http://localhost:8080/mcp",
      "headers": {
        "Authorization": "Bearer cw_your_api_key_here"
      }
    }
  }
}
```

### 通用 MCP Client

- **端点**: `http://localhost:8080/mcp`
- **协议**: MCP 2025-03-26 (Streamable HTTP)
- **认证**: `Authorization: Bearer <api_key>`

---

## 故障排查

### MCP 工具返回 `-32603 API 404 Not Found`

**现象**：MCP server 日志中出现：
```
response error id=N error=ErrorData { code: ErrorCode(-32603),
  message: "API 404 Not Found: null", ... }
```

**原因**：`api_url` 配置中的尾部斜杠与路径拼接产生双斜杠 `//`，导致后端 404。

例如：`api_url = "http://localhost:3000/"` + 路径 `api/pages` → `http://localhost:3000//api/pages` → 404。

**解决**：确保 `[mcp-server]` 配置段中的 `api_url` **不带尾部斜杠**：
```toml
[mcp-server]
api_url = "http://localhost:3000"
```

MCP server 已在代码层面自动去除尾部斜杠，但建议配置文件中也保持一致。

### MCP Client 报 `fetch failed`

**现象**：VS Code 或其他 MCP 客户端报：
```
Error sending message to http://localhost:8080/: TypeError: fetch failed
```

**原因**：MCP server 进程未运行或已崩溃。

**解决**：
```bash
# 检查进程
pgrep -f cowiki-rmcp-server

# 重启
cargo run -p cowiki-rmcp-server
```

### 认证失败 (401)

确保所有 MCP 请求（包括 `initialize`）都携带 `Authorization: Bearer <api_key>` header。
API Key 可通过 `POST /api/auth/register` 获取。

### 后端连接失败

MCP server 启动日志会打印后端地址：
```
MCP server on 0.0.0.0:8080, backend: http://localhost:3000
```

确认 cowiki-server 在该地址正常运行：
```bash
curl http://localhost:3000/api/health
# 应返回: ok
```

---

## 代码结构

```
crates/
├── utils/              # 共享配置
│   └── src/lib.rs
├── rmcp-server/
│   ├── Cargo.toml      # rmcp + reqwest (无 cowiki-core/db)
│   └── src/
│       ├── main.rs     # hyper 启动 + 端口配置
│       └── server.rs   # MCP→REST proxy (10 工具 → HTTP API)
└── server/             # axum REST API (唯一业务逻辑源)
```

rmcp-server 不依赖 cowiki-core/db，每个 MCP 工具直接调用 `POST/GET http://localhost:3000/api/*`。

MCP 协议处理由 rmcp SDK 负责（JSON-RPC、SSE、session），工具通过 `#[tool_router(server_handler)]` 宏声明式绑定。

---

## 运行

### 配置

MCP 服务器与主服务共用 `cowiki.conf` 配置文件（通过 `cowiki-utils` crate），支持：

| 配置方式 | 优先级 |
|----------|--------|
| `COWIKI_MCP_PORT` 环境变量 | 最高 |
| `COWIKI_PORT` 环境变量 | 高 |
| `[server] port` in `cowiki.conf` | 中 |
| 默认值 | 8080 |

```bash
# 方式 1: 环境变量指定端口
COWIKI_MCP_PORT=9090 cargo run -p cowiki-rmcp-server

# 方式 2: 使用 cowiki.conf 中的 [server] port
cargo run -p cowiki-rmcp-server -- --config cowiki.conf

# 方式 3: 使用默认端口 8080
cargo run -p cowiki-rmcp-server

# 启动主服务器 (端口 3000)
cargo run -p cowiki-server
```

### 运行测试

```bash
# 共享配置 crate 测试
cargo test -p cowiki-utils

# MCP 服务器测试 (需要网络下载 rmcp)
cargo test -p cowiki-rmcp-server --test unit_tests
```
