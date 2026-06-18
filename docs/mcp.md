# cowiki MCP Server

cowiki 通过 **Model Context Protocol (MCP)** 向 AI Agent 暴露 wiki 操作能力。
Agent（pi / claude）通过 MCP 协议直接在 workspace 内读写页面、搜索知识。

## 架构

MCP server 是**独立进程**，内嵌 `cowiki-core` 直接操作 git repo 和数据库，
**不是 REST 代理**——工具调用不经过 cowiki-server HTTP API。

```
MCP Client ──→ rmcp-server (127.0.0.1:9380) ──→ WikiFsGateway ──→ git repo + Postgres
              (rmcp + hyper)                    (cowiki-core)
```

## 协议

- **Transport**: Streamable HTTP（MCP 协议 2025-03-26）
- **SDK**: rmcp v1.7
- **端点**: `http://127.0.0.1:9380/mcp/sse`
- **绑定**: 仅 loopback（`127.0.0.1`），不接受外部连接
- **认证**: 可选 `COWIKI_MCP_AUTH_TOKEN` 共享密钥

## 工具列表（5 个）

Agent 通过 pi-mcp-adapter 的 `directTools` 模式直接调用以下工具，无需 `cowiki_` 前缀：

| 工具 | 说明 | 关键参数 |
|------|------|----------|
| `cowiki_list` | 列出目录内容 | `dir`, `recursive`? |
| `cowiki_read` | 读取单个页面或源文件 | `path` |
| `cowiki_write` | 创建/更新页面（含 YAML frontmatter） | `path`, `body` |
| `cowiki_remove` | 删除页面或文件 | `path` |
| `cowiki_search` | 全文搜索（Postgres FTS） | `query`, `top_k`? |

所有工具接收可选的 `_workspace`、`_branch`、`_execution_id` 上下文参数，
由 pi 通过环境变量 `COWIKI_WORKSPACE`、`COWIKI_BRANCH`、`COWIKI_EXECUTION_ID` 自动注入。

### 工具约束

- `sources/` 目录只读——agent 不能写入或删除源文件
- workspace slug 经过验证，阻止路径遍历（`../`、`/`、`\`）
- 写入/删除操作自动记录到 `.cowiki/tracking.json`，供 compile 流程收集产出页面

## 快速开始

### 启动

```bash
# 1. 启动 cowiki server（REST API + 数据库）
cargo run -p cowiki-server

# 2. 启动 MCP server（独立进程）
COWIKI_DATA_DIR=./data COWIKI_MCP_AUTH_TOKEN=my-secret cargo run --bin cowiki-mcp
```

MCP server 通过 `COWIKI_DATA_DIR` 定位 git repo（默认 `./data`），
端口通过 `COWIKI_MCP_PORT` 设置（默认 `9380`）。

### 认证配置

当 `COWIKI_MCP_AUTH_TOKEN` 设置时，所有 MCP 请求必须携带 `Authorization: Bearer <token>`。
cowiki server 启动时会读取同一环境变量，并自动将其注入 pi agent 的 `.mcp.json` 配置中：

```json
{
  "mcpServers": {
    "cowiki": {
      "url": "http://127.0.0.1:9380/mcp/sse",
      "lifecycle": "keep-alive",
      "directTools": true,
      "headers": {
        "Authorization": "Bearer my-secret"
      }
    }
  }
}
```

若不设置 `COWIKI_MCP_AUTH_TOKEN`，MCP server 接受所有 loopback 请求（开发环境适用）。

## 配置

| 环境变量 | 默认值 | 说明 |
|----------|--------|------|
| `COWIKI_MCP_PORT` | `9380` | MCP server 端口（仅端口号，总是绑 `127.0.0.1`） |
| `COWIKI_DATA_DIR` | `./data` | Wiki repo 数据目录 |
| `COWIKI_MCP_AUTH_TOKEN` | （空） | 可选共享密钥，设置后强制 Bearer 认证 |

也支持 CLI 参数：

```bash
cargo run --bin cowiki-mcp -- --data-dir ./data --port 9380
```

## 故障排查

### MCP Client 连接失败

```bash
# 检查进程
pgrep -f cowiki-mcp

# 确认端口监听
ss -tlnp | grep 9380

# 手动测试（无认证时）
curl http://127.0.0.1:9380/mcp/sse
```

### 认证失败 (401)

确认 cowiki server 和 MCP server 使用了相同的 `COWIKI_MCP_AUTH_TOKEN`。
若 token 不匹配，检查 `.env` 文件中该环境变量是否一致。

### 搜索无结果

确认 Postgres FTS 索引已建立（migration 005），且 `pages` 表中有对应 workspace 的数据。
MCP search 使用 `plainto_tsquery('english', ...)` 进行全文匹配。

## 代码结构

```
crates/mcp/                  # 独立 MCP server crate
├── Cargo.toml               # rmcp + hyper + cowiki-core（直连 git/DB）
└── src/
    ├── main.rs              # CLI 入口 + 启动循环
    └── lib.rs               # 5 tools（#[tool_router]）+ AuthLayer + start_mcp_server()

crates/core/
└── src/
    └── gateway.rs           # WikiFsGateway — 5 primitives（list/read/write/remove/search）
                              # 含 workspace slug 验证 + FTS 搜索 SQL
```

MCP server 是 cowiki workspace 的成员 crate，通过 `cowiki-core` 直接操作 git repo 和数据库，
零 HTTP 代理开销。
